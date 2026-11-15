//! The calculation report pack (`REPORTPACK`): one A4 portrait PDF, a plain-text variant and a CSV
//! variant of every check the model-wide check can make, with the summaries of the clash, quantity,
//! estimate and programme records.
//!
//! The pack adds nothing to what the engines wrote: each check shows the element mark, the inputs
//! stored with the element, the calculation lines exactly as the engine returned them, and the
//! status. A section with no data is listed in the contents as "not included: no data" and has no
//! pages. Every page carries the project name, the date the pack was composed and "page n of m". The
//! cover says the pack is a record of calculations for checking by an engineer and is not a
//! certification. The PDF is written by the sheets crate's PDF writer, and its tables by the sheets
//! crate's table layout.

use std::collections::BTreeMap;

use afrobim_draw::Pt2;
use afrobim_kernel::{Document, ElementId, ModuleRegistry};
use afrobim_sheets::item::{layers, text as sheet_text};
use afrobim_sheets::table::{draw_table, layout_table, rows_that_fit};
use afrobim_sheets::{wrap, Align, ComposedSheet, Rect, Styles, Table};

use crate::check_all::{Row, Status};

/// What the cover says about the pack.
pub const STATEMENT: &str = "This pack is a record of calculations for checking by an engineer. It is not a certification, an approval or a statement that anything complies with a standard.";

/// What the engines rest on, from their own READMEs: (area, documents and editions read, status and limits).
/// The manual's "Standards and editions" table is the same list.
pub const BASIS: &[(&str, &str, &str)] = &[
    ("Traffic signs and markings", "UK Traffic Signs Manual (chapter editions as held, Chapter 7 2013, Chapter 5 2018 edition written for TSRGD 2016); Uganda Traffic Signs Manual Vol. 1 and 2 (July 2004)", "The 2003 edition of Chapter 5 is superseded and was not used. TSRGD 2016 itself, Chapters 6 and 8 and TrafxPLAN documents were not read."),
    ("Sign supports", "IHE Sign Structures Guide 2021 applying BS EN 12899-1:2007 and its UK National Annex; BS EN 1993-1-1; PD 6547 or CD 354 revision 1 for the planted foundation; MoWT Road Design Manual Vol. 4 Bridge Design (January 2010) Section 5.3 Table 2 for Uganda wind", "The ground factor G and the BS EN 40-2 Table 7 planting depth are inputs (tables not held). The MoWT wind map is blank in the published PDF. Using the bridge manual's velocity pressure as the sign's basic wind pressure is a reading of two documents that do not refer to each other."),
    ("Drainage", "MoWT Road Design Manual Vol. 2 Drainage Design (January 2010); DMRB CG 501 v2.2.0, CD 521 v1.2.0, CD 522 v0.1.0, CD 529 v1.1.0 (September 2025); SANRAL Drainage Manual, sixth edition (September 2013)", "Which basis governs is the engineer's choice and each keeps its own values. The MoWT complete culvert check uses FHWA HDS-5 inlet control (equations; nomographs can differ by 10 per cent). SANRAL: the slope unit of Table 7.2 is not stated by the manual and is the engineer's answer; rainfall is always entered."),
    ("Pavement", "MoWT Road Design Manual Vol. 3 Part I Flexible Pavements (January 2010); DMRB CD 224 Revision 0 (March 2020) and CD 226 v0.1.0 (29 November 2021); AASHTO Guide for Design of Pavement Structures (1993)", "CD 224 and CD 226 are SUPERSEDED revisions (by v0.1.0 and v0.2.0 of 2025); the current ones could not be obtained and the DMRB method gives no complete structure. The South African basis (SAPEM Chapter 10, reproducing Draft TRH4) gives design traffic in E80, traffic class, climatic class and the subgrade action only: the TRH4 catalogue was not obtainable, so it gives no layers. E80 is not interchangeable with the other methods' standard axles."),
    ("Road restraint", "DMRB CD 377 v5.0.1 (27 February 2026); CD 127 v1.0.1 (29 July 2021); BS EN 1317-2:2010 (the owner's scan); DD ENV 1317-4:2002", "The currency of BS EN 1317-2:2010 is for the owner to confirm. DD ENV 1317-4:2002 is a Draft for Development that CD 377 5.0.1 still cites. PD CEN/TS 1317-7 is not held."),
    ("Gantries", "DMRB CD 365 Revision 1 (March 2020); CD 127 v1.0.1; CD 377 Revision 4 (January 2021, read only, no value taken)", "CD 365 Revision 1 and CD 377 Revision 4 are watermarked SUPERSEDED. The current CD 365 is awaited."),
    ("Street lighting", "DMRB TD 501 v0.1.0 (September 2025); TA 501 Revision 0 (March 2020); CD 109 v1.1.0; CD 377 v5.0.1; BS 5489-1:2003 (owner's copy); Stockmar Handbook about Interpretation of EN 13201 (LiTG, October 2017); MoWT Road Design Manual Vol. 1 Section 12.8; TII DN-LHT-03038 (2012)", "BS 5489-1:2003 is a SUPERSEDED edition (replaced in 2013 and 2020). The EN 13201-2 class values come from the Stockmar handbook, a SECONDARY source: BS EN 13201-2, -3 and -5 are not held. The MoWT manual and TII document are secondary references."),
    ("Lighting photometry", "IES LM-63 and EULUMDAT file formats as described in the sources cited in afrobim-lighting; EN 13032-1 (public preview)", "Not an EN 13201-3 calculation; not validated against DIALux, Relux or AGi32; luminance needs a user r-table; TI, U_l and wet U_o are not computed."),
    ("Sign visibility", "UK Traffic Signs Manual Chapter 1 (1982 edition), Chapter 2 x-height extract, Chapter 3 (2008), Chapter 4 (2013), Chapter 7 (2013); Uganda Traffic Signs Manual Vol. 1 (July 2004)", "An analysis aid: it compares and cites and states no compliance. The 1.05 m eye height is stated in the sources for other criteria. Stopping sight distance (TD 9) is not held."),
    ("Alignment design", "Uganda MoWT Road Design Manual Vol. I: Geometric Design (January 2010), built in as a selectable basis (class, terrain, design speed, emax), or the user's own limits table with its sources, or the built-in row with the user's table laid over it", "The copy of the manual used was hosted by a third party and was not compared with a Ministry original. Cells where the manual contradicts itself are withheld and read Not checked. Minimum curve and spiral lengths, tangent lengths and the minimum vertical curve length are not stated by the manual and are Not checked."),
    ("Programme, estimate and site progress", "No standard: durations, rates, terms and records are the user's", "MS Project XML and Primavera P6 XER files have not been opened in those programs."),
];

