// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Designed alignments in the app: making one from PIs, editing it, and keeping every module that
//! stands on it in step.
//!
//! The engine (`afrobim-design`) holds the geometry. A design is two elements, an `alignment`
//! element (the ordinary one every module reads) and an `alignment_design` element (the PIs and
//! PVIs). Every edit here goes through `store_design`, which rewrites the alignment element in
//! place, so its id and name do not change, and then resynchronises what stands on it.
//!
//! Nothing here holds an engineering value. A PI starts with no radius and no spiral, a profile
//! has no K and no grade limit, and a limits table is whatever the user loads.

use std::path::Path;

use afrobim_alignment::{Alignment, Pi, SignLocation, SignSide, TrafficSide};
use afrobim_design::{self as design, AlignmentDesign, CheckReport, DesignBasis, DesignElement, HorizontalDesign, HorizontalSolution, VPvi, VerticalDesign};
use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

use crate::{alignment_glue, check_inputs, markings_glue, modules_glue, signs_glue};

/// The property an alignment keeps its check inputs under (read by CHECKALL).
pub const CHECK_KEY: &str = "check.alignment";

/// What was brought up to date after an edit, and what could not be.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resync {
    /// One line per kind of element that was resynchronised.
    pub lines: Vec<String>,
    /// Elements now out of range or refused, with the reason the module gave.
    pub problems: Vec<String>,
    /// The engine's warnings about the design itself (a profile that stops short, and so on).
    pub warnings: Vec<String>,
}

impl Resync {
    /// The short note for the status line and the command history.
    pub fn note(&self) -> String {
        let mut text = if self.lines.is_empty() { "Nothing else stands on this alignment.".to_string() } else { format!("Resynchronised: {}.", self.lines.join("; ")) };
        if !self.problems.is_empty() {
            text.push_str(&format!(" Out of range or refused: {}.", self.problems.join("; ")));
        }
        text
    }
}

fn message(e: design::DesignError) -> String {
    e.to_string()
}

/// The data of a design element.
pub fn design_element(doc: &Document, id: ElementId) -> Option<DesignElement> {
    let e = doc.find(id).filter(|e| e.category == design::DESIGN_CATEGORY)?;
    serde_json::from_value(e.data.clone()).ok()
}

/// The design element that built this alignment element, if it was designed.
pub fn design_of_alignment(doc: &Document, alignment: ElementId) -> Option<ElementId> {
    doc.elements().iter().filter(|e| e.category == design::DESIGN_CATEGORY).find(|e| design_element(doc, e.id).and_then(|d| d.alignment) == Some(alignment)).map(|e| e.id)
}

/// The alignment element of a design element.
pub fn alignment_of_design(doc: &Document, design_id: ElementId) -> Option<ElementId> {
    design_element(doc, design_id)?.alignment.filter(|a| doc.find(*a).is_some())
}

/// Every design in the document: design element, alignment element, name.
pub fn designs(doc: &Document) -> Vec<(ElementId, ElementId, String)> {
    design::designs(doc).into_iter().filter_map(|(id, _)| Some((id, alignment_of_design(doc, id)?, doc.find(alignment_of_design(doc, id)?)?.name.clone()))).collect()
}

/// A design element for a selection that may be the alignment, its design element or nothing.
pub fn design_for_selection(doc: &Document, selected: Option<ElementId>) -> Option<ElementId> {
    let e = doc.find(selected?)?;
    if e.category == design::DESIGN_CATEGORY {
        Some(e.id)
    } else if e.category == alignment_glue::CATEGORY {
        design_of_alignment(doc, e.id)
    } else {
        None
    }
}

/// The design itself, or why it cannot be read.
pub fn load(doc: &Document, design_id: ElementId) -> Result<AlignmentDesign, String> {
    design::load_design(doc, design_id).map_err(message)
}

fn hide_design_layers(doc: &mut Document) {
    for layer in doc.layers_mut().iter_mut() {
        if layer.name == design::DESIGN_LAYER || layer.name == design::ALIGNMENTS_LAYER {
            layer.visible = false;
        }
    }
}

// ─── making a design ────────────────────────────────────────────────────────────────────

/// A new designed alignment from the PIs clicked or typed. Every PI starts as an angle point
/// with no radius and no spiral: the engineer enters those in the PI table.
pub fn create(doc: &mut Document, name: &str, start_station_m: f64, points: &[(f64, f64)]) -> Result<(ElementId, ElementId), String> {
    if name.trim().is_empty() {
        return Err("Give the alignment a name.".into());
    }
    if points.len() < 2 {
        return Err("An alignment needs at least two PIs, a start and an end.".into());
    }
    let pis: Vec<Pi> = points.iter().map(|(x, y)| Pi::point(*x, *y)).collect();
    let horizontal = HorizontalDesign::new(name.trim(), start_station_m, pis);
    let ids = design::add_design(doc, &AlignmentDesign::new(horizontal, None)).map_err(message)?;
    hide_design_layers(doc);
    Ok(ids)
}

// ─── edits ──────────────────────────────────────────────────────────────────────────────

/// An edit of the plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HEdit {
    MovePi(usize, f64, f64),
    InsertPi(usize, f64, f64),
    DeletePi(usize),
    SetRadius(usize, f64),
    SetSpirals(usize, f64, f64),
}

