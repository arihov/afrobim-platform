//! Turning schedule rows into a plan: what would be created, or why a row cannot be.
//!
//! Nothing here touches a document and nothing here needs the sign engine (fonts). The inputs
//! are the parsed file, the column mapping, the rule pack's catalog entries (what
//! `SignEngine::catalog()` returns), the document's alignments and the marks already in the
//! document.

use std::collections::{BTreeMap, HashMap};

use afrobim_alignment::{
    place_with, Alignment, PlaceOptions, PlacementError, SignLocation as AlignLocation, SignSide, StationEquationError, StationReference, TrafficSide,
};
use afrobim_kernel::{Document, ElementId, SitePlacement};
use afrobim_signs::{definition_from_entry, CatalogEntry, SignDefinition, SignLocation};

use super::columns::{ColumnMapping, Field, LengthUnit};
use super::csv::CsvTable;
use super::params::{apply_param, is_known_param, parse_legend, takes_legend};
use super::{PROP_DESCRIPTION, PROP_MOUNTING_HEIGHT_MM, PROP_NOTES, PROP_SIZE_MM, SIGN_CATEGORY};

/// A sign already in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct ExistingSign {
    pub id: ElementId,
    pub mark: String,
}

/// The signs (elements of category `sign`) of a document, by mark.
pub fn existing_signs(doc: &Document) -> Vec<ExistingSign> {
    doc.elements().iter().filter(|e| e.category == SIGN_CATEGORY).map(|e| ExistingSign { id: e.id, mark: e.name.clone() }).collect()
}

/// The alignments of a document, read from its alignment elements. An element whose data cannot
/// be read as an alignment is left out.
pub fn alignments_in(doc: &Document) -> Vec<Alignment> {
    doc.elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok())
        .collect()
}

/// What the planner reads besides the file.
#[derive(Debug, Clone, Copy)]
pub struct PlanContext<'a> {
    /// The rule pack's catalog: `SignEngine::catalog()`.
    pub catalog: &'a [CatalogEntry],
    pub alignments: &'a [Alignment],
    pub existing: &'a [ExistingSign],
    /// Which side traffic keeps to; it decides which way a sign faces. Left for the UK and Uganda.
    pub traffic: TrafficSide,
}

/// A file-level problem: no row can be planned.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlanError {
    #[error("the mapping leaves required column(s) unmapped or absent from the file: {}", .0.iter().map(Field::key).collect::<Vec<_>>().join(", "))]
    MissingColumns(Vec<Field>),
}

/// The class of a problem with a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// A required cell is empty.
    MissingValue,
    /// The record has more cells than the header has columns.
    ExtraCells,
    /// The mark was already used by an earlier row of the file.
    DuplicateMarkInFile,
    /// The code names no catalog entry, or (by diagram number) more than one.
    UnknownCode,
    /// No alignment of that name, or more than one.
    AlignmentNotFound,
    /// A number could not be read.
    BadNumber,
    /// A station could not be read.
    BadStation,
    /// The station is not on the alignment (beyond its ends, or in a gap).
    StationOutOfRange,
    /// The station occurs more than once under the station equations and no region was given.
    StationAmbiguous,
    /// The region does not exist on the alignment, or the station is not in it.
    RegionInvalid,
    /// The side is not left or right.
    BadSide,
    /// A `param:` column the schedule does not know.
    UnknownParam,
    /// A parameter value that is not accepted.
    BadValue,
    /// The sign type needs a legend and none was given.
    MissingRequiredParam,
    /// A legend was given for a sign type that has none.
    LegendNotAccepted,
    /// The placement failed for another reason (profile, ...).
    PlacementFailed,
}

/// One reason a row cannot be created.
#[derive(Debug, Clone, PartialEq)]
pub struct RowIssue {
    pub kind: IssueKind,
    /// The file's header of the column concerned, when there is one.
    pub column: Option<String>,
    pub message: String,
}

/// Where the sign is to stand, as the row states it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementRequest {
    /// The alignment's own name (the file's spelling is matched without regard to case).
    pub alignment: String,
    /// The station as displayed on the drawings, m.
    pub station_m: f64,
    pub region: Option<usize>,
    pub side: SignSide,
    /// Distance from the centreline, m, never negative.
    pub offset_m: f64,
    /// The internal (continuous) station it resolves to, m.
    pub internal_station_m: f64,
}