/// One block of a section's body.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(String),
    Para(String),
    /// Lines kept as they are (calculation lines): wrapped only when too long for the page.
    Lines(Vec<String>),
    Table(Table),
}

/// One section of the pack: `body` is `None` when there is no data for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub title: String,
    pub body: Option<Vec<Block>>,
}

/// One check, for the CSV variant.
#[derive(Debug, Clone, PartialEq)]
pub struct PackRow {
    pub section: String,
    pub mark: String,
    pub category: String,
    pub check: String,
    pub status: &'static str,
    pub reason: String,
    pub inputs: Vec<String>,
    pub lines: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub project: String,
    pub info: afrobim_sheets::ProjectInfo,
    pub document: String,
    pub composed: String,
    pub standard: String,
    pub sections: Vec<Section>,
    pub rows: Vec<PackRow>,
}

const SECTION_TITLES: [&str; 14] = [
    "Design basis and warnings",
    "Supports",
    "Drainage",
    "Pavement",
    "Restraint",
    "Lighting (class check and photometry)",
    "Gantries",
    "Alignment checks",
    "Sign visibility",
    "Sign and marking layout rules",
    "Clash summary",
    "Quantities summary",
    "Estimate summary",
    "Programme summary",
];

/// Which section a check belongs to.
fn section_of(doc: &Document, r: &Row) -> usize {
    let category = doc.find(r.element).map(|e| e.category.clone()).unwrap_or_default();
    match category.as_str() {
        c if c == afrobim_drainage::DRAIN_CATEGORY || c == afrobim_drainage::CULVERT_CATEGORY => 2,
        c if c == afrobim_pavement::PAVEMENT_CATEGORY => 3,
        c if c == afrobim_restraint::CATEGORY => 4,
        c if c == afrobim_lighting::CATEGORY => 5,
        c if c == afrobim_gantry::GANTRY_CATEGORY => 6,
        c if c == crate::alignment_glue::CATEGORY => 7,
        c if c == afrobim_markings::CATEGORY || c == afrobim_markings::CATEGORY_EXT => 9,
        _ if r.check.starts_with("Sign support") => 1,
        _ if r.check.starts_with("Sign visibility") => 8,
        _ => 9,
    }
}

/// The stored-input keys a check reads. A check is shown only the inputs that belong to it: a
/// sign's visibility inputs are not printed under its support check, and the other way round.
/// A check that reads none (the marking rules, the layout rules) shows none.
pub fn keys_of_check(check: &str) -> &'static [&'static str] {
    use crate::check_inputs as k;
    if check.starts_with("Sign support design") {
        &[k::KEY_SUPPORT]
    } else if check.starts_with("Sign visibility") {
        &["check.visibility"]
    } else if check.starts_with("Side drain") {
        &[k::KEY_DRAIN, k::KEY_RATIONAL]
    } else if check.starts_with("Culvert") {
        &[k::KEY_CULVERT, k::KEY_RATIONAL]
    } else if check.starts_with("Lighting requirement") {
        &[k::KEY_LIGHTING]
    } else if check.starts_with("Barrier run") {
        &[k::KEY_RESTRAINT]
    } else if check.starts_with("Pavement design check") {
        &[k::KEY_PAVEMENT]
    } else if check.starts_with("Alignment geometry") {
        &[crate::design_glue::CHECK_KEY]
    } else {
        &[]
    }
}

/// The inputs stored with an element that belong to `check`, as `key: value` lines, empty entries left out.
pub fn inputs_lines(doc: &Document, id: ElementId, check: &str) -> Vec<String> {
    let keys = keys_of_check(check);
    fn walk(prefix: &str, v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Null => {}
            serde_json::Value::String(s) if s.trim().is_empty() => {}
            serde_json::Value::Array(a) if a.is_empty() => {}
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    walk(&if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") }, x, out);
                }
            }
            serde_json::Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    walk(&format!("{prefix}[{}]", i + 1), x, out);
                }
            }
            serde_json::Value::String(s) => out.push(format!("{prefix}: {s}")),
            other => out.push(format!("{prefix}: {other}")),
        }
    }
    let mut out = Vec::new();
    let Some(e) = doc.find(id) else { return out };
    for (k, text) in &e.properties {
        if !k.starts_with("check.") || k == crate::visibility_ui::KEY_ACCEPTED || !keys.contains(&k.as_str()) {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(v) => walk(k.trim_start_matches("check."), &v, &mut out),
            Err(_) => out.push(format!("{k}: {text}")),
        }
    }
    out
}

/// True for a line an engine wrote to warn about an edition, a secondary source or a limit of the
/// method. These are collected in the design basis section; they also stay in the check they came from.
fn is_warning(line: &str) -> bool {
    let l = line.to_lowercase();
    ["superseded", "secondary", "not the current", "withdrawn", "warning", "edition gap", "not validated", "does not govern"].iter().any(|k| l.contains(k))
}

fn section_blocks_for_rows(rows: &[&PackRow]) -> Vec<Block> {
    let mut out = Vec::new();
    for r in rows {
        out.push(Block::Heading(format!("{} ({}): {}", r.mark, r.category, r.check)));
        out.push(Block::Para(format!("Status: {}. {}", r.status, r.reason)));
        if !r.inputs.is_empty() {
            out.push(Block::Para("Inputs stored with the element".into()));
            out.push(Block::Lines(r.inputs.clone()));
        }
        if !r.lines.is_empty() {
            out.push(Block::Para("Calculation, as the engine wrote it".into()));
            out.push(Block::Lines(r.lines.clone()));
        }
        if !r.notes.is_empty() {
            out.push(Block::Para("Messages from the model".into()));
            out.push(Block::Lines(r.notes.clone()));
        }
    }
    out
}

fn table_from_csv(title: &str, csv: &str) -> Table {
    let mut t = Table::from_csv(title, csv);
    t.right_align_numeric_columns();
    t
}

