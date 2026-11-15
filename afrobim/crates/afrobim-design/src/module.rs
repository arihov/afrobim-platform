//! The design and the option set as kernel elements, following the pattern of
//! `afrobim-schedule` (`add_programme`, `store_programme`, `load_programme`).
//!
//! # How a designed alignment is stored
//!
//! [`add_design`] adds **two** elements:
//!
//! - an `alignment` element, exactly as the app adds an imported alignment (category
//!   `afrobim_alignment::CATEGORY`, layer `Alignments`, hidden on the sheet, `data` = the
//!   serialised [`Alignment`] with its profile, named uniquely), so signs, markings, drainage,
//!   corridors and sheets find it by name or id as they do any other;
//! - an `alignment_design` element holding the **inputs** (PIs, PVIs) and the id of the
//!   alignment element, so the design can be edited again.
//!
//! [`store_design`] rebuilds the alignment from the edited inputs and replaces the alignment
//! element's data **in place**, keeping its id and name, so corridors and drainage elements that
//! refer to it stay linked (they are stale until refreshed, as when an alignment is re-imported).
//! A design that is impossible is refused and nothing in the document changes.

use std::collections::BTreeMap;

use afrobim_alignment::{format_station, plan_drawing, Alignment, CATEGORY as ALIGNMENT_CATEGORY};
use afrobim_corridor::surface_of;
use afrobim_cost::estimate_of;
use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt, Severity, SitePlacement};
use serde::{Deserialize, Serialize};

use crate::design::{AlignmentDesign, DesignedAlignment};
use crate::error::DesignError;
use crate::options::{compare_options, OptionsComparison, CompareInputs, CrossingFeature, OptionSet};

pub const DESIGN_CATEGORY: &str = "alignment_design";
pub const OPTIONS_CATEGORY: &str = "alignment_options";
pub const DESIGN_LAYER: &str = "Alignment design";
pub const ALIGNMENTS_LAYER: &str = "Alignments";

/// The `data` of an `alignment_design` element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignElement {
    pub design: AlignmentDesign,
    /// The alignment element built from the design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<ElementId>,
}

fn alignment_element_data(a: &Alignment) -> Result<(serde_json::Value, f64, f64, Option<SitePlacement>), DesignError> {
    let plan = plan_drawing(a).ok_or_else(|| DesignError::Document("The designed alignment has no length.".into()))?;
    let data = serde_json::to_value(a).map_err(|e| DesignError::Document(e.to_string()))?;
    let site = SitePlacement { x: plan.origin_x_m * 1000.0, y: plan.origin_y_m * 1000.0, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None };
    Ok((data, plan.drawing.extents.width().max(1.0), plan.drawing.extents.height().max(1.0), Some(site)))
}


/// Adds a design: the alignment element and the design element. Returns `(design id, alignment id)`.
/// Refused, with the document unchanged, if the design is impossible.
pub fn add_design(doc: &mut Document, design: &AlignmentDesign) -> Result<(ElementId, ElementId), DesignError> {
    let mut design = design.clone();
    let name = doc.unique_name(if design.horizontal.name.trim().is_empty() { "Designed alignment" } else { design.horizontal.name.trim() });
    design.horizontal.name = name.clone();
    let built = design.build()?;
    let (data, w, h, site) = alignment_element_data(&built.alignment)?;
    let aid = doc.add(ALIGNMENT_CATEGORY, &name, Pt::new(0.0, 0.0), w, h, ALIGNMENTS_LAYER, data);
    if let Some(e) = doc.find_mut(aid) {
        e.site = site;
    }
    if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == ALIGNMENTS_LAYER) {
        layer.visible = false;
    }
    let value = serde_json::to_value(DesignElement { design, alignment: Some(aid) }).map_err(|e| DesignError::Document(e.to_string()))?;
    doc.ensure_layer(DESIGN_LAYER);
    let did = doc.add(DESIGN_CATEGORY, &format!("{name} (design)"), Pt::new(0.0, 0.0), w, h, DESIGN_LAYER, value);
    let site = doc.find(aid).and_then(|a| a.site.clone());
    if let Some(e) = doc.find_mut(did) {
        e.site = site;
    }
    Ok((did, aid))
}