/// Applies a plan edit to a copy of the design, without touching the document. The error is the
/// engine's reason, in its own words.
pub fn try_horizontal(design: &AlignmentDesign, edit: HEdit) -> Result<(AlignmentDesign, HorizontalSolution), String> {
    let mut d = design.clone();
    let solution = match edit {
        HEdit::MovePi(i, x, y) => d.horizontal.move_pi(i, x, y),
        HEdit::InsertPi(i, x, y) => d.horizontal.insert_pi(i, Pi::point(x, y)),
        HEdit::DeletePi(i) => d.horizontal.delete_pi(i),
        HEdit::SetRadius(i, r) => d.horizontal.set_radius(i, r),
        HEdit::SetSpirals(i, a, b) => d.horizontal.set_spirals(i, a, b),
    }
    .map_err(message)?;
    Ok((d, solution))
}

/// Applies a plan edit and stores it. A refused edit returns the engine's reason and leaves the
/// document exactly as it was.
pub fn edit_horizontal(doc: &mut Document, design_id: ElementId, edit: HEdit) -> Result<Resync, String> {
    let current = load(doc, design_id)?;
    let (changed, _) = try_horizontal(&current, edit)?;
    store(doc, design_id, &changed)
}

/// An edit of the profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VEdit {
    /// A PVI on the profile as it is, at this station.
    InsertAt(f64),
    InsertPvi(usize, f64, f64),
    MovePvi(usize, f64, f64),
    DeletePvi(usize),
    CurveLength(usize, f64),
    CurveK(usize, f64),
    NoCurve(usize),
}

pub fn try_vertical(design: &VerticalDesign, edit: VEdit) -> Result<VerticalDesign, String> {
    let mut v = design.clone();
    match edit {
        VEdit::InsertAt(s) => v.insert_pvi_at_station(s),
        VEdit::InsertPvi(i, s, z) => v.insert_pvi(i, VPvi::point(s, z)),
        VEdit::MovePvi(i, s, z) => v.move_pvi(i, s, z),
        VEdit::DeletePvi(i) => v.delete_pvi(i),
        VEdit::CurveLength(i, l) => v.set_curve_length(i, l),
        VEdit::CurveK(i, k) => v.set_curve_k(i, k),
        VEdit::NoCurve(i) => v.set_curve(i, design::CurveSpec::None),
    }
    .map_err(message)?;
    Ok(v)
}

/// Applies a profile edit and stores it. Refused edits change nothing.
pub fn edit_vertical(doc: &mut Document, design_id: ElementId, edit: VEdit) -> Result<Resync, String> {
    let mut d = load(doc, design_id)?;
    let v = d.vertical.clone().ok_or("This alignment has no profile yet. Add PVIs to start one.")?;
    d.vertical = Some(try_vertical(&v, edit)?);
    store(doc, design_id, &d)
}

/// Replaces the whole profile (a new one from clicked PVIs, or an accepted fit to ground).
pub fn set_vertical(doc: &mut Document, design_id: ElementId, vertical: Option<VerticalDesign>) -> Result<Resync, String> {
    let mut d = load(doc, design_id)?;
    if let Some(v) = &vertical {
        v.solve().map_err(message)?;
    }
    d.vertical = vertical;
    store(doc, design_id, &d)
}

/// Writes a whole design (both parts) and resynchronises. Refused designs change nothing.
pub fn store(doc: &mut Document, design_id: ElementId, d: &AlignmentDesign) -> Result<Resync, String> {
    let built = design::store_design(doc, design_id, d).map_err(message)?;
    let Some(alignment) = alignment_of_design(doc, design_id) else { return Err("The alignment element of this design is missing.".into()) };
    let mut r = resync(doc, alignment);
    r.warnings = built.warnings;
    Ok(r)
}

// ─── resynchronising ────────────────────────────────────────────────────────────────────

/// Puts the signs placed by station back where the alignment now puts them. A sign whose
/// station is beyond the alignment now keeps its position and is listed.
fn replace_signs(doc: &mut Document, name: &str, alignment: &Alignment) -> (Vec<ElementId>, Vec<String>) {
    let placed: Vec<(ElementId, String, f64, f64)> = doc
        .elements()
        .iter()
        .filter(|e| e.category == signs_glue::CATEGORY)
        .filter_map(|e| {
            let s = e.site.as_ref()?;
            (s.alignment.as_deref() == Some(name)).then(|| Some((e.id, e.name.clone(), s.station_m?, s.offset_m?)))?
        })
        .collect();
    let (mut moved, mut problems) = (Vec::new(), Vec::new());
    for (id, mark, station, offset) in placed {
        let location = SignLocation { alignment: Some(name.to_string()), station: Some(station), offset: Some(offset.abs()), side: Some(if offset >= 0.0 { SignSide::Right } else { SignSide::Left }) };
        match afrobim_alignment::place(alignment, &location, TrafficSide::Left) {
            Ok(p) => {
                if let (Some(pos), Some(e)) = (p.positions.first(), doc.find_mut(id)) {
                    let mut site = pos.site_placement(name);
                    site.offset_m = Some(offset);
                    e.site = Some(site);
                    moved.push(id);
                }
            }
            Err(e) => problems.push(format!("{mark} at {}: {e}", afrobim_alignment::format_station(station, 3))),
        }
    }
    (moved, problems)
}