/// Builds the pack from the document. `composed` is the date text printed on every page.
pub fn build(doc: &Document, registry: &ModuleRegistry, engine: Option<&afrobim_signs::SignEngine>, composed: &str, document: &str, standard: &str) -> Pack {
    let info = afrobim_sheets::project_info(doc);
    let project = if info.project.trim().is_empty() { document.to_string() } else { info.project.trim().to_string() };
    let report = crate::sheets_glue::check_report(doc, registry);
    let rows: Vec<(usize, PackRow)> = report
        .rows
        .iter()
        .map(|r| {
            let s = section_of(doc, r);
            (
                s,
                PackRow { section: SECTION_TITLES[s].to_string(), mark: r.mark.clone(), category: r.category.clone(), check: r.check.clone(), status: r.status.label(), reason: r.reason.clone(), inputs: inputs_lines(doc, r.element, &r.check), lines: r.lines.clone(), notes: r.notes.clone() },
            )
        })
        .collect();

    // Design basis: the documents, then every warning the engines wrote, with where it appeared.
    let mut warnings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, r) in &rows {
        for l in r.lines.iter().chain(r.notes.iter()).filter(|l| is_warning(l)) {
            let marks = warnings.entry(l.trim().to_string()).or_default();
            if !marks.contains(&r.mark) {
                marks.push(r.mark.clone());
            }
        }
    }
    let mut basis = vec![
        Block::Heading("Design bases in use in this document".into()),
        Block::Lines(design_bases_lines(doc, standard)),
        Block::Para(format!("Sign design standard of this document: {standard}. The documents and editions below are those the engines state in their own documentation; which of them governs a project is the engineer's choice.")),
        Block::Table(Table { headers: vec!["Area".into(), "Documents and editions read".into(), "Status and limits".into()], rows: BASIS.iter().map(|b| vec![b.0.into(), b.1.into(), b.2.into()]).collect(), col_widths_mm: vec![26.0, 70.0, 0.0], title: Some("Standards and editions".into()), ..Table::default() }),
        Block::Heading("Warnings written by the engines in this pack".into()),
    ];
    if warnings.is_empty() {
        basis.push(Block::Para("No check in this pack carries a superseded-edition, secondary-source or method warning.".into()));
    } else {
        basis.push(Block::Para("Each line is as the engine wrote it, followed by the elements whose check carried it. Superseded editions and secondary sources are named in the lines.".into()));
        let mut lines = Vec::new();
        for (l, marks) in &warnings {
            lines.push(format!("{l}   [{}]", marks.join(", ")));
        }
        basis.push(Block::Lines(lines));
    }

    let mut sections: Vec<Section> = vec![Section { title: SECTION_TITLES[0].into(), body: Some(basis) }];
    for s in 1..=9 {
        let mine: Vec<&PackRow> = rows.iter().filter(|(i, _)| *i == s).map(|(_, r)| r).collect();
        let mut body = (!mine.is_empty()).then(|| section_blocks_for_rows(&mine));
        if s == 5 {
            let photo = photometry_blocks(doc);
            if !photo.is_empty() {
                body.get_or_insert_with(Vec::new).extend(photo);
            } else if body.is_some() {
                body.get_or_insert_with(Vec::new).push(Block::Para("Photometry: not included: no luminaire file was run for any installation.".into()));
            }
        }
        sections.push(Section { title: SECTION_TITLES[s].into(), body });
    }
    sections.push(Section { title: SECTION_TITLES[10].into(), body: clash_blocks(doc) });
    sections.push(Section { title: SECTION_TITLES[11].into(), body: quantity_blocks(doc, registry, engine) });
    sections.push(Section { title: SECTION_TITLES[12].into(), body: estimate_blocks(doc, engine) });
    sections.push(Section { title: SECTION_TITLES[13].into(), body: programme_blocks(doc) });

    Pack { project, info, document: document.to_string(), composed: composed.to_string(), standard: standard.to_string(), sections, rows: rows.into_iter().map(|(_, r)| r).collect() }
}

/// The speed band rule and the design basis each kind of check is using, from what the document stores.
pub fn design_bases_lines(doc: &Document, standard: &str) -> Vec<String> {
    use afrobim_drainage::design::DesignBasis;
    let mut out = vec![format!("Sign standard: {standard}.")];
    out.push(format!("Speed band rule (a km/h limit read against the UK mph thresholds): {}.", speed_rule_text(&afrobim_kernel::speed_band_rule(doc))));
    // Drainage: the basis of each element that has stored inputs.
    let mut drain: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for e in doc.elements() {
        let basis = if e.category == afrobim_drainage::DRAIN_CATEGORY {
            crate::check_inputs::load::<crate::check_inputs::DrainInputs>(doc, e.id, crate::check_inputs::KEY_DRAIN).map(|i| i.basis())
        } else if e.category == afrobim_drainage::CULVERT_CATEGORY {
            crate::check_inputs::load::<crate::check_inputs::CulvertInputs>(doc, e.id, crate::check_inputs::KEY_CULVERT).map(|i| i.basis())
        } else {
            None
        };
        if let Some(b) = basis {
            drain.entry(b.name()).or_default().push(e.name.clone());
        }
        let _: Option<DesignBasis> = None;
    }
    if drain.is_empty() {
        out.push("Drainage: no basis is stored (no drain or culvert has stored check inputs).".into());
    }
    for (name, marks) in &drain {
        out.push(format!("Drainage: {name}: {}.", marks.join(", ")));
    }
    // Pavement: the method of each pavement with stored inputs.
    let mut methods: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for e in doc.elements().iter().filter(|e| e.category == afrobim_pavement::PAVEMENT_CATEGORY) {
        if let Some(form) = crate::check_inputs::load::<crate::road_glue::DesignForm>(doc, e.id, crate::check_inputs::KEY_PAVEMENT) {
            methods.entry(crate::road_glue::methods()[form.method.min(crate::road_glue::methods().len() - 1)].name()).or_default().push(e.name.clone());
        }
    }
    if methods.is_empty() {
        out.push("Pavement: no design method is stored.".into());
    }
    for (name, marks) in &methods {
        out.push(format!("Pavement: {name}: {}.", marks.join(", ")));
    }
    // Alignment checks: where the limits come from.
    let mut any = false;
    for e in doc.elements().iter().filter(|e| e.category == crate::alignment_glue::CATEGORY) {
        if let Some(s) = crate::design_glue::stored_check(doc, e.id) {
            any = true;
            let what = match s.source {
                crate::design_glue::LimitSource::UserFile => format!("the user's limits file {} (row {})", s.limits_path, s.row_label),
                crate::design_glue::LimitSource::Builtin => format!("the built-in MoWT Road Design Manual Vol. I (January 2010), {}", builtin_text(&s.builtin)),
                crate::design_glue::LimitSource::BuiltinWithOverlay => format!("the built-in MoWT Road Design Manual Vol. I (January 2010), {}, with the user's limits file {} laid over it (row {})", builtin_text(&s.builtin), s.limits_path, s.row_label),
            };
            out.push(format!("Alignment {}: limits from {what}.", e.name));
        }
    }
    if !any {
        out.push("Alignment checks: no limits source is stored.".into());
    }
    // Markings: the basis of each second-set marking, and every UK value used under the Uganda basis.
    let (mut uk, mut ug, mut fallbacks) = (0, 0, Vec::new());
    for e in doc.elements().iter().filter(|e| e.category == afrobim_markings::CATEGORY_EXT) {
        if let Ok(d) = afrobim_markings::ExtDefinition::from_value(&e.data) {
            match d.basis {
                afrobim_markings::Basis::Uk => uk += 1,
                afrobim_markings::Basis::Uganda => ug += 1,
            }
            if let Some(r) = d.uk_fallback {
                fallbacks.push(format!("{}: {r}", e.name));
            }
        }
    }
    if uk + ug > 0 {
        out.push(format!("Road markings, second set: {uk} to the UK Traffic Signs Manual Chapter 5 (2018), {ug} to the Uganda Traffic Signs Manual (2004)."));
        for f in fallbacks {
            out.push(format!("UK value used under the Uganda basis by the engineer's choice, {f}"));
        }
    }
    out
}