/// Replaces the design stored in a `alignment_design` element and rebuilds the linked alignment
/// element in place. The name of the alignment element is kept. Nothing changes if the design is impossible.
pub fn store_design(doc: &mut Document, id: ElementId, design: &AlignmentDesign) -> Result<DesignedAlignment, DesignError> {
    let current = load_design_element(doc, id)?;
    let mut design = design.clone();
    if let Some(a) = current.alignment.and_then(|a| doc.find(a)) {
        design.horizontal.name = a.name.clone();
    }
    let built = design.build()?;
    let (data, w, h, site) = alignment_element_data(&built.alignment)?;
    let value = serde_json::to_value(DesignElement { design, alignment: current.alignment }).map_err(|e| DesignError::Document(e.to_string()))?;
    if let Some(aid) = current.alignment {
        let e = doc.find_mut(aid).ok_or_else(|| DesignError::Document(format!("The alignment element {} of this design no longer exists.", aid.0)))?;
        e.data = data;
        e.width = w;
        e.height = h;
        e.site = site.clone();
    }
    let e = doc.find_mut(id).ok_or_else(|| DesignError::Document(format!("no element {}", id.0)))?;
    e.data = value;
    e.width = w;
    e.height = h;
    e.site = site;
    Ok(built)
}

fn load_design_element(doc: &Document, id: ElementId) -> Result<DesignElement, DesignError> {
    let e = doc.find(id).ok_or_else(|| DesignError::Document(format!("no element {}", id.0)))?;
    if e.category != DESIGN_CATEGORY {
        return Err(DesignError::Document(format!("element {} is a '{}', not an alignment design", id.0, e.category)));
    }
    serde_json::from_value(e.data.clone()).map_err(|err| DesignError::Document(format!("'{}' is not a valid design: {err}", e.name)))
}

/// Reads the design of an `alignment_design` element.
pub fn load_design(doc: &Document, id: ElementId) -> Result<AlignmentDesign, DesignError> {
    load_design_element(doc, id).map(|d| d.design)
}