/// A sign the plan would create, with everything but its layout.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedSign {
    /// The line of the file the row starts on.
    pub row: usize,
    pub mark: String,
    /// The catalog key the code resolved to.
    pub catalog_key: String,
    pub catalog_name: String,
    pub description: Option<String>,
    /// The definition: the catalog's seed, the parameters, then the legend.
    pub definition: SignDefinition,
    pub placement: PlacementRequest,
    /// The site placement the alignment gives, with the rotation override applied.
    pub site: SitePlacement,
    /// The element properties the schedule sets (see the module constants of `sign_schedule`).
    pub properties: BTreeMap<String, String>,
    /// The element already carrying this mark, if any.
    pub existing: Option<ElementId>,
    /// Things worth knowing that do not stop the row.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowOutcome {
    Valid(Box<PlannedSign>),
    Invalid(Vec<RowIssue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanRow {
    /// The line of the file the record starts on (the header is usually line 1).
    pub row: usize,
    /// The mark as written, empty if the cell was.
    pub mark: String,
    pub outcome: RowOutcome,
}

/// The plan: one entry per data record, in file order. No record is left out.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    pub rows: Vec<PlanRow>,
    pub delimiter: char,
    /// Lines that were comments (first cell starts with `#`) and so are not data.
    pub comment_lines: Vec<usize>,
    /// Lines with nothing on them.
    pub blank_lines: Vec<usize>,
    /// File headers the mapping does not read.
    pub unread_headers: Vec<String>,
    /// Mapped headers that are not in the file (their field is absent, which is only allowed
    /// for optional fields).
    pub mapped_headers_not_in_file: Vec<String>,
}

impl ImportPlan {
    pub fn valid(&self) -> impl Iterator<Item = &PlannedSign> {
        self.rows.iter().filter_map(|r| match &r.outcome {
            RowOutcome::Valid(p) => Some(p.as_ref()),
            RowOutcome::Invalid(_) => None,
        })
    }

    pub fn invalid(&self) -> impl Iterator<Item = (&PlanRow, &[RowIssue])> {
        self.rows.iter().filter_map(|r| match &r.outcome {
            RowOutcome::Invalid(i) => Some((r, i.as_slice())),
            RowOutcome::Valid(_) => None,
        })
    }

    /// Valid rows whose mark is already in the document.
    pub fn duplicates(&self) -> impl Iterator<Item = &PlannedSign> {
        self.valid().filter(|p| p.existing.is_some())
    }

    pub fn valid_count(&self) -> usize {
        self.valid().count()
    }

    pub fn invalid_count(&self) -> usize {
        self.rows.len() - self.valid_count()
    }

    /// One line per problem, `row N: message`, for a report.
    pub fn error_lines(&self) -> Vec<String> {
        self.invalid().flat_map(|(r, issues)| issues.iter().map(move |i| format!("row {}: {}", r.row, i.message))).collect()
    }
}