fn builtin_text(b: &crate::design_glue::BuiltinChoice) -> String {
    match b.selection() {
        Ok(s) => format!("class {}, {}, {} km/h, emax {} %, {} maximum gradient{}", s.class.manual_name(), s.terrain.manual_name(), s.design_speed_kmh, s.emax_percent, if b.absolute_grade { "absolute" } else { "desirable" }, if b.minimum_grade_in_cut { ", minimum grade in cut checked" } else { "" }),
        Err(why) => format!("selection incomplete ({why})"),
    }
}

fn speed_rule_text(rule: &afrobim_kernel::SpeedBandRule) -> String {
    use afrobim_kernel::SpeedBandRule as R;
    match rule {
        R::RoundUp { .. } => "round up to the next band".to_string(),
        R::Exact => "use the exact converted value".to_string(),
        R::AskEachTime => "ask each time".to_string(),
        R::Table(t) => format!("the user's table of {} limit(s)", t.len()),
    }
}

fn photometry_blocks(doc: &Document) -> Vec<Block> {
    use afrobim_lighting::calc::{photometry_settings_of, run_stored_photometry};
    let mut out = Vec::new();
    for e in doc.elements().iter().filter(|e| e.category == afrobim_lighting::CATEGORY) {
        if !matches!(photometry_settings_of(doc, e.id), Ok(Some(_))) {
            continue;
        }
        let alignment = afrobim_lighting::lighting_of(doc, e.id).ok().and_then(|d| {
            let name = match &d.definition {
                afrobim_lighting::LightingDefinition::Installation(i) => i.alignment.clone(),
                afrobim_lighting::LightingDefinition::Column(c) => c.alignment.clone(),
            };
            crate::alignment_glue::alignments(doc).into_iter().find(|a| a.1 == name).and_then(|a| crate::alignment_glue::alignment_of(doc, a.0))
        });
        out.push(Block::Heading(format!("{}: photometry from a luminaire file", e.name)));
        out.push(Block::Para(crate::photometry_ui::LIMITS.to_string()));
        match run_stored_photometry(doc, e.id, alignment.as_ref(), None) {
            Err(err) => out.push(Block::Para(format!("The stored run could not be repeated: {err}"))),
            Ok(run) => {
                let c = &run.flux_check;
                let mut lines = vec![
                    format!("File {} (SHA-256 {}).", run.file_name, &run.file_sha256[..12.min(run.file_sha256.len())]),
                    format!("Flux by integration {:.0} lm; downward fraction {:.1} %; stated lamp flux {}.", c.integrated_flux_lm, c.downward_fraction * 100.0, c.stated_lamp_flux_lm.map(|f| format!("{f:.0} lm")).unwrap_or_else(|| "not stated".into())),
                    format!("{} luminaire(s); maintenance factor {}; contribution distance {} m; plane level {:.2} m; maintained values.", run.result.luminaire_count, run.result.maintenance_factor, run.result.contribution_distance_m, run.result.plane_level_m),
                ];
                if let Some(d) = c.difference_percent {
                    lines.push(format!("Integrated flux against the file's own figures: {d:+.1} % (no tolerance applied)."));
                }
                out.push(Block::Lines(lines));
                out.push(Block::Table(Table::new(
                    &["Area", "Points", "Average lx", "Minimum lx", "Maximum lx", "U_o min/avg", "min/max"],
                    run.result.areas.iter().map(|a| vec![a.name.clone(), format!("{} x {}", a.n_along, a.n_across), format!("{:.2}", a.e_h.average), format!("{:.2}", a.e_h.minimum), format!("{:.2}", a.e_h.maximum), format!("{:.3}", a.e_h.uniformity_min_avg), format!("{:.3}", a.e_h.uniformity_min_max)]).collect(),
                )));
                if !run.warnings.is_empty() {
                    out.push(Block::Lines(run.warnings.iter().map(|w| format!("Warning: {w}")).collect()));
                }
            }
        }
    }
    out
}