/// Brings everything that stands on an alignment up to date after its geometry or profile
/// changed: placed signs and their supports, markings, drainage, gantries, lighting, restraint,
/// pavement and corridors. Sheets compose from the model when they are opened, so they need
/// nothing stored.
pub fn resync(doc: &mut Document, alignment_id: ElementId) -> Resync {
    let mut out = Resync::default();
    let Some(name) = doc.find(alignment_id).map(|e| e.name.clone()) else { return out };
    let Some(alignment) = alignment_glue::alignment_of(doc, alignment_id) else { return out };
    let (moved, mut problems) = replace_signs(doc, &name, &alignment);
    let mut supports = 0;
    for id in &moved {
        supports += afrobim_supports::sync_support_sites(doc, *id).unwrap_or(0);
    }
    if !moved.is_empty() {
        out.lines.push(format!("{} sign(s) moved{}", moved.len(), if supports > 0 { format!(" with {supports} support element(s)") } else { String::new() }));
    }
    let count = |doc: &Document, categories: &[&str]| doc.elements().iter().filter(|e| categories.contains(&e.category.as_str())).count();
    let markings = count(doc, &[markings_glue::CATEGORY]);
    markings_glue::sync(doc);
    // Markings whose stations now run past the ends of the alignment.
    let (first, last) = (alignment.start_station_m, alignment.end_station_m());
    for e in doc.elements().iter().filter(|e| e.category == markings_glue::CATEGORY) {
        if let Ok(def) = afrobim_markings::MarkingDefinition::from_value(&e.data) {
            if def.alignment == name && (def.start_station_m < first - 1e-6 || def.end_station_m > last + 1e-6) {
                problems.push(format!(
                    "{} runs from {} to {} but the alignment now runs from {} to {}",
                    e.name,
                    afrobim_alignment::format_station(def.start_station_m, 3),
                    afrobim_alignment::format_station(def.end_station_m, 3),
                    afrobim_alignment::format_station(first, 3),
                    afrobim_alignment::format_station(last, 3)
                ));
            }
        }
    }
    if markings > 0 {
        out.lines.push(format!("{markings} road marking(s)"));
    }
    let road = count(
        doc,
        &[afrobim_drainage::DRAIN_CATEGORY, afrobim_drainage::CULVERT_CATEGORY, afrobim_gantry::GANTRY_CATEGORY, afrobim_lighting::CATEGORY, afrobim_restraint::CATEGORY, afrobim_pavement::PAVEMENT_CATEGORY, afrobim_corridor::CORRIDOR_CATEGORY],
    );
    problems.extend(modules_glue::sync_all(doc));
    if road > 0 {
        out.lines.push(format!("{road} drainage, gantry, lighting, restraint, pavement or corridor element(s)"));
    }
    out.problems = problems;
    out
}

// ─── checks against a limits table ──────────────────────────────────────────────────────

/// Where the limits of an alignment check come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LimitSource {
    /// The user's own limits file (the only source before the built-in basis existed, so a stored
    /// selection without a source reads as this).
    #[default]
    UserFile,
    /// The built-in Uganda MoWT Road Design Manual Vol. I, January 2010.
    Builtin,
    /// The built-in row with the limits the user's file states laid over it (`StandardRow::overlay`).
    BuiltinWithOverlay,
}

impl LimitSource {
    pub const ALL: [LimitSource; 3] = [LimitSource::Builtin, LimitSource::UserFile, LimitSource::BuiltinWithOverlay];

    pub fn label(self) -> &'static str {
        match self {
            LimitSource::Builtin => "Built-in: MoWT Road Design Manual Vol. I, January 2010",
            LimitSource::UserFile => "My limits file",
            LimitSource::BuiltinWithOverlay => "Built-in with my file laid over it",
        }
    }
}

/// The selection made from `builtin_bases()`. Every field starts empty: class, terrain, design
/// speed and emax are the engineer's choice.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BuiltinChoice {
    pub class: Option<design::mowt::RoadClass>,
    pub terrain: Option<design::mowt::Terrain>,
    pub speed_kmh: Option<f64>,
    pub emax_percent: Option<f64>,
    /// False: the desirable maximum gradient; true: the absolute maximum.
    pub absolute_grade: bool,
    /// The manual's 0.5 % minimum gradient in cuttings, applied to the whole alignment.
    pub minimum_grade_in_cut: bool,
}

impl BuiltinChoice {
    /// The engine's selection, or what is still to be chosen.
    pub fn selection(&self) -> Result<design::BuiltinSelection, String> {
        Ok(design::BuiltinSelection {
            class: self.class.ok_or("choose the road class")?,
            terrain: self.terrain.ok_or("choose the terrain")?,
            design_speed_kmh: self.speed_kmh.ok_or("choose the design speed")?,
            emax_percent: self.emax_percent.ok_or("choose the maximum superelevation emax")?,
            options: design::MowtOptions {
                grade: if self.absolute_grade { design::mowt::GradeLimit::Absolute } else { design::mowt::GradeLimit::Desirable },
                minimum_grade_in_cut: self.minimum_grade_in_cut,
            },
        })
    }
}

/// What `ALIGNCHECK` stores on the alignment (`check.alignment`), and `CHECKALL` reads back.
/// Selections stored before the built-in basis was offered have only the two first fields and
/// read as "my limits file".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AlignCheckInputs {
    /// Path of the limits file (CSV or JSON).
    pub limits_path: String,
    /// The label of the design speed row chosen.
    pub row_label: String,
    #[serde(default)]
    pub source: LimitSource,
    #[serde(default)]
    pub builtin: BuiltinChoice,
}

/// The id of the one built-in basis.
pub const BUILTIN_ID: &str = "mowt_2010";