/// Plans an import. Fails only when a required column cannot be read at all; every other problem
/// is a [`RowIssue`] on its row.
pub fn plan_import(table: &CsvTable, mapping: &ColumnMapping, ctx: &PlanContext) -> Result<ImportPlan, PlanError> {
    // Resolve each mapped column to its place in the file.
    struct Col {
        index: usize,
        header: String,
        unit: Option<LengthUnit>,
    }
    let mut cols: HashMap<Field, Col> = HashMap::new();
    let mut not_in_file = Vec::new();
    for entry in &mapping.columns {
        match table.headers.iter().position(|h| h.trim().eq_ignore_ascii_case(entry.header.trim())) {
            Some(index) => {
                cols.entry(entry.field.clone()).or_insert(Col { index, header: table.headers[index].clone(), unit: entry.unit });
            }
            None => not_in_file.push(entry.header.clone()),
        }
    }
    let missing: Vec<Field> = Field::REQUIRED.iter().filter(|f| !cols.contains_key(*f)).cloned().collect();
    if !missing.is_empty() {
        return Err(PlanError::MissingColumns(missing));
    }
    let read: Vec<usize> = cols.values().map(|c| c.index).collect();
    let unread_headers: Vec<String> =
        table.headers.iter().enumerate().filter(|(i, h)| !read.contains(i) && !h.trim().is_empty()).map(|(_, h)| h.clone()).collect();

    let mut seen_marks: HashMap<String, usize> = HashMap::new();
    let mut rows = Vec::with_capacity(table.rows.len());
    for rec in &table.rows {
        let cell = |f: &Field| -> Option<&str> {
            let c = cols.get(f)?;
            rec.cells.get(c.index).map(|s| s.trim()).filter(|s| !s.is_empty())
        };
        let header_of = |f: &Field| cols.get(f).map(|c| c.header.clone());
        let mut issues: Vec<RowIssue> = Vec::new();
        let mut issue = |kind: IssueKind, f: Option<&Field>, message: String| {
            issues.push(RowIssue { kind, column: f.and_then(&header_of), message });
        };

        if rec.cells.len() > table.headers.len() && rec.cells[table.headers.len()..].iter().any(|c| !c.trim().is_empty()) {
            issue(
                IssueKind::ExtraCells,
                None,
                format!("the row has {} cells but the header has {} columns; the extra cells hold data", rec.cells.len(), table.headers.len()),
            );
        }

        // Mark.
        let mark = cell(&Field::Mark).unwrap_or("").to_string();
        if mark.is_empty() {
            issue(IssueKind::MissingValue, Some(&Field::Mark), "the mark is empty".into());
        } else if let Some(first) = seen_marks.get(&mark.to_lowercase()) {
            issue(IssueKind::DuplicateMarkInFile, Some(&Field::Mark), format!("the mark '{mark}' is already used by row {first} of the file"));
        } else {
            seen_marks.insert(mark.to_lowercase(), rec.row_line());
        }

        // Code.
        let mut entry: Option<&CatalogEntry> = None;
        match cell(&Field::Code) {
            None => issue(IssueKind::MissingValue, Some(&Field::Code), "the sign code is empty".into()),
            Some(code) => match find_entry(ctx.catalog, code) {
                Ok(e) => entry = Some(e),
                Err(m) => issue(IssueKind::UnknownCode, Some(&Field::Code), m),
            },
        }

        // Alignment.
        let mut alignment: Option<&Alignment> = None;
        match cell(&Field::Alignment) {
            None => issue(IssueKind::MissingValue, Some(&Field::Alignment), "the alignment is empty".into()),
            Some(name) => {
                let found: Vec<&Alignment> = ctx.alignments.iter().filter(|a| a.name.trim().eq_ignore_ascii_case(name)).collect();
                match found.len() {
                    1 => alignment = Some(found[0]),
                    0 => issue(
                        IssueKind::AlignmentNotFound,
                        Some(&Field::Alignment),
                        format!(
                            "there is no alignment '{name}' in the document (it has: {})",
                            if ctx.alignments.is_empty() { "none".to_string() } else { ctx.alignments.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ") }
                        ),
                    ),
                    n => issue(IssueKind::AlignmentNotFound, Some(&Field::Alignment), format!("{n} alignments are named '{name}'")),
                }
            }
        }

        // Station and region.
        let mut station: Option<f64> = None;
        match cell(&Field::Station) {
            None => issue(IssueKind::MissingValue, Some(&Field::Station), "the station is empty".into()),
            Some(t) => match parse_station(t) {
                Ok(v) => station = Some(v),
                Err(m) => issue(IssueKind::BadStation, Some(&Field::Station), m),
            },
        }
        let mut region: Option<usize> = None;
        if let Some(t) = cell(&Field::Region) {
            match t.parse::<usize>() {
                Ok(r) => region = Some(r),
                Err(_) => issue(IssueKind::BadNumber, Some(&Field::Region), format!("the region '{t}' is not a whole number (0 is the region before the first station equation)")),
            }
        }

        // Side.
        let mut side: Option<SignSide> = None;
        match cell(&Field::Side) {
            None => issue(IssueKind::MissingValue, Some(&Field::Side), "the side is empty".into()),
            Some(t) => match t.to_lowercase().as_str() {
                "left" | "l" | "lhs" => side = Some(SignSide::Left),
                "right" | "r" | "rhs" => side = Some(SignSide::Right),
                "both" => issue(IssueKind::BadSide, Some(&Field::Side), "side 'both' is not accepted: give one row per sign, one left and one right".into()),
                other => issue(IssueKind::BadSide, Some(&Field::Side), format!("the side '{other}' is not left or right")),
            },
        }

        // Offset (m).
        let mut offset: Option<f64> = None;
        match cell(&Field::Offset) {
            None => issue(IssueKind::MissingValue, Some(&Field::Offset), "the offset from the centreline is empty".into()),
            Some(t) => match length(t, &Field::Offset, cols.get(&Field::Offset).and_then(|c| c.unit)) {
                Ok(mm) if mm < 0.0 => issue(
                    IssueKind::BadNumber,
                    Some(&Field::Offset),
                    format!("the offset '{t}' is negative; it is a distance from the centreline and the side says which side"),
                ),
                Ok(mm) => offset = Some(mm / 1000.0),
                Err(m) => issue(IssueKind::BadNumber, Some(&Field::Offset), m),
            },
        }

        // Optional numbers.
        let mut mounting: Option<f64> = None;
        if let Some(t) = cell(&Field::MountingHeight) {
            match length(t, &Field::MountingHeight, cols.get(&Field::MountingHeight).and_then(|c| c.unit)) {
                Ok(mm) if mm < 0.0 => issue(IssueKind::BadNumber, Some(&Field::MountingHeight), format!("the mounting height '{t}' is negative")),
                Ok(mm) => mounting = Some(mm),
                Err(m) => issue(IssueKind::BadNumber, Some(&Field::MountingHeight), m),
            }
        }
        let mut size: Option<f64> = None;
        if let Some(t) = cell(&Field::Size) {
            match length(t, &Field::Size, cols.get(&Field::Size).and_then(|c| c.unit)) {
                Ok(mm) if mm <= 0.0 => issue(IssueKind::BadNumber, Some(&Field::Size), format!("the size '{t}' is not positive")),
                Ok(mm) => size = Some(mm),
                Err(m) => issue(IssueKind::BadNumber, Some(&Field::Size), m),
            }
        }
        let mut rotation: Option<f64> = None;
        if let Some(t) = cell(&Field::Rotation) {
            match number(t) {
                Ok(v) => rotation = Some(v.rem_euclid(360.0)),
                Err(m) => issue(IssueKind::BadNumber, Some(&Field::Rotation), format!("rotation: {m}")),
            }
        }

        // Definition: catalog seed, parameters, legend.
        let mut definition: Option<SignDefinition> = None;
        let mut warnings: Vec<String> = Vec::new();
        if let Some(e) = entry {
            let mut def = definition_from_entry(e, None);
            let mut params: Vec<(&String, &Col)> = Vec::new();
            for (f, c) in &cols {
                if let Field::Param(p) = f {
                    params.push((p, c));
                }
            }
            params.sort_by_key(|(_, c)| c.index);
            for (name, c) in params {
                let Some(value) = rec.cells.get(c.index).map(|s| s.trim()).filter(|s| !s.is_empty()) else { continue };
                let f = Field::Param(name.clone());
                if !is_known_param(name) {
                    issue(IssueKind::UnknownParam, Some(&f), format!("there is no parameter '{name}' (column '{}')", c.header));
                } else if let Err(m) = apply_param(&mut def, name, value) {
                    issue(IssueKind::BadValue, Some(&f), format!("parameter {name}: {m}"));
                }
            }
            let legend = cell(&Field::Legend);
            if takes_legend(def.kind) {
                match legend {
                    None => issue(
                        IssueKind::MissingRequiredParam,
                        Some(&Field::Legend),
                        format!("the legend is required: a {:?} sign has lettered destinations", def.kind),
                    ),
                    Some(t) => match parse_legend(t) {
                        Ok(d) => def.destinations = d,
                        Err(m) => issue(IssueKind::BadValue, Some(&Field::Legend), format!("legend: {m}")),
                    },
                }
            } else if legend.is_some() {
                issue(IssueKind::LegendNotAccepted, Some(&Field::Legend), format!("a {:?} sign has no legend to set; clear the legend cell", def.kind));
            }
            if let (Some(mm), Some(sizes)) = (size, e.sizes.as_ref()) {
                if let Some(permitted) = &sizes.permitted_mm {
                    if !permitted.iter().any(|p| (p - mm).abs() < 0.5) {
                        warnings.push(format!("size {mm} mm is not one of the sizes the rule pack permits for this sign ({})", permitted.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")));
                    }
                }
            }
            definition = Some(def);
        }

        // Placement, once its inputs are all good.
        let mut placed: Option<(PlacementRequest, SitePlacement)> = None;
        if let (Some(a), Some(st), Some(sd), Some(off)) = (alignment, station, side, offset) {
            let location = AlignLocation { alignment: Some(a.name.clone()), station: Some(st), offset: Some(off), side: Some(sd) };
            let options = PlaceOptions { station: StationReference::Displayed { region }, ..PlaceOptions::default() };
            match place_with(a, &location, ctx.traffic, &options) {
                Ok(p) => match p.positions.first() {
                    Some(pos) => {
                        let mut site = pos.site_placement(&a.name);
                        if let Some(r) = rotation {
                            warnings.push(format!("rotation {r:.2} degrees given: it replaces the {:.2} degrees the alignment gives", site.rotation_deg));
                            site.rotation_deg = r;
                        }
                        placed = Some((
                            PlacementRequest { alignment: a.name.clone(), station_m: st, region, side: sd, offset_m: off, internal_station_m: pos.station_m },
                            site,
                        ));
                    }
                    None => issue(IssueKind::PlacementFailed, None, "the alignment gave no position for that station".into()),
                },
                Err(e) => {
                    let kind = match &e {
                        PlacementError::StationOffAlignment { .. } => IssueKind::StationOutOfRange,
                        PlacementError::Station { source, .. } => match source {
                            StationEquationError::Ambiguous { .. } => IssueKind::StationAmbiguous,
                            StationEquationError::NotOnAlignment { .. } => IssueKind::StationOutOfRange,
                            StationEquationError::NoSuchRegion { .. } | StationEquationError::NotInRegion { .. } => IssueKind::RegionInvalid,
                            _ => IssueKind::PlacementFailed,
                        },
                        _ => IssueKind::PlacementFailed,
                    };
                    let f = match kind {
                        IssueKind::RegionInvalid => Field::Region,
                        _ => Field::Station,
                    };
                    issue(kind, Some(&f), e.to_string());
                }
            }
        }

        let outcome = if issues.is_empty() {
            let (e, mut def, (req, site)) = (entry.unwrap(), definition.unwrap(), placed.unwrap());
            def.code = mark.clone();
            def.location = Some(SignLocation {
                alignment: Some(req.alignment.clone()),
                station: Some(req.station_m),
                offset: Some(req.offset_m),
                side: Some(match req.side {
                    SignSide::Right => afrobim_signs::SignSide::Right,
                    _ => afrobim_signs::SignSide::Left,
                }),
            });
            let mut properties = BTreeMap::new();
            let description = cell(&Field::Description).map(str::to_string);
            if let Some(d) = &description {
                properties.insert(PROP_DESCRIPTION.to_string(), d.clone());
            }
            if let Some(mm) = mounting {
                properties.insert(PROP_MOUNTING_HEIGHT_MM.to_string(), fmt_num(mm));
            }
            if let Some(mm) = size {
                properties.insert(PROP_SIZE_MM.to_string(), fmt_num(mm));
            }
            if let Some(n) = cell(&Field::Notes) {
                properties.insert(PROP_NOTES.to_string(), n.to_string());
            }
            let existing = ctx.existing.iter().find(|x| x.mark.eq_ignore_ascii_case(&mark)).map(|x| x.id);
            if existing.is_some() {
                warnings.push(format!("a sign with the mark '{mark}' is already in the document"));
            }
            RowOutcome::Valid(Box::new(PlannedSign {
                row: rec.row_line(),
                mark: mark.clone(),
                catalog_key: e.key.clone(),
                catalog_name: e.name.clone(),
                description,
                definition: def,
                placement: req,
                site,
                properties,
                existing,
                warnings,
            }))
        } else {
            RowOutcome::Invalid(issues)
        };
        rows.push(PlanRow { row: rec.row_line(), mark, outcome });
    }

    Ok(ImportPlan {
        rows,
        delimiter: table.delimiter,
        comment_lines: table.comment_lines.clone(),
        blank_lines: table.blank_lines.clone(),
        unread_headers,
        mapped_headers_not_in_file: not_in_file,
    })
}

trait RowLine {
    fn row_line(&self) -> usize;
}

impl RowLine for super::csv::CsvRow {
    fn row_line(&self) -> usize {
        self.line
    }
}

/// The catalog entry a code names: by key, then by diagram number, ignoring case.
fn find_entry<'a>(catalog: &'a [CatalogEntry], code: &str) -> Result<&'a CatalogEntry, String> {
    if let Some(e) = catalog.iter().find(|e| e.key.eq_ignore_ascii_case(code)) {
        return Ok(e);
    }
    let by_diagram: Vec<&CatalogEntry> = catalog.iter().filter(|e| e.diagram.as_deref().is_some_and(|d| d.trim().eq_ignore_ascii_case(code))).collect();
    match by_diagram.len() {
        1 => Ok(by_diagram[0]),
        0 => Err(format!("the code '{code}' is not a sign of the loaded rule pack")),
        n => Err(format!("the diagram number '{code}' names {n} signs of the rule pack ({}); use the catalog key", by_diagram.iter().map(|e| e.key.as_str()).collect::<Vec<_>>().join(", "))),
    }
}

/// Reads a plain number: `.` is the decimal mark, no thousands separator.
pub fn number(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.contains(',') {
        return Err(format!("'{t}' has a comma; write the decimal mark as '.' and no thousands separator"));
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(format!("'{t}' is not a number")),
    }
}

/// Reads a length in millimetres. A trailing `mm` or `m` in the cell says the unit; otherwise the
/// column's unit (from the mapping or the field's default) applies.
fn length(text: &str, field: &Field, column_unit: Option<LengthUnit>) -> Result<f64, String> {
    let t = text.trim();
    let lower = t.to_lowercase();
    let (num, unit) = if let Some(n) = lower.strip_suffix("mm") {
        (n.trim().to_string(), Some(LengthUnit::Millimetres))
    } else if let Some(n) = lower.strip_suffix('m') {
        (n.trim().to_string(), Some(LengthUnit::Metres))
    } else {
        (lower.clone(), None)
    };
    let unit = unit.or(column_unit).or(field.default_unit()).unwrap_or(LengthUnit::Metres);
    number(&num).map(|v| unit.to_mm(v))
}

/// Reads a station as the drawings show it: `k+mmm.mmm` (kilometres, then metres under 1000),
/// or plain metres (`12345.678`, optionally ending in `m`). A leading `-` makes it negative.
pub fn parse_station(text: &str) -> Result<f64, String> {
    let t = text.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b.trim()),
        None => (false, t),
    };
    let value = if let Some((km, m)) = body.split_once('+') {
        let (km, m) = (km.trim(), m.trim());
        if km.is_empty() || !km.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("the station '{t}': the part before '+' must be whole kilometres"));
        }
        let metres = number(m).map_err(|e| format!("the station '{t}': {e}"))?;
        if !(0.0..1000.0).contains(&metres) {
            return Err(format!("the station '{t}': the metres after '+' must be at least 0 and under 1000"));
        }
        let km: f64 = km.parse().map_err(|_| format!("the station '{t}': the kilometres are not a number"))?;
        km * 1000.0 + metres
    } else {
        let plain = body.strip_suffix('m').or_else(|| body.strip_suffix('M')).unwrap_or(body).trim();
        number(plain).map_err(|e| format!("the station '{t}': {e}"))?
    };
    Ok(if neg { -value } else { value })
}