fn clash_blocks(doc: &Document) -> Option<Vec<Block>> {
    use afrobim_coordination::Status as S;
    let (_, co) = crate::coordination_glue::load(doc);
    if co.tests.is_empty() && co.results.clashes.is_empty() && co.issues.is_empty() {
        return None;
    }
    let statuses = [S::New, S::Active, S::Reviewed, S::Approved, S::Resolved];
    let mut out = vec![Block::Para(format!("{} test(s), {} clash(es) on record, {} issue(s).", co.tests.len(), co.results.clashes.len(), co.issues.len()))];
    if !co.tests.is_empty() {
        let rows = co
            .tests
            .iter()
            .map(|t| {
                let mut r = vec![t.name.clone(), t.kind.label().to_string(), if t.kind == afrobim_coordination::ClashKind::Clearance { format!("{} mm", t.tolerance_mm) } else { String::new() }];
                r.extend(statuses.iter().map(|s| co.results.clashes.iter().filter(|c| c.test == t.name && c.status == *s).count().to_string()));
                r
            })
            .collect();
        out.push(Block::Table(Table::new(&["Test", "Type", "Tolerance", "New", "Active", "Reviewed", "Approved", "Resolved"], rows).with_title("Clash tests and clashes by status")));
    }
    let live: Vec<_> = co.results.clashes.iter().filter(|c| matches!(c.status, S::New | S::Active | S::Reviewed)).collect();
    if !live.is_empty() {
        let rows = live
            .iter()
            .map(|c| {
                vec![
                    c.id.clone(),
                    c.test.clone(),
                    c.mark_a.clone(),
                    c.mark_b.clone(),
                    match (c.depth_mm, c.distance_mm) {
                        (Some(d), _) => format!("{d:.1} mm deep"),
                        (_, Some(d)) => format!("{d:.1} mm apart"),
                        _ => String::new(),
                    },
                    c.status.label().to_string(),
                    c.assignee.clone(),
                ]
            })
            .collect();
        out.push(Block::Table(Table::new(&["Id", "Test", "Element A", "Element B", "Depth or distance", "Status", "Assignee"], rows).with_title("Clashes still open")));
    }
    Some(out)
}

fn quantity_blocks(doc: &Document, registry: &ModuleRegistry, engine: Option<&afrobim_signs::SignEngine>) -> Option<Vec<Block>> {
    let mut out = Vec::new();
    if let Some(engine) = engine {
        let elements: Vec<&afrobim_kernel::Element> = doc.elements().iter().collect();
        let signs = crate::signs_glue::definitions(&elements);
        if !signs.is_empty() {
            let (_, summary, count, area) = crate::signs_glue::quantities(engine, &signs);
            out.push(Block::Para(format!("{count} sign face(s), {area:.3} m2 in all.")));
            out.push(Block::Table(table_from_csv("Sign faces by ground colour", &summary)));
        }
    }
    let model = afrobim_sheets::Model { doc, registry };
    let t = afrobim_sheets::module_quantities(&model, "Quantities reported by the road elements", None, &["plan_origin_x_m", "plan_origin_y_m"]);
    if !t.rows.is_empty() {
        out.push(Block::Table(t));
    }
    (!out.is_empty()).then_some(out)
}

fn estimate_blocks(doc: &Document, engine: Option<&afrobim_signs::SignEngine>) -> Option<Vec<Block>> {
    let (id, _) = crate::cost_glue::list(doc).into_iter().next()?;
    let est = crate::cost_glue::load(doc, id)?;
    let signs = crate::cost_glue::sign_rows(engine, doc);
    let lines = crate::cost_glue::collect(doc, &signs);
    let mut out = Vec::new();
    match est.compute(&lines) {
        Err(e) => out.push(Block::Para(format!("The estimate does not price: {e}"))),
        Ok(r) => {
            out.push(Block::Para(format!("Estimate '{}', revision {}, in {}. Rates and additions are the user's; none comes from a standard.", r.name, r.revision, r.reporting.code)));
            out.push(Block::Table(
                Table::new(&["Section", "Title", "Items", "Amount"], r.sections.iter().map(|s| vec![s.code.clone(), s.title.clone(), s.items.to_string(), s.amount_reporting.to_string()]).collect()).with_title("Bill sections"),
            ));
            let mut totals = vec![vec!["Bill total before additions".to_string(), r.subtotal.to_string()]];
            totals.extend(r.additions.iter().map(|a| vec![a.name.clone(), a.amount.to_string()]));
            totals.push(vec!["Grand total".to_string(), r.grand_total.to_string()]);
            out.push(Block::Table(Table::new(&["Line", "Amount"], totals).with_title("Totals")));
            out.push(Block::Para(format!("{} item(s) unpriced.", r.unpriced.len())));
        }
    }
    Some(out)
}

fn programme_blocks(doc: &Document) -> Option<Vec<Block>> {
    let (_, p) = crate::schedule_glue::load(doc)?;
    let mut out = Vec::new();
    match afrobim_schedule::schedule_with(&p, &afrobim_schedule::ScheduleOptions { critical: crate::schedule_glue::critical_basis_of(doc) }) {
        Err(e) => out.push(Block::Para(format!("The programme does not schedule: {}", match e {
            afrobim_schedule::ScheduleError::Invalid(issues) => issues.into_iter().map(|i| i.message).collect::<Vec<_>>().join("; "),
            afrobim_schedule::ScheduleError::Other(m) => m,
        }))),
        Ok(s) => {
            out.push(Block::Para(format!("Programme '{}': start {}, finish {}, {} activities. Durations and constraints are the user's.", p.name, s.project_start, s.project_finish, s.rows.len())));
            let mut t = table_from_csv("Activities", &afrobim_schedule::activity_table_csv(&p, &s));
            t.title = Some("Activities".into());
            out.push(Block::Table(t));
            if !s.issues.is_empty() {
                out.push(Block::Para("Findings of the scheduler".into()));
                out.push(Block::Lines(s.issues.iter().map(|i| i.message.clone()).collect()));
            }
        }
    }
    Some(out)
}

// ─── the PDF ────────────────────────────────────────────────────────────────────────────

const W: f64 = 210.0;
const H: f64 = 297.0;
const ML: f64 = 18.0;
const MR: f64 = 18.0;
const TOP: f64 = H - 24.0;
const BOTTOM: f64 = 22.0;

struct Pager<'a> {
    styles: &'a Styles,
    pages: Vec<ComposedSheet>,
    y: f64,
}