/// The limits an `AlignCheckInputs` asks for: the basis and the label of the row to check.
pub fn resolve_basis(inputs: &AlignCheckInputs) -> Result<(DesignBasis, String), String> {
    match inputs.source {
        LimitSource::UserFile => {
            if inputs.limits_path.trim().is_empty() {
                return Err("no limits table is loaded".into());
            }
            Ok((load_basis(Path::new(inputs.limits_path.trim()))?, inputs.row_label.clone()))
        }
        LimitSource::Builtin => {
            let built = DesignBasis::from_builtin(BUILTIN_ID, &inputs.builtin.selection()?).map_err(message)?;
            let label = built.rows[0].label.clone();
            Ok((built, label))
        }
        LimitSource::BuiltinWithOverlay => {
            let mut built = DesignBasis::from_builtin(BUILTIN_ID, &inputs.builtin.selection()?).map_err(message)?;
            let user = load_basis(Path::new(inputs.limits_path.trim()))?;
            let user_row = user.row(&inputs.row_label).ok_or_else(|| format!("choose the row of your limits file to lay over the built-in limits ('{}' is not in it)", inputs.row_label))?;
            let merged = built.rows[0].overlay(user_row);
            built.name = format!("{} overlaid with {}", built.name, user.name);
            built.source = format!("{}. Laid over it, wherever the file states a value: {}", built.source, user.source);
            let label = merged.label.clone();
            built.rows = vec![merged];
            Ok((built, label))
        }
    }
}

pub fn stored_check(doc: &Document, alignment: ElementId) -> Option<AlignCheckInputs> {
    check_inputs::load(doc, alignment, CHECK_KEY)
}

pub fn store_check(doc: &mut Document, alignment: ElementId, inputs: &AlignCheckInputs) -> bool {
    check_inputs::save(doc, alignment, CHECK_KEY, inputs)
}

/// Reads a limits table from a CSV or JSON file.
pub fn load_basis(path: &Path) -> Result<DesignBasis, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    basis_from_text(&text, path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("json")))
}

pub fn basis_from_text(text: &str, json: bool) -> Result<DesignBasis, String> {
    if json { DesignBasis::from_json_str(text) } else { DesignBasis::from_csv_str(text) }.map_err(message)
}