/// A number to at most 3 decimals without trailing zeros.
pub(crate) fn fmt_num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stations_in_every_accepted_form() {
        assert_eq!(parse_station("12+345.678").unwrap(), 12345.678);
        assert_eq!(parse_station("0+250").unwrap(), 250.0);
        assert_eq!(parse_station("0+050.5").unwrap(), 50.5);
        assert_eq!(parse_station("12345.678").unwrap(), 12345.678);
        assert_eq!(parse_station("250 m").unwrap(), 250.0);
        assert_eq!(parse_station("-0+250").unwrap(), -250.0);
        assert_eq!(parse_station(" 1+000.000 ").unwrap(), 1000.0);
        assert!(parse_station("1+1000").is_err());
        assert!(parse_station("a+100").is_err());
        assert!(parse_station("1+2,5").is_err());
        assert!(parse_station("CH 1+100").is_err());
        assert!(parse_station("").is_err());
    }

    #[test]
    fn lengths_take_the_column_unit_unless_the_cell_says_otherwise() {
        assert_eq!(length("2.5", &Field::Offset, None).unwrap(), 2500.0);
        assert_eq!(length("2500", &Field::MountingHeight, None).unwrap(), 2500.0);
        assert_eq!(length("2.5", &Field::MountingHeight, Some(LengthUnit::Metres)).unwrap(), 2500.0);
        assert_eq!(length("2.5 m", &Field::MountingHeight, None).unwrap(), 2500.0);
        assert_eq!(length("2500mm", &Field::Offset, None).unwrap(), 2500.0);
        assert!(length("2,5", &Field::Offset, None).is_err());
        assert!(length("abc", &Field::Offset, None).is_err());
    }

    #[test]
    fn numbers_are_formatted_without_noise() {
        assert_eq!(fmt_num(2000.0), "2000");
        assert_eq!(fmt_num(2.5), "2.5");
        assert_eq!(fmt_num(0.1 + 0.2), "0.3");
    }
}