/// The design elements in the document: id and name.
pub fn designs(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == DESIGN_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

// ---------------------------------------------------------------- option sets

/// Adds an option set element.
pub fn add_option_set(doc: &mut Document, set: &OptionSet) -> ElementId {
    let name = doc.unique_name(&set.name);
    let (w, h) = option_extents(set).map_or((100.0, 100.0), |(a, b)| (a, b));
    doc.ensure_layer(DESIGN_LAYER);
    doc.add(OPTIONS_CATEGORY, &name, Pt::new(0.0, 0.0), w, h, DESIGN_LAYER, serde_json::to_value(set).unwrap_or_default())
}

/// Replaces the option set stored in an element.
pub fn store_option_set(doc: &mut Document, id: ElementId, set: &OptionSet) -> Result<(), DesignError> {
    let (w, h) = option_extents(set).unwrap_or((100.0, 100.0));
    let e = doc.find_mut(id).ok_or_else(|| DesignError::Document(format!("no element {}", id.0)))?;
    if e.category != OPTIONS_CATEGORY {
        return Err(DesignError::Document(format!("element {} is a '{}', not an option set", id.0, e.category)));
    }
    e.data = serde_json::to_value(set).map_err(|er| DesignError::Document(er.to_string()))?;
    e.width = w;
    e.height = h;
    Ok(())
}

/// Reads the option set of an element.
pub fn load_option_set(doc: &Document, id: ElementId) -> Result<OptionSet, DesignError> {
    let e = doc.find(id).ok_or_else(|| DesignError::Document(format!("no element {}", id.0)))?;
    if e.category != OPTIONS_CATEGORY {
        return Err(DesignError::Document(format!("element {} is a '{}', not an option set", id.0, e.category)));
    }
    serde_json::from_value(e.data.clone()).map_err(|err| DesignError::Document(format!("'{}' is not a valid option set: {err}", e.name)))
}

/// Every alignment element of the document as a crossing feature of kind `alignment`, except
/// those whose names are listed.
pub fn crossing_features_from_alignments(doc: &Document, exclude: &[&str]) -> Vec<CrossingFeature> {
    doc.elements()
        .iter()
        .filter(|e| e.category == ALIGNMENT_CATEGORY && !exclude.contains(&e.name.as_str()))
        .filter_map(|e| {
            let a: Alignment = serde_json::from_value(e.data.clone()).ok()?;
            Some(CrossingFeature { name: e.name.clone(), kind: "alignment".into(), polyline: a.polyline(2.0) })
        })
        .collect()
}

/// Compares the options of a stored set, reading its surface and estimate (user rates) from the
/// document. `crossings` are the features to count; see [`crossing_features_from_alignments`].
pub fn compare_in_document(doc: &Document, id: ElementId, crossings: Vec<CrossingFeature>) -> Result<OptionsComparison, DesignError> {
    let set = load_option_set(doc, id)?;
    let tin = match set.surface {
        Some(s) => Some(surface_of(doc, s).ok_or_else(|| DesignError::Document(format!("element {} is not a surface in the document", s.0)))?),
        None => None,
    };
    let estimate = match set.estimate {
        Some(s) => Some(estimate_of(doc, s).ok_or_else(|| DesignError::Document(format!("element {} is not an estimate in the document", s.0)))?.estimate),
        None => None,
    };
    let inputs = CompareInputs { ground: tin.as_ref(), crossings, estimate: estimate.as_ref(), extra_quantities: BTreeMap::new() };
    Ok(compare_options(&set, &inputs))
}

// ---------------------------------------------------------------- drawing

/// A text label for the plan, for a front end that can draw text (the draw crate has glyph
/// outlines only). Positions are plan coordinates in metres.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLabel {
    pub x_m: f64,
    pub y_m: f64,
    pub text: String,
    pub kind: PlanLabelKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanLabelKind {
    /// `PI 3  1+234.567` at the PI.
    Pi,
    /// Curve data (radius, deflection, tangent, length, external) beside the curve centre side of the PI.
    CurveData,
    /// `TS 1+000.000`, `SC`, `CS`, `ST` at the points.
    CurvePoint,
}

/// The labels for the plan of a design: PI number and station, the curve data, and the TS/SC/CS/ST points.
/// Stations are `k+mmm.mmm` and lengths metres to two or three decimals, as the engine writes them;
/// a front end with display units uses [`plan_labels_with`].
pub fn plan_labels(design: &AlignmentDesign) -> Result<Vec<PlanLabel>, DesignError> {
    plan_labels_with(design, &|s| format_station(s, 3), None)
}

/// As [`plan_labels`] with the front end's own formatting: `station` writes a station in metres,
/// and `length`, when given, writes every length of the curve data (radius, tangent, length,
/// external, spirals; metres in, text with its unit out). With `length` `None` the lengths are
/// written as [`plan_labels`] writes them. Angles are degrees either way.
pub fn plan_labels_with(design: &AlignmentDesign, station: &dyn Fn(f64) -> String, length: Option<&dyn Fn(f64) -> String>) -> Result<Vec<PlanLabel>, DesignError> {
    let h = design.horizontal.solve()?;
    let mut out = Vec::new();
    for (i, p) in design.horizontal.pis.iter().enumerate() {
        let at = h.curve_at_pi(i).map(|c| c.pi_station_m);
        out.push(PlanLabel { x_m: p.x, y_m: p.y, text: match at { Some(s) => format!("PI {i}  {}", station(s)), None => format!("PI {i}") }, kind: PlanLabelKind::Pi });
    }
    for c in &h.curves {
        let p = design.horizontal.pis[c.pi_index];
        let dir = if matches!(c.turn, afrobim_alignment::Turn::Left) { "LT" } else { "RT" };
        let l = |metres: f64, decimals: usize| match length {
            Some(f) => f(metres),
            None => format!("{metres:.decimals$}"),
        };
        let mut text = format!("D={:.4} deg {dir}  R={}  T={}  L={}  E={}", c.deflection_rad.to_degrees(), l(c.radius_m, 2), l(c.tangent_in_m, 3), l(c.total_length_m, 3), l(c.external_m, 3));
        if c.spiral_in_m > 0.0 || c.spiral_out_m > 0.0 {
            text.push_str(&format!("  Ls={}/{}", l(c.spiral_in_m, 2), l(c.spiral_out_m, 2)));
        }
        out.push(PlanLabel { x_m: (p.x + c.centre.0) / 2.0, y_m: (p.y + c.centre.1) / 2.0, text, kind: PlanLabelKind::CurveData });
        let spirals = c.spiral_in_m > 0.0 || c.spiral_out_m > 0.0;
        let (a, b) = if spirals { ("TS", "ST") } else { ("PC", "PT") };
        out.push(PlanLabel { x_m: c.ts.0, y_m: c.ts.1, text: format!("{a} {}", station(c.ts_station_m)), kind: PlanLabelKind::CurvePoint });
        if spirals {
            out.push(PlanLabel { x_m: c.sc.0, y_m: c.sc.1, text: format!("SC {}", station(c.sc_station_m)), kind: PlanLabelKind::CurvePoint });
            out.push(PlanLabel { x_m: c.cs.0, y_m: c.cs.1, text: format!("CS {}", station(c.cs_station_m)), kind: PlanLabelKind::CurvePoint });
        }
        out.push(PlanLabel { x_m: c.st.0, y_m: c.st.1, text: format!("{b} {}", station(c.st_station_m)), kind: PlanLabelKind::CurvePoint });
    }
    Ok(out)
}

/// A line in plan metres: (x1, y1, x2, y2), width, colour.
type PlanLine = ([f64; 4], f64, Option<Colour>);

/// The plan of a design as primitives in plan metres (not yet translated or scaled): the PI
/// polyline in grey, the centreline in black, a cross at every PI in red and a small cross at
/// each curve centre in blue.
fn plan_primitives(design: &AlignmentDesign, centreline_colour: Option<Colour>) -> Result<Vec<PlanLine>, DesignError> {
    let built = design.horizontal.solve()?;
    let mut lines: Vec<PlanLine> = Vec::new();
    let pis = &design.horizontal.pis;
    for w in pis.windows(2) {
        lines.push(([w[0].x, w[0].y, w[1].x, w[1].y], 0.0, Some(Colour::Grey)));
    }
    let chord = built.alignment.min_radius().map_or(20.0, |r| (8.0 * r * 0.005).sqrt().clamp(0.25, 20.0));
    for w in built.alignment.polyline(chord).windows(2) {
        lines.push(([w[0].0, w[0].1, w[1].0, w[1].1], 0.0, centreline_colour));
    }
    let size = built.alignment.length() / 200.0;
    let arm = size.clamp(1.0, 25.0);
    for p in pis {
        lines.push(([p.x - arm, p.y, p.x + arm, p.y], 0.0, Some(Colour::Red)));
        lines.push(([p.x, p.y - arm, p.x, p.y + arm], 0.0, Some(Colour::Red)));
    }
    for c in &built.curves {
        let a = arm / 2.0;
        lines.push(([c.centre.0 - a, c.centre.1, c.centre.0 + a, c.centre.1], 0.0, Some(Colour::Blue)));
        lines.push(([c.centre.0, c.centre.1 - a, c.centre.0, c.centre.1 + a], 0.0, Some(Colour::Blue)));
    }
    Ok(lines)
}

fn to_drawing(lines: Vec<PlanLine>) -> Drawing {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for l in &lines {
        for (x, y) in [(l.0[0], l.0[1]), (l.0[2], l.0[3])] {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if !min_x.is_finite() {
        return Drawing::default();
    }
    let mm = |x: f64, y: f64| Pt2::new((x - min_x) * 1000.0, (y - min_y) * 1000.0);
    let primitives = lines.iter().map(|l| Primitive::Line { from: mm(l.0[0], l.0[1]), to: mm(l.0[2], l.0[3]), width: l.1, colour: l.2 }).collect();
    Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: mm(max_x, max_y) } }
}

fn option_extents(set: &OptionSet) -> Option<(f64, f64)> {
    let d = option_drawing(set).ok()?;
    Some((d.extents.width().max(1.0), d.extents.height().max(1.0)))
}

const OPTION_COLOURS: [Colour; 5] = [Colour::Black, Colour::Blue, Colour::Red, Colour::Green, Colour::Brown];

fn option_drawing(set: &OptionSet) -> Result<Drawing, DesignError> {
    let mut all = Vec::new();
    for (i, o) in set.options.iter().enumerate() {
        if let Ok(lines) = plan_primitives(&o.design, Some(OPTION_COLOURS[i % OPTION_COLOURS.len()])) {
            // Only the centreline in the option's colour: drop the PI furniture for the overview.
            all.extend(lines.into_iter().filter(|l| l.2 == Some(OPTION_COLOURS[i % OPTION_COLOURS.len()])));
        }
    }
    if all.is_empty() {
        return Err(DesignError::Document("none of the options can be drawn".into()));
    }
    Ok(to_drawing(all))
}

fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

/// The module for `alignment_design` elements: draws the designed alignment in plan with its PIs
/// (red crosses) and curve centres (blue crosses) and reports the curve data.
///
/// `derived`: `pis`, `curves`, `length_m`, `min_radius_m`, `start_station`, `end_station`,
/// `plan_origin_x_m`, `plan_origin_y_m` (plan coordinates of the drawing origin, as for
/// alignments), and one entry `Curve PI n` per curve with the figures as text. Labels come from
/// [`plan_labels`]. An impossible design gives Error notes in plain words and an empty drawing.
#[derive(Debug, Clone, Copy, Default)]
pub struct DesignModule;

impl Module for DesignModule {
    fn category(&self) -> &str {
        DESIGN_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Alignment designs"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let data: DesignElement = serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}' is not a valid alignment design: {e}", element.name))?;
        let built = match data.design.build() {
            Ok(b) => b,
            Err(e) => {
                let notes = match &e {
                    DesignError::Impossible(ps) => ps.iter().map(|p| note(Severity::Error, "design.impossible", p.message.clone())).collect(),
                    other => vec![note(Severity::Error, "design.impossible", other.to_string())],
                };
                return Ok(Generated { notes, ..Generated::default() });
            }
        };
        let lines = plan_primitives(&data.design, Some(Colour::Black)).map_err(|e| e.to_string())?;
        let origin = lines.iter().flat_map(|l| [(l.0[0], l.0[1]), (l.0[2], l.0[3])]).fold((f64::INFINITY, f64::INFINITY), |a, p| (a.0.min(p.0), a.1.min(p.1)));
        let drawing = to_drawing(lines);
        let a = &built.alignment;
        let mut derived = BTreeMap::new();
        derived.insert("pis".to_string(), data.design.horizontal.pis.len().to_string());
        derived.insert("curves".to_string(), built.horizontal.curves.len().to_string());
        derived.insert("length_m".to_string(), format!("{:.3}", a.length()));
        derived.insert("min_radius_m".to_string(), a.min_radius().map_or("none".into(), |r| format!("{r:.3}")));
        derived.insert("start_station".to_string(), format_station(a.start_station_m, 3));
        derived.insert("end_station".to_string(), format_station(a.end_station_m(), 3));
        derived.insert("plan_origin_x_m".to_string(), format!("{:.6}", origin.0));
        derived.insert("plan_origin_y_m".to_string(), format!("{:.6}", origin.1));
        for c in &built.horizontal.curves {
            derived.insert(
                format!("Curve PI {}", c.pi_index),
                format!(
                    "deflection {:.4} deg, R {:.3} m, Ls {:.3}/{:.3} m, T {:.3}/{:.3} m, L {:.3} m, E {:.3} m, M {:.3} m, TS {}, ST {}",
                    c.deflection_rad.to_degrees(),
                    c.radius_m,
                    c.spiral_in_m,
                    c.spiral_out_m,
                    c.tangent_in_m,
                    c.tangent_out_m,
                    c.total_length_m,
                    c.external_m,
                    c.mid_ordinate_m,
                    format_station(c.ts_station_m, 3),
                    format_station(c.st_station_m, 3)
                ),
            );
        }
        let notes = built.warnings.iter().map(|w| note(Severity::Warning, "design.warning", w.clone())).collect();
        Ok(Generated { drawing, notes, derived, solids: Vec::new() })
    }
}

/// The module for `alignment_options` elements: draws every buildable option's centreline in plan,
/// each in its own colour in the order of the set (black, blue, red, green, brown, then again), and
/// notes the options that cannot be built.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptionSetModule;

impl Module for OptionSetModule {
    fn category(&self) -> &str {
        OPTIONS_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Alignment options"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let set: OptionSet = serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}' is not a valid option set: {e}", element.name))?;
        let mut notes = Vec::new();
        for o in &set.options {
            if let Err(e) = o.design.build() {
                notes.push(note(Severity::Error, "options.unbuildable", format!("Option '{}' cannot be built: {e}", o.name)));
            }
        }
        let drawing = option_drawing(&set).unwrap_or_default();
        let mut derived = BTreeMap::new();
        derived.insert("options".to_string(), set.options.len().to_string());
        derived.insert("criteria".to_string(), set.criteria.len().to_string());
        Ok(Generated { drawing, notes, derived, solids: Vec::new() })
    }
}

/// Registers both modules.
pub fn register(modules: &mut afrobim_kernel::ModuleRegistry) {
    modules.register(Box::new(DesignModule));
    modules.register(Box::new(OptionSetModule));
}