impl Pager<'_> {
    fn new_page(&mut self) {
        let n = self.pages.len() + 1;
        self.pages.push(ComposedSheet::new(&n.to_string(), "Calculation report pack", W, H));
        self.y = TOP;
    }

    fn page(&mut self) -> &mut ComposedSheet {
        if self.pages.is_empty() {
            self.new_page();
        }
        self.pages.last_mut().expect("a page exists")
    }

    fn ensure(&mut self, need: f64) {
        if self.pages.is_empty() || self.y - need < BOTTOM {
            self.new_page();
        }
    }

    fn gap(&mut self, mm: f64) {
        self.y -= mm;
    }

    fn text(&mut self, s: &str, h: f64, bold: bool, indent: f64) {
        for line in wrap(s, W - ML - MR - indent, h, bold) {
            self.ensure(h * 1.6);
            self.y -= h * 1.5;
            let y = self.y;
            self.page().text(sheet_text(layers::TEXT, Pt2::new(ML + indent, y), h, line, Align::Left, 0.0, bold, [0, 0, 0]));
        }
    }

    fn table(&mut self, t: &Table) {
        let layout = layout_table(t, W - ML - MR, self.styles);
        let mut first = 0;
        if t.rows.is_empty() {
            return;
        }
        while first < t.rows.len() {
            self.ensure(30.0);
            let mut n = rows_that_fit(&layout, first, self.y - BOTTOM).min(t.rows.len() - first);
            if n == 0 {
                if self.y < TOP - 1.0 {
                    self.new_page();
                    continue;
                }
                n = 1;
            }
            let area = Rect::new(ML, BOTTOM, W - MR, self.y);
            let styles = self.styles;
            let used = draw_table(self.page(), t, &layout, first, n, &area, styles);
            self.y -= used + 3.0;
            first += n;
            if first < t.rows.len() {
                self.new_page();
            }
        }
    }

    fn blocks(&mut self, blocks: &[Block]) {
        let t = self.styles.text;
        for b in blocks {
            match b {
                Block::Heading(s) => {
                    self.gap(2.0);
                    self.text(s, t.heading, true, 0.0);
                }
                Block::Para(s) => self.text(s, t.normal, false, 0.0),
                Block::Lines(lines) => {
                    for l in lines {
                        self.text(l, t.normal, false, 3.0);
                    }
                }
                Block::Table(tab) => {
                    self.gap(1.0);
                    self.table(tab);
                }
            }
        }
    }
}

/// Lays the pack out on A4 portrait pages. The contents list needs the page of each section, so
/// the layout runs twice: the second time with the pages the first one found.
pub fn compose(pack: &Pack) -> Vec<ComposedSheet> {
    let mut starts = vec![0usize; pack.sections.len()];
    let mut pages = lay_out(pack, &starts).0;
    for _ in 0..2 {
        let (p, s) = lay_out(pack, &starts);
        pages = p;
        if s == starts {
            break;
        }
        starts = s;
    }
    let total = pages.len();
    let styles = Styles::default();
    for (i, sheet) in pages.iter_mut().enumerate() {
        let h = styles.text.small;
        sheet.text(sheet_text(layers::TEXT, Pt2::new(ML, H - 12.0), styles.text.normal, pack.project.clone(), Align::Left, 0.0, true, [0, 0, 0]));
        sheet.text(sheet_text(layers::TEXT, Pt2::new(W - MR, H - 12.0), h, "Calculation report pack", Align::Right, 0.0, false, [0, 0, 0]));
        sheet.line(layers::FRAME, Pt2::new(ML, H - 15.0), Pt2::new(W - MR, H - 15.0), 0.25, [0, 0, 0]);
        sheet.line(layers::FRAME, Pt2::new(ML, 16.0), Pt2::new(W - MR, 16.0), 0.25, [0, 0, 0]);
        sheet.text(sheet_text(layers::TEXT, Pt2::new(ML, 11.0), h, format!("{}   Composed {}", pack.project, pack.composed), Align::Left, 0.0, false, [0, 0, 0]));
        sheet.text(sheet_text(layers::TEXT, Pt2::new(W - MR, 11.0), h, format!("Page {} of {}", i + 1, total), Align::Right, 0.0, false, [0, 0, 0]));
        sheet.text(sheet_text(layers::TEXT, Pt2::new(ML, 7.0), h * 0.9, "A record of calculations for checking by an engineer; not a certification.", Align::Left, 0.0, false, [90, 90, 90]));
    }
    pages
}

fn lay_out(pack: &Pack, starts: &[usize]) -> (Vec<ComposedSheet>, Vec<usize>) {
    let styles = Styles::default();
    let t = styles.text;
    let mut p = Pager { styles: &styles, pages: Vec::new(), y: TOP };
    p.new_page();
    // Cover.
    p.gap(30.0);
    p.text("Calculation report pack", t.title * 1.6, true, 0.0);
    p.gap(4.0);
    p.text(&pack.project, t.title, true, 0.0);
    p.gap(8.0);
    let info = &pack.info;
    let mut fields: Vec<(String, String)> = vec![
        ("Project".into(), pack.project.clone()),
        ("Client".into(), info.client.clone()),
        ("Contract number".into(), info.contract_number.clone()),
        ("Document".into(), pack.document.clone()),
        ("Revision".into(), info.current_revision()),
        ("Prepared by".into(), info.drawn_by.clone()),
        ("Checked by".into(), info.checked_by.clone()),
        ("Approved by".into(), info.approved_by.clone()),
        ("Date composed".into(), pack.composed.clone()),
        ("Sign design standard".into(), pack.standard.clone()),
    ];
    fields.retain(|(_, v)| !v.trim().is_empty());
    for (k, v) in fields {
        p.text(&format!("{k}: {v}"), t.heading, false, 0.0);
    }
    p.gap(12.0);
    p.text(STATEMENT, t.heading, true, 0.0);
    p.gap(6.0);
    p.text("Every calculation line in this pack is as the engine wrote it, with the warnings it raised. A status of Passes means the engine's own lines passed on the inputs stored with the element. Inputs the engineer has not entered are listed as input needed; nothing is assumed in their place.", t.normal, false, 0.0);
    p.text(&format!("Composed by AfroBIM {}.", env!("CARGO_PKG_VERSION")), t.normal, false, 0.0);

    // Contents.
    p.new_page();
    p.text("Contents", t.title, true, 0.0);
    p.gap(3.0);
    let contents_y: Vec<(usize, f64)> = Vec::new();
    let _ = contents_y;
    for (i, s) in pack.sections.iter().enumerate() {
        p.ensure(t.normal * 1.6);
        p.y -= t.normal * 1.7;
        let y = p.y;
        let label = format!("{}. {}", i + 1, s.title);
        match &s.body {
            Some(_) => {
                let page = starts.get(i).copied().unwrap_or(0);
                let sheet = p.page();
                sheet.text(sheet_text(layers::TEXT, Pt2::new(ML, y), t.normal, label, Align::Left, 0.0, false, [0, 0, 0]));
                sheet.text(sheet_text(layers::TEXT, Pt2::new(W - MR, y), t.normal, if page == 0 { String::new() } else { page.to_string() }, Align::Right, 0.0, false, [0, 0, 0]));
            }
            None => {
                let sheet = p.page();
                sheet.text(sheet_text(layers::TEXT, Pt2::new(ML, y), t.normal, format!("{label}: not included: no data"), Align::Left, 0.0, false, [90, 90, 90]));
            }
        }
    }

    // Sections with data, each starting on a new page.
    let mut found = vec![0usize; pack.sections.len()];
    for (i, s) in pack.sections.iter().enumerate() {
        let Some(body) = &s.body else { continue };
        p.new_page();
        found[i] = p.pages.len();
        p.text(&format!("{}. {}", i + 1, s.title), t.title, true, 0.0);
        p.gap(2.0);
        p.blocks(body);
    }
    (p.pages, found)
}