/// The columns of a limits file, in the engine's names.
pub const LIMIT_COLUMNS: [&str; 20] = [
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

/// A blank limits template: the column headers and no values. It carries the two metadata keys
/// with nothing after them, and the engine refuses it until the user has entered a source.
pub fn blank_template() -> String {
    let mut headers = LIMIT_COLUMNS.to_vec();
    headers.push("headlight_beam_deg");
    format!("#name,\n#source,\n{}\n", headers.join(","))
}

/// The explanation shown when no limits source is chosen yet.
pub const NO_LIMITS_TEXT: &str = "No limits source is chosen. Choose the built-in Uganda MoWT Road Design Manual Vol. I: Geometric Design (January 2010), with your class, terrain, design speed and emax; or load a limits table (CSV or JSON) that names its source (document, edition, table, page); or lay your table over the built-in one. Until then every check reads Not checked. No value is taken from memory.";

/// Said wherever the built-in limits are used: the copy of the manual is not the Ministry's own.
pub const THIRD_PARTY_NOTE: &str = "The copy of the manual used for the built-in limits was hosted by a third party and was not compared with a Ministry original. The edition is the one the document gives itself (January 2010).";


/// Runs the engine's checks of a design against one row of a limits table.
pub fn run_check(design: &AlignmentDesign, basis: &DesignBasis, row_label: &str) -> Result<CheckReport, String> {
    let row = basis.row(row_label).ok_or_else(|| format!("The limits table has no row labelled '{row_label}'."))?;
    let built = design.build().map_err(message)?;
    Ok(design::run_checks(basis, row, Some(&built.horizontal), built.vertical.as_ref(), &design::CheckOptions::default()))
}

/// The check of one designed alignment as `CHECKALL` reports it: a status, a reason and lines.
pub enum AlignmentCheck {
    InputNeeded(String),
    NotChecked(String),
    Report(CheckReport),
    Failed(String),
}

pub fn check_for_report(doc: &Document, alignment: ElementId) -> Option<AlignmentCheck> {
    let design_id = design_of_alignment(doc, alignment)?;
    let Some(stored) = stored_check(doc, alignment) else {
        return Some(AlignmentCheck::InputNeeded("no limits file or design speed is stored for this alignment: open ALIGNCHECK, load your limits table and choose the design speed".into()));
    };
    if stored.source == LimitSource::UserFile && stored.limits_path.trim().is_empty() {
        return Some(AlignmentCheck::NotChecked("no limits source is chosen, so no limit could be applied: open ALIGNCHECK and choose the built-in MoWT manual or load a limits table".into()));
    }
    let (basis, row_label) = match resolve_basis(&stored) {
        Ok(b) => b,
        Err(e) => return Some(AlignmentCheck::InputNeeded(format!("the stored limits cannot be used ({e}): open ALIGNCHECK and choose them again"))),
    };
    let design = match load(doc, design_id) {
        Ok(d) => d,
        Err(e) => return Some(AlignmentCheck::Failed(e)),
    };
    Some(match run_check(&design, &basis, &row_label) {
        Ok(r) => AlignmentCheck::Report(r),
        Err(e) => AlignmentCheck::InputNeeded(format!("{e}: open ALIGNCHECK and choose the design speed row")),
    })
}

/// The lines of a report, one per item, in words.
pub fn report_lines(report: &CheckReport) -> Vec<String> {
    let mut lines = vec![format!("Limits: {} (source: {}), row {}", report.basis_name, report.source, report.row_label)];
    for i in &report.items {
        let value = i.value.map_or("n/a".to_string(), |v| format!("{v:.3}"));
        let limit = i.limit.map_or("none".to_string(), |v| format!("{v:.3}"));
        lines.push(format!("{} {}: {} (value {value} {}, limit {limit}; {}) {:?}. {}", i.id, i.location, i.description, i.unit, i.limit_basis, i.status, i.message));
    }
    lines
}

// ─── export ─────────────────────────────────────────────────────────────────────────────

/// LandXML text of a designed alignment.
pub fn landxml(doc: &Document, design_id: ElementId) -> Result<String, String> {
    load(doc, design_id)?.to_landxml(&design::LandXmlOptions::default()).map_err(message)
}

// ─── option sets ────────────────────────────────────────────────────────────────────────

pub fn option_sets(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == design::OPTIONS_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

pub fn load_set(doc: &Document, id: ElementId) -> Result<design::OptionSet, String> {
    design::load_option_set(doc, id).map_err(message)
}

pub fn store_set(doc: &mut Document, id: ElementId, set: &design::OptionSet) -> Result<(), String> {
    design::store_option_set(doc, id, set).map_err(message)
}

/// A new, empty option set. The surface, the estimate and the criteria are the user's to choose.
pub fn new_set(doc: &mut Document, name: &str) -> Result<ElementId, String> {
    if name.trim().is_empty() {
        return Err("Name the option set.".into());
    }
    let id = design::add_option_set(doc, &design::OptionSet { name: name.trim().to_string(), ..Default::default() });
    hide_design_layers(doc);
    Ok(id)
}

/// Adds the current state of a design to a set as a new option with this name.
pub fn add_option(doc: &mut Document, set_id: ElementId, design_id: ElementId, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name the option.".into());
    }
    let mut set = load_set(doc, set_id)?;
    if set.options.iter().any(|o| o.name == name) {
        return Err(format!("There is already an option called '{name}'."));
    }
    let mut d = load(doc, design_id)?;
    d.horizontal.name = name.to_string();
    if let Some(v) = d.vertical.as_mut() {
        v.name = name.to_string();
    }
    set.options.push(design::AlignmentOption { name: name.to_string(), design: d, corridor: None, note: String::new() });
    store_set(doc, set_id, &set)
}

pub fn duplicate_option(doc: &mut Document, set_id: ElementId, index: usize) -> Result<(), String> {
    let mut set = load_set(doc, set_id)?;
    let src = set.options.get(index).cloned().ok_or("No such option.")?;
    let mut name = format!("{} copy", src.name);
    let mut n = 2;
    while set.options.iter().any(|o| o.name == name) {
        name = format!("{} copy {n}", src.name);
        n += 1;
    }
    let mut copy = src;
    copy.design.horizontal.name = name.clone();
    if let Some(v) = copy.design.vertical.as_mut() {
        v.name = name.clone();
    }
    copy.name = name;
    set.options.insert(index + 1, copy);
    store_set(doc, set_id, &set)
}

pub fn delete_option(doc: &mut Document, set_id: ElementId, index: usize) -> Result<(), String> {
    let mut set = load_set(doc, set_id)?;
    if index >= set.options.len() {
        return Err("No such option.".into());
    }
    set.options.remove(index);
    store_set(doc, set_id, &set)
}

pub fn rename_option(doc: &mut Document, set_id: ElementId, index: usize, name: &str) -> Result<(), String> {
    let name = name.trim();
    let mut set = load_set(doc, set_id)?;
    if name.is_empty() {
        return Err("Name the option.".into());
    }
    if set.options.iter().enumerate().any(|(i, o)| i != index && o.name == name) {
        return Err(format!("There is already an option called '{name}'."));
    }
    let o = set.options.get_mut(index).ok_or("No such option.")?;
    o.name = name.to_string();
    o.design.horizontal.name = name.to_string();
    if let Some(v) = o.design.vertical.as_mut() {
        v.name = name.to_string();
    }
    store_set(doc, set_id, &set)
}

/// Makes an option the current design: its PIs and PVIs replace those of the design element, which
/// keeps its alignment element and name, and everything on the alignment is resynchronised.
pub fn make_current(doc: &mut Document, set_id: ElementId, index: usize, design_id: ElementId) -> Result<Resync, String> {
    let set = load_set(doc, set_id)?;
    let option = set.options.get(index).ok_or("No such option.")?;
    store(doc, design_id, &option.design)
}

/// Gives every option the typical section of an existing corridor (cut, fill and land take need
/// one). The corridor's own definition is copied; nothing is assumed.
pub fn set_corridor_from(doc: &mut Document, set_id: ElementId, corridor: Option<ElementId>) -> Result<(), String> {
    let mut set = load_set(doc, set_id)?;
    let def = match corridor {
        Some(id) => Some(afrobim_corridor::corridor_element(doc, id).ok_or("That corridor cannot be read.")?.definition),
        None => None,
    };
    for o in &mut set.options {
        o.corridor = def.clone();
    }
    store_set(doc, set_id, &set)
}

/// The comparison of a set, with the document's alignments as crossing features (the design's own
/// alignments are left out) and its own surface and estimate.
pub fn compare(doc: &Document, set_id: ElementId) -> Result<design::OptionsComparison, String> {
    let set = load_set(doc, set_id)?;
    let names: Vec<&str> = set.options.iter().map(|o| o.name.as_str()).collect();
    let mut exclude: Vec<&str> = names.clone();
    let designed: Vec<String> = designs(doc).into_iter().map(|d| d.2).collect();
    exclude.extend(designed.iter().map(|s| s.as_str()));
    let crossings = design::crossing_features_from_alignments(doc, &exclude);
    design::compare_in_document(doc, set_id, crossings).map_err(message)
}

// ─── demo ───────────────────────────────────────────────────────────────────────────────

/// A small designed alignment and a set of two options, for `--demo`.
///
/// **Every number is an arbitrary demonstration value**: the PI coordinates, the start station,
/// the radii and spiral lengths, the PVI stations and levels, the vertical curve lengths. None is
/// a design value or from a standard. The PIs lie inside the made-up demo ground of
/// `road_glue::demo`, so the profile editor has a ground line to show.
pub fn demo(doc: &mut Document) -> Vec<String> {
    let mut problems = Vec::new();
    // Demo values: three PIs in plan, start station 5000 m.
    // The last interior PI has no radius on purpose: the plan and the table show "radius needed".
    let mut h = HorizontalDesign::new("Demo design A", 5000.0, vec![Pi::point(100.0, -60.0), Pi::point(330.0, 40.0), Pi::point(460.0, 330.0), Pi::point(420.0, 520.0)]);
    // Demo values: radius 250 m with spirals of 60 m at the one interior PI.
    h.pis[1].radius = 250.0;
    h.pis[1].spiral_in = 60.0;
    h.pis[1].spiral_out = 60.0;
    let length = match h.solve() {
        Ok(s) => s.alignment.length(),
        Err(e) => {
            problems.push(format!("Demo design: {e}"));
            return problems;
        }
    };
    let s0 = 5000.0;
    // Demo values: levels around 99 m to 103 m, a 120 m curve at the first interior PVI, K 40 at the second.
    let v = VerticalDesign::new(
        "Demo design A",
        vec![VPvi::point(s0, 99.0), VPvi::with_length(s0 + length * 0.35, 101.5, 120.0), VPvi::with_k(s0 + length * 0.7, 99.5, 40.0), VPvi::point(s0 + length, 101.0)],
    );
    let a = AlignmentDesign::new(h.clone(), Some(v.clone()));
    let (design_a, _) = match design::add_design(doc, &a) {
        Ok(ids) => ids,
        Err(e) => {
            problems.push(format!("Demo design: {e}"));
            return problems;
        }
    };
    hide_design_layers(doc);
    // Demo values: the alignment check is set to the built-in MoWT basis, class II, rolling
    // terrain, 70 km/h, emax 7 %, so that CHECKALL shows real statuses with their citations.
    if let Some(alignment) = alignment_of_design(doc, design_a) {
        let builtin = BuiltinChoice { class: Some(design::mowt::RoadClass::II), terrain: Some(design::mowt::Terrain::Rolling), speed_kmh: Some(70.0), emax_percent: Some(7.0), ..Default::default() };
        store_check(doc, alignment, &AlignCheckInputs { source: LimitSource::Builtin, builtin, ..Default::default() });
    }
    // Demo values: the second option moves the middle PI 60 m to the south and uses a 300 m radius.
    let mut b = a.clone();
    b.horizontal.name = "Option B".into();
    b.horizontal.pis[1].y -= 60.0;
    b.horizontal.pis[1].radius = 300.0;
    if let Some(v) = b.vertical.as_mut() {
        v.name = "Option B".into();
    }
    let mut a_option = a.clone();
    a_option.horizontal.name = "Option A".into();
    if let Some(v) = a_option.vertical.as_mut() {
        v.name = "Option A".into();
    }
    let surface = afrobim_corridor::surfaces(doc).first().map(|s| s.0);
    let set = design::OptionSet {
        name: "Demo options (arbitrary demo values)".into(),
        surface,
        estimate: None,
        options: vec![
            design::AlignmentOption { name: "Option A".into(), design: a_option, corridor: None, note: "demo".into() },
            design::AlignmentOption { name: "Option B".into(), design: b, corridor: None, note: "demo".into() },
        ],
        criteria: Vec::new(),
    };
    let set_id = design::add_option_set(doc, &set);
    hide_design_layers(doc);
    let _ = design_a;
    // Demo only: the typical section of the demo corridor, for the whole length of each option.
    if let Some(corridor) = doc.elements().iter().find(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY).map(|e| e.id) {
        if let Some(mut def) = afrobim_corridor::corridor_element(doc, corridor).map(|c| c.definition) {
            def.start_station_m = None;
            def.end_station_m = None;
            if let Ok(mut s) = load_set(doc, set_id) {
                for o in &mut s.options {
                    o.corridor = Some(def.clone());
                }
                if let Err(e) = store_set(doc, set_id, &s) {
                    problems.push(format!("Demo options: {e}"));
                }
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;

    fn doc_with_design() -> (Document, ElementId, ElementId) {
        let mut doc = Document::default();
        let (d, a) = create(&mut doc, "Test road", 1000.0, &[(0.0, 0.0), (400.0, 0.0), (400.0, 300.0)]).unwrap();
        // Give the middle PI a radius and a profile: test values, arbitrary.
        edit_horizontal(&mut doc, d, HEdit::SetRadius(1, 100.0)).unwrap();
        let v = VerticalDesign::new("Test road", vec![VPvi::point(1000.0, 100.0), VPvi::with_length(1300.0, 104.0, 100.0), VPvi::point(1650.0, 101.0)]);
        set_vertical(&mut doc, d, Some(v)).unwrap();
        (doc, d, a)
    }

    fn add_sign_at(doc: &mut Document, alignment: ElementId, station: f64, offset: f64) -> ElementId {
        let id = doc.add(signs_glue::CATEGORY, "SIGN-T", Pt::new(0.0, 0.0), 100.0, 100.0, "0", serde_json::Value::Null);
        alignment_glue::place_sign(doc, id, alignment, station, None, offset.abs(), offset > 0.0).unwrap();
        id
    }

    #[test]
    fn a_new_design_has_no_radius_and_no_spiral_until_entered() {
        let mut doc = Document::default();
        let (d, a) = create(&mut doc, "Road", 0.0, &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]).unwrap();
        let design = load(&doc, d).unwrap();
        assert!(design.horizontal.pis.iter().all(|p| p.radius == 0.0 && p.spiral_in == 0.0 && p.spiral_out == 0.0));
        assert!(design.vertical.is_none());
        assert_eq!(design_of_alignment(&doc, a), Some(d));
        assert!(create(&mut doc, "", 0.0, &[(0.0, 0.0), (1.0, 1.0)]).is_err());
        assert!(create(&mut doc, "One", 0.0, &[(0.0, 0.0)]).is_err());
    }

    #[test]
    fn editing_a_pi_keeps_the_alignment_element_and_moves_a_placed_sign() {
        let (mut doc, d, a) = doc_with_design();
        let sign = add_sign_at(&mut doc, a, 1200.0, 5.0);
        let before = doc.find(sign).unwrap().site.clone().unwrap();
        let name_before = doc.find(a).unwrap().name.clone();
        let r = edit_horizontal(&mut doc, d, HEdit::MovePi(2, 400.0, 360.0)).unwrap();
        // The alignment element kept its id and name, and its length changed.
        assert_eq!(alignment_of_design(&doc, d), Some(a));
        assert_eq!(doc.find(a).unwrap().name, name_before);
        // The sign on the straight part has the same station and offset but is placed from the new geometry.
        let after = doc.find(sign).unwrap().site.clone().unwrap();
        assert_eq!(after.station_m, before.station_m);
        assert!(r.lines.iter().any(|l| l.contains("sign")), "{r:?}");
        // Move PI 1 so that the straight where the sign stands turns: the sign must move.
        edit_horizontal(&mut doc, d, HEdit::MovePi(1, 400.0, 40.0)).unwrap();
        let moved = doc.find(sign).unwrap().site.clone().unwrap();
        assert!((moved.y - before.y).abs() > 1000.0 || (moved.x - before.x).abs() > 1000.0, "{before:?} {moved:?}");
        assert_eq!(moved.alignment.as_deref(), Some(name_before.as_str()));
    }

    #[test]
    fn a_refused_edit_leaves_the_document_unchanged_and_undo_restores() {
        let (mut doc, d, _a) = doc_with_design();
        let before = doc.to_json();
        // A 100 m radius cannot fit on a 300 m leg if the next PI sits right on top of this one.
        let bad = edit_horizontal(&mut doc, d, HEdit::SetRadius(1, 1.0e9));
        assert!(bad.is_err(), "an enormous radius cannot fit");
        assert_eq!(doc.to_json(), before, "a refused edit changes nothing");
        // An accepted edit changes the document, and restoring the snapshot taken before it undoes it.
        let snapshot = doc.snapshot();
        edit_horizontal(&mut doc, d, HEdit::MovePi(2, 400.0, 340.0)).unwrap();
        assert_ne!(doc.to_json(), before);
        doc.restore(snapshot);
        assert_eq!(doc.to_json(), before);
    }

    #[test]
    fn a_profile_pvi_edit_changes_a_markings_level() {
        let (mut doc, d, a) = doc_with_design();
        let request = markings_glue::MarkingRequest { line: 0, start_station_m: 1050.0, end_station_m: 1200.0, distance_m: 0.0, right: false, speed_mph: 30, speed_kmh: None, width_mm: Some(150), material: String::new(), basis_uganda: false, uk_fallback: None };
        let mark = markings_glue::add(&mut doc, a, &request).unwrap();
        let level = |doc: &Document| {
            let e = doc.elements().iter().find(|e| e.name == mark).unwrap();
            let def = afrobim_markings::MarkingDefinition::from_value(&e.data).unwrap();
            def.geometry.unwrap().profile.unwrap().elevation_at(1100.0).unwrap()
        };
        let before = level(&doc);
        let r = edit_vertical(&mut doc, d, VEdit::MovePvi(1, 1300.0, 106.0)).unwrap();
        let after = level(&doc);
        assert!((after - before).abs() > 0.1, "{before} {after}");
        assert!(r.lines.iter().any(|l| l.contains("marking")), "{r:?}");
        // A refused profile edit (a PVI moved past its neighbour) changes nothing.
        let json = doc.to_json();
        assert!(edit_vertical(&mut doc, d, VEdit::MovePvi(1, 1700.0, 106.0)).is_err());
        assert_eq!(doc.to_json(), json);
    }

    #[test]
    fn a_shorter_alignment_lists_the_marking_and_the_sign_now_out_of_range() {
        let (mut doc, d, a) = doc_with_design();
        let request = markings_glue::MarkingRequest { line: 0, start_station_m: 1100.0, end_station_m: 1500.0, distance_m: 0.0, right: false, speed_mph: 30, speed_kmh: None, width_mm: Some(150), material: String::new(), basis_uganda: false, uk_fallback: None };
        let mark = markings_glue::add(&mut doc, a, &request).unwrap();
        let sign = add_sign_at(&mut doc, a, 1470.0, 4.0);
        let sign_mark = doc.find(sign).unwrap().name.clone();
        // The last PI moves in towards the second: the alignment is now much shorter than 1500.
        let r = edit_horizontal(&mut doc, d, HEdit::MovePi(2, 400.0, 110.0)).unwrap();
        assert!(r.problems.iter().any(|p| p.starts_with(&mark) && p.contains("alignment now runs")), "{:?}", r.problems);
        assert!(r.problems.iter().any(|p| p.starts_with(&sign_mark)), "{:?}", r.problems);
        assert!(r.note().contains("Out of range or refused"));
        // The sign keeps its place, so nothing is moved without a station to put it at.
        assert!(doc.find(sign).unwrap().site.is_some());
    }

    fn limits() -> DesignBasis {
        // Arbitrary test limits, labelled as such: not from any standard.
        let text = "#name,Test limits (arbitrary)\n#source,test fixture, not a standard\nlabel,design_speed_kmh,min_radius_m,max_grade\nrow A,60,150,0.02\n";
        basis_from_text(text, false).unwrap()
    }

    #[test]
    fn a_check_with_a_limits_table_gives_statuses_and_with_none_says_not_checked() {
        let (mut doc, d, a) = doc_with_design();
        let design = load(&doc, d).unwrap();
        let report = run_check(&design, &limits(), "row A").unwrap();
        // Radius 100 against a minimum of 150 fails; the 2 % grade limit is exceeded by the 1.3 % ... check each.
        let radius = report.items.iter().find(|i| i.id == "H.radius").expect("a radius check");
        assert_eq!(radius.status, design::CheckStatus::Fail);
        assert!(radius.limit_basis.contains("table"), "{}", radius.limit_basis);
        // A limit the table does not give is Not checked, never passed.
        let curve = report.items.iter().find(|i| i.id == "H.curve_length").expect("a curve length check");
        assert_eq!(curve.status, design::CheckStatus::NotChecked);
        assert!(report.source.contains("test fixture"));
        // CHECKALL's view: nothing stored asks for input, an empty path says not checked, a file works.
        assert!(matches!(check_for_report(&doc, a), Some(AlignmentCheck::InputNeeded(_))));
        store_check(&mut doc, a, &AlignCheckInputs::default());
        assert!(matches!(check_for_report(&doc, a), Some(AlignmentCheck::NotChecked(_))));
        let dir = std::env::temp_dir().join(format!("afrobim-limits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("limits.csv");
        std::fs::write(&path, "#name,t\n#source,fixture\nlabel,min_radius_m\nrow A,150\n").unwrap();
        store_check(&mut doc, a, &AlignCheckInputs { limits_path: path.display().to_string(), row_label: "row A".into(), ..Default::default() });
        match check_for_report(&doc, a) {
            Some(AlignmentCheck::Report(r)) => assert!(r.count(design::CheckStatus::Fail) >= 1),
            _ => panic!("expected a report"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_blank_template_has_headers_and_no_values_and_is_refused_until_filled() {
        let t = blank_template();
        let header = t.lines().last().unwrap();
        assert!(header.starts_with("label,design_speed_kmh,min_radius_m") && header.ends_with("headlight_beam_deg"));
        assert!(t.lines().take(2).all(|l| l == "#name," || l == "#source,"));
        assert!(basis_from_text(&t, false).is_err(), "a template with no source and no rows is not a limits table");
    }

    #[test]
    fn an_option_set_round_trips_duplicates_deletes_and_makes_an_option_current() {
        let (mut doc, d, a) = doc_with_design();
        let set = new_set(&mut doc, "Options").unwrap();
        add_option(&mut doc, set, d, "Base").unwrap();
        // The same name twice is refused.
        assert!(add_option(&mut doc, set, d, "Base").is_err());
        // Change the design, add it as a second option.
        edit_horizontal(&mut doc, d, HEdit::MovePi(2, 400.0, 340.0)).unwrap();
        add_option(&mut doc, set, d, "Longer").unwrap();
        duplicate_option(&mut doc, set, 1).unwrap();
        let loaded = load_set(&doc, set).unwrap();
        assert_eq!(loaded.options.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["Base", "Longer", "Longer copy"]);
        // It survives save and open.
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(load_set(&reopened, set).unwrap(), loaded);
        // Make the first option current: the design goes back, the alignment element is the same.
        let length_now = alignment_glue::alignment_of(&doc, a).unwrap().length();
        make_current(&mut doc, set, 0, d).unwrap();
        assert!(alignment_glue::alignment_of(&doc, a).unwrap().length() < length_now);
        assert_eq!(alignment_of_design(&doc, d), Some(a));
        delete_option(&mut doc, set, 2).unwrap();
        assert_eq!(load_set(&doc, set).unwrap().options.len(), 2);
        // The comparison is computed and every figure says how.
        let cmp = compare(&doc, set).unwrap();
        assert_eq!(cmp.options.len(), 2);
        assert!(cmp.options[0].figure("length_m").is_some_and(|f| f.value.is_some() && !f.how.is_empty()));
    }

    #[test]
    fn the_demo_makes_a_design_with_two_options() {
        let mut doc = Document::default();
        assert_eq!(demo(&mut doc), Vec::<String>::new());
        assert_eq!(designs(&doc).len(), 1);
        let sets = option_sets(&doc);
        assert_eq!(sets.len(), 1);
        assert_eq!(load_set(&doc, sets[0].0).unwrap().options.len(), 2);
    }
}