/// The pack as one PDF. Returns the bytes and the number of pages.
pub fn pdf(pack: &Pack) -> (Vec<u8>, usize) {
    let pages = compose(pack);
    let n = pages.len();
    (afrobim_sheets::write_pdf(&pages, &format!("{}: calculation report pack", pack.project)), n)
}

// ─── plain text and CSV ─────────────────────────────────────────────────────────────────

fn table_text(t: &Table) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(title) = &t.title {
        out.push(title.clone());
    }
    let n = t.columns();
    let cell = |r: &Vec<String>, c: usize| r.get(c).cloned().unwrap_or_default();
    let mut widths = vec![0usize; n];
    for c in 0..n {
        widths[c] = t.headers.get(c).map(|h| h.chars().count()).unwrap_or(0).max(t.rows.iter().map(|r| cell(r, c).chars().count()).max().unwrap_or(0)).min(60);
    }
    let fmt = |r: &Vec<String>| (0..n).map(|c| format!("{:<w$}", cell(r, c), w = widths[c])).collect::<Vec<_>>().join(" | ").trim_end().to_string();
    out.push(fmt(&t.headers));
    out.extend(t.rows.iter().map(fmt));
    out
}

/// The plain-text variant.
pub fn text(pack: &Pack) -> String {
    let mut out = String::new();
    out.push_str(&format!("CALCULATION REPORT PACK: {}\n{}\n", pack.project, "=".repeat(60)));
    out.push_str(&format!("Composed {}. Document: {}. Sign design standard: {}.\n{}\n\n", pack.composed, pack.document, pack.standard, STATEMENT));
    out.push_str("Contents\n");
    for (i, s) in pack.sections.iter().enumerate() {
        out.push_str(&format!("  {}. {}{}\n", i + 1, s.title, if s.body.is_none() { ": not included: no data" } else { "" }));
    }
    for (i, s) in pack.sections.iter().enumerate() {
        let Some(body) = &s.body else { continue };
        out.push_str(&format!("\n{}\n{}. {}\n{}\n", "-".repeat(60), i + 1, s.title, "-".repeat(60)));
        for b in body {
            match b {
                Block::Heading(h) => out.push_str(&format!("\n{h}\n")),
                Block::Para(p) => out.push_str(&format!("{p}\n")),
                Block::Lines(l) => l.iter().for_each(|x| out.push_str(&format!("  {x}\n"))),
                Block::Table(t) => {
                    out.push('\n');
                    table_text(t).iter().for_each(|x| out.push_str(&format!("  {x}\n")));
                }
            }
        }
    }
    out
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// The CSV variant: one row per check with the section, the inputs and the calculation lines (as
/// the engine wrote them, joined by line breaks inside a quoted field).
pub fn csv(pack: &Pack) -> String {
    let mut out = String::from("Section,Mark,Category,Check,Status,Reason,Inputs,Calculation,Messages\n");
    for r in &pack.rows {
        let fields = [r.section.as_str(), r.mark.as_str(), r.category.as_str(), r.check.as_str(), r.status, r.reason.as_str(), &r.inputs.join("\n"), &r.lines.join("\n"), &r.notes.join("\n")];
        out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// Writes the three variants beside `path` (the one with the extension given gets that variant; the
/// others take the same stem). Returns what was written.
pub fn write_all(pack: &Pack, path: &std::path::Path) -> Result<Vec<String>, String> {
    let stem = path.with_extension("");
    let mut out = Vec::new();
    let (bytes, pages) = pdf(pack);
    for (ext, data) in [("pdf", bytes), ("txt", text(pack).into_bytes()), ("csv", csv(pack).into_bytes())] {
        let p = stem.with_extension(ext);
        std::fs::write(&p, data).map_err(|e| format!("Could not write {}: {e}", p.display()))?;
        out.push(format!("{}{}", p.display(), if ext == "pdf" { format!(" ({pages} pages)") } else { String::new() }));
    }
    Ok(out)
}

/// Statuses that count as findings, for the summary line.
pub fn summary_line(pack: &Pack) -> String {
    let count = |s: Status| pack.rows.iter().filter(|r| r.status == s.label()).count();
    format!(
        "{} check(s): {} pass, {} fail, {} differ, {} need input, {} not checked; {} of {} section(s) included.",
        pack.rows.len(),
        count(Status::Passes),
        count(Status::Fails),
        count(Status::Differs),
        count(Status::InputNeeded),
        count(Status::NotChecked),
        pack.sections.iter().filter(|s| s.body.is_some()).count(),
        pack.sections.len()
    )
}

// ─── the window ─────────────────────────────────────────────────────────────────────────

use egui::{Color32, RichText};

use crate::app::AfroBimApp;

/// What the report pack window keeps.
#[derive(Default)]
pub struct PackUi {
    pub open: bool,
    pub pack: Option<Pack>,
    pub pages: usize,
    pub message: String,
}

impl AfroBimApp {
    /// Composes the pack from the live document.
    pub(crate) fn pack_compose(&mut self) {
        self.sync_generated();
        let pack = build(&self.doc, &self.modules, self.engine.as_deref(), &crate::sheets_glue::now_text(), &self.title_for_pack(), &self.standard_label());
        self.pack.pages = compose(&pack).len();
        self.status = format!("Report pack composed: {}", summary_line(&pack));
        self.pack.pack = Some(pack);
    }

    pub(crate) fn pack_open(&mut self) {
        self.pack.open = true;
        self.pack_compose();
    }

    pub(crate) fn pack_export_dialog(&mut self) {
        let Some(pack) = self.pack.pack.clone() else { return };
        let Some(mut path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name("calculation-report-pack.pdf").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        self.pack.message = match write_all(&pack, &path) {
            Ok(w) => format!("Wrote {}.", w.join(", ")),
            Err(e) => e,
        };
        self.status = self.pack.message.clone();
    }

    pub(crate) fn pack_windows(&mut self, ctx: &egui::Context) {
        if !self.pack.open {
            return;
        }
        let mut open = true;
        let (mut export, mut recompose) = (false, false);
        crate::win::new(ctx, "Calculation report pack").open(&mut open).default_width(640.0).default_height(520.0).show(ctx, |ui| {
            ui.label(RichText::new(STATEMENT).strong());
            ui.label(RichText::new("One A4 portrait PDF, with a plain-text and a CSV variant beside it. Every calculation line is as the engine wrote it; sections with no data are listed as not included.").small().weak());
            let Some(pack) = &self.pack.pack else { return };
            ui.label(format!("Composed {} for '{}': {} page(s). {}", pack.composed, pack.project, self.pack.pages, summary_line(pack)));
            ui.horizontal(|ui| {
                if ui.button("Compose again").clicked() {
                    recompose = true;
                }
                if ui.button("Export PDF, text and CSV...").clicked() {
                    export = true;
                }
            });
            if !self.pack.message.is_empty() {
                ui.label(RichText::new(&self.pack.message).small());
            }
            ui.separator();
            for (i, s) in pack.sections.iter().enumerate() {
                match &s.body {
                    Some(_) => ui.label(format!("{}. {}", i + 1, s.title)),
                    None => ui.label(RichText::new(format!("{}. {}: not included: no data", i + 1, s.title)).color(Color32::from_rgb(0x8A, 0x91, 0x99))),
                };
            }
        });
        self.pack.open = open;
        if recompose {
            self.pack_compose();
        }
        if export {
            self.pack_export_dialog();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets_glue::test_support::{demo_doc, registry, scratch};

    #[test]
    fn the_demo_pack_has_the_expected_sections_pages_and_marks_the_empty_ones() {
        let mut doc = demo_doc();
        // As the app's demo does: a side drain, a culvert and a portal gantry on the first alignment.
        let (aid, _) = crate::alignment_glue::alignments(&doc).remove(0);
        let a = crate::alignment_glue::alignment_of(&doc, aid).unwrap();
        for (kind, from, to, offset) in [(0, 0.05, 0.45, 7.0), (1, 0.25, 0.25, 0.0), (2, 0.35, 0.35, 0.0)] {
            crate::modules_glue::add(&mut doc, kind, aid, a.start_station_m + a.length() * from, a.start_station_m + a.length() * to, offset).unwrap();
        }
        let reg = registry();
        let pack = build(&doc, &reg, None, "2026-11-14 12:00 UTC", "demo.afrobim", "UK-TSM-2019");
        let titles: Vec<&str> = pack.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles.len(), 14);
        let body = |t: &str| pack.sections.iter().find(|s| s.title.starts_with(t)).unwrap().body.is_some();
        // The demo has drains, a culvert, a gantry, pavement, barrier, lighting and an alignment...
        for present in ["Design basis", "Drainage", "Pavement", "Restraint", "Lighting", "Gantries", "Clash summary", "Quantities summary", "Estimate summary", "Programme summary"] {
            assert!(body(present), "{present} should have data in the demo model");
        }
        // ...but no signs here, so the sign sections are empty and listed as such.
        assert!(!body("Supports") && !body("Sign visibility"));
        let (bytes, pages) = pdf(&pack);
        assert!(bytes.starts_with(b"%PDF") && pages >= 8, "{pages} pages");
        // The page count is stable for the same model, and each page says "of m".
        let sheets = compose(&pack);
        assert_eq!(sheets.len(), pages);
        for (i, s) in sheets.iter().enumerate() {
            let texts: Vec<String> = s.texts().map(|t| t.text.clone()).collect();
            assert!(texts.iter().any(|t| t == &format!("Page {} of {}", i + 1, pages)), "page {} lacks its number", i + 1);
            assert!(texts.iter().any(|t| t.contains("2026-11-14 12:00 UTC")), "page {} lacks the date", i + 1);
            assert!(texts.iter().any(|t| t == &pack.project), "page {} lacks the project", i + 1);
            assert_eq!((s.width_mm, s.height_mm), (210.0, 297.0));
        }
        let cover: Vec<String> = sheets[0].texts().map(|t| t.text.clone()).collect();
        assert!(cover.iter().any(|t| t.contains("not a certification")), "the cover carries the statement");
        let contents: Vec<String> = sheets[1].texts().map(|t| t.text.clone()).collect();
        assert!(contents.iter().any(|t| t.contains("Supports: not included: no data")), "{contents:?}");
        // The text and CSV variants.
        let t = text(&pack);
        assert!(t.contains("not included: no data") && t.contains("CALCULATION REPORT PACK"));
        assert!(csv(&pack).starts_with("Section,Mark,Category,Check,Status"));
        // Calculation lines are carried exactly as the engine returned them.
        let report = crate::sheets_glue::check_report(&doc, &reg);
        let row = report.rows.iter().find(|r| !r.lines.is_empty()).expect("a row with lines");
        assert!(pack.rows.iter().any(|r| r.mark == row.mark && r.lines == row.lines));
        // Written to disk, all three variants.
        let path = scratch("pack.pdf");
        let written = write_all(&pack, &path).unwrap();
        assert_eq!(written.len(), 3);
        assert!(std::fs::read(path.with_extension("txt")).unwrap().len() > 1000);
    }
}
