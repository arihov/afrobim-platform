//! Sheet sets: definitions, numbering, series expansion, composition, and persistence in the
//! document as one element of category `sheets`.

use std::collections::BTreeMap;

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt, Severity};
use serde::{Deserialize, Serialize};

use crate::dxf_out::write_dxf_set;
use crate::item::{layers, ComposedSheet};
use crate::model::{station_text, Ctx, Model};
use crate::paper::Page;
use crate::pdf::write_pdf;
use crate::plan::{match_lines_for, plan_windows};
use crate::project::{project_info, ProjectInfo};
use crate::sections::{pick_sections, SectionsView};
use crate::style::Styles;
use crate::table::{layout_table, rows_that_fit, Table};
use crate::titleblock::{draw_title_block, SheetFields, TitleBlockLayout, TitleValues};
use crate::view::{compose_view, ViewDef, ViewKind};

/// The element category that holds a sheet set in a document.
pub const SHEETS_CATEGORY: &str = "sheets";
/// The layer the element is placed on.
pub const SHEETS_LAYER: &str = "Sheets";

/// How sheet numbers are made: `prefix`, then `start + i * step` padded with zeros to `digits`,
/// then `suffix`. All entered by the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Numbering {
    pub prefix: String,
    pub digits: usize,
    pub start: u32,
    pub step: u32,
    pub suffix: String,
}

impl Default for Numbering {
    /// Default only: 001, 002, 003 and so on, with no prefix.
    fn default() -> Self {
        Numbering { prefix: String::new(), digits: 3, start: 1, step: 1, suffix: String::new() }
    }
}

impl Numbering {
    /// The number of the sheet at position `index` (0-based) of the set.
    pub fn number(&self, index: usize) -> String {
        format!("{}{:0width$}{}", self.prefix, self.start as u64 + index as u64 * self.step as u64, self.suffix, width = self.digits)
    }
}

/// One sheet: its page, title block, own title block values and views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetSpec {
    pub title: String,
    pub page: Page,
    pub title_block: TitleBlockLayout,
    pub fields: SheetFields,
    pub views: Vec<ViewDef>,
}

impl SheetSpec {
    pub fn new(title: &str, page: Page, views: Vec<ViewDef>) -> SheetSpec {
        SheetSpec { title: title.to_string(), page, title_block: TitleBlockLayout::default(), fields: SheetFields::default(), views }
    }

    /// The rectangle the views share: the frame less the title block.
    pub fn views_area(&self) -> crate::geom::Rect {
        self.title_block.views_area(&self.page.frame_rect())
    }

    /// The scale text of the title block: the one scale of the views, or "AS SHOWN" when they differ.
    pub fn scale_text(&self) -> String {
        let scales: Vec<String> = self.views.iter().filter_map(|v| v.kind.scale()).map(|s| s.label()).collect();
        match scales.first() {
            None => String::new(),
            Some(first) if scales.iter().all(|s| s == first) => first.clone(),
            Some(_) => "AS SHOWN".to_string(),
        }
    }
}

/// A run of plan sheets along an alignment: "plan sheets every L metres with overlap".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSeries {
    /// The sheet every plan sheet is made from; its view number `plan_view` must be a plan view,
    /// whose station range is replaced sheet by sheet.
    pub template: SheetSpec,
    pub plan_view: usize,
    /// Start and end of the series (internal stations); `None` takes the alignment's own ends.
    pub from_station_m: Option<f64>,
    pub to_station_m: Option<f64>,
    /// Length of alignment on each sheet, m.
    pub length_m: f64,
    /// How far each sheet runs back over the one before it, m.
    pub overlap_m: f64,
}

/// A table that continues over as many sheets as its rows need, the header repeated on each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSeries {
    pub template: SheetSpec,
    pub table_view: usize,
}

/// The sections of a corridor, `per_sheet` to a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionSeries {
    pub template: SheetSpec,
    pub sections_view: usize,
    pub per_sheet: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SheetSource {
    Single(SheetSpec),
    PlanSeries(PlanSeries),
    TableSeries(TableSeries),
    SectionSeries(SectionSeries),
}

/// A sheet set: numbering, styles and the sheets (or series of sheets) in order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SheetSet {
    pub name: String,
    pub numbering: Numbering,
    pub styles: Styles,
    pub sources: Vec<SheetSource>,
}

impl SheetSet {
    pub fn new(name: &str) -> SheetSet {
        SheetSet { name: name.to_string(), ..SheetSet::default() }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a sheet set always serialises")
    }

    pub fn from_value(v: &serde_json::Value) -> Result<SheetSet, String> {
        serde_json::from_value(v.clone()).map_err(|e| format!("The sheet set cannot be read: {e}"))
    }

    /// Findings that need no model: series with impossible numbers, views missing from a template.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, s) in self.sources.iter().enumerate() {
            let at = format!("Source {}", i + 1);
            match s {
                SheetSource::Single(_) => {}
                SheetSource::PlanSeries(p) => {
                    if p.length_m.is_nan() || p.overlap_m.is_nan() || p.length_m <= 0.0 || p.overlap_m < 0.0 || p.overlap_m >= p.length_m {
                        out.push(format!("{at}: a plan series needs a positive length and an overlap of at least 0 and less than the length."));
                    }
                    if !matches!(p.template.views.get(p.plan_view).map(|v| &v.kind), Some(ViewKind::Plan(_))) {
                        out.push(format!("{at}: view {} of the template is not a plan view.", p.plan_view + 1));
                    }
                }
                SheetSource::TableSeries(t) => {
                    if !matches!(t.template.views.get(t.table_view).map(|v| &v.kind), Some(ViewKind::Table(_))) {
                        out.push(format!("{at}: view {} of the template is not a table.", t.table_view + 1));
                    }
                }
                SheetSource::SectionSeries(t) => {
                    if t.per_sheet == 0 {
                        out.push(format!("{at}: a section series needs at least one section per sheet."));
                    }
                    if !matches!(t.template.views.get(t.sections_view).map(|v| &v.kind), Some(ViewKind::Sections(_))) {
                        out.push(format!("{at}: view {} of the template is not a sections view.", t.sections_view + 1));
                    }
                }
            }
        }
        out
    }
}

/// A sheet of the set once series are expanded and numbers given.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedSheet {
    pub number: String,
    pub spec: SheetSpec,
}

/// Expands series into sheets and numbers them. The second value lists what could not be
/// expanded.
pub fn expand(model: &Model, set: &SheetSet) -> (Vec<ExpandedSheet>, Vec<String>) {
    let mut out: Vec<ExpandedSheet> = Vec::new();
    let mut warnings = set.validate();
    let styles = &set.styles;
    for source in &set.sources {
        let first = out.len();
        let number = |j: usize| set.numbering.number(first + j);
        match source {
            SheetSource::Single(spec) => out.push(ExpandedSheet { number: number(0), spec: spec.clone() }),
            SheetSource::PlanSeries(p) => {
                let Some(ViewKind::Plan(plan)) = p.template.views.get(p.plan_view).map(|v| &v.kind) else { continue };
                let alignment = match model.alignment(&plan.alignment) {
                    Ok(a) => a,
                    Err(e) => {
                        warnings.push(e);
                        continue;
                    }
                };
                let from = p.from_station_m.unwrap_or(alignment.start_station_m).max(alignment.start_station_m);
                let to = p.to_station_m.unwrap_or(alignment.end_station_m()).min(alignment.end_station_m());
                let Some(windows) = plan_windows(from, to, p.length_m, p.overlap_m) else {
                    warnings.push(format!("The plan series on '{}' cannot be made from {from:.3} to {to:.3} with these lengths.", plan.alignment));
                    continue;
                };
                let numbers: Vec<String> = (0..windows.len()).map(number).collect();
                for (i, (a, b)) in windows.iter().enumerate() {
                    let mut spec = p.template.clone();
                    if let ViewKind::Plan(v) = &mut spec.views[p.plan_view].kind {
                        v.from_station_m = *a;
                        v.to_station_m = *b;
                        v.match_lines = match_lines_for(&windows, i, &numbers);
                    }
                    let range = format!("STA {} TO {}", station_text(&alignment, *a, plan.station_decimals), station_text(&alignment, *b, plan.station_decimals));
                    spec.title = if p.template.title.is_empty() { format!("PLAN {range}") } else { format!("{} {range}", p.template.title) };
                    out.push(ExpandedSheet { number: numbers[i].clone(), spec });
                }
            }
            SheetSource::TableSeries(t) => {
                let Some(ViewKind::Table(table)) = t.template.views.get(t.table_view).map(|v| &v.kind) else { continue };
                let def = &t.template.views[t.table_view];
                let rect = t.template.views_area().fraction(&def.area).inset(1.0).inset(2.0);
                let layout = layout_table(table, rect.width(), styles);
                // Keep the column widths of the whole table on every sheet.
                let widths: Vec<f64> = layout.col_x.windows(2).map(|w| w[1] - w[0]).collect();
                let mut chunks: Vec<(usize, usize)> = Vec::new();
                let mut at = 0;
                while at < table.rows.len() {
                    let n = rows_that_fit(&layout, at, rect.height()).max(1);
                    chunks.push((at, n));
                    at += n;
                }
                if chunks.is_empty() {
                    chunks.push((0, 0));
                }
                let m = chunks.len();
                for (j, (a, n)) in chunks.into_iter().enumerate() {
                    let mut spec = t.template.clone();
                    if let ViewKind::Table(tab) = &mut spec.views[t.table_view].kind {
                        tab.rows = table.rows[a..a + n].to_vec();
                        tab.col_widths_mm = widths.clone();
                    }
                    if m > 1 {
                        spec.title = format!("{} ({} of {m})", t.template.title, j + 1);
                    }
                    out.push(ExpandedSheet { number: number(j), spec });
                }
            }
            SheetSource::SectionSeries(t) => {
                let Some(ViewKind::Sections(view)) = t.template.views.get(t.sections_view).map(|v| &v.kind) else { continue };
                if t.per_sheet == 0 {
                    continue;
                }
                let Some(id) = model.id_of(afrobim_corridor::CORRIDOR_CATEGORY, &view.corridor) else {
                    warnings.push(format!("There is no corridor called '{}' for a section series.", view.corridor));
                    continue;
                };
                let corridor = match afrobim_corridor::corridor_of(model.doc, id) {
                    Ok(c) => c,
                    Err(e) => {
                        warnings.push(format!("Corridor '{}' cannot be built for a section series: {e}", view.corridor));
                        continue;
                    }
                };
                let stations: Vec<f64> = pick_sections(&corridor.sections, view).iter().map(|s| s.station_m).collect();
                let chunks: Vec<&[f64]> = stations.chunks(t.per_sheet).collect();
                let m = chunks.len().max(1);
                for (j, chunk) in chunks.iter().enumerate() {
                    let mut spec = t.template.clone();
                    if let ViewKind::Sections(v) = &mut spec.views[t.sections_view].kind {
                        v.stations_m = Some(chunk.to_vec());
                    }
                    if m > 1 {
                        spec.title = format!("{} ({} of {m})", t.template.title, j + 1);
                    }
                    out.push(ExpandedSheet { number: number(j), spec });
                }
                let _: &SectionsView = view;
            }
        }
    }
    (out, warnings)
}

/// Composes one expanded sheet. `position` is its 1-based place in the set of `total` sheets.
pub fn compose_sheet(model: &Model, styles: &Styles, info: &ProjectInfo, sheet: &ExpandedSheet, position: usize, total: usize) -> ComposedSheet {
    let spec = &sheet.spec;
    let (w, h) = spec.page.dimensions_mm();
    let mut out = ComposedSheet::new(&sheet.number, &spec.title, w, h);
    if spec.page.frame.draw_paper_edge {
        let p = styles.pens.pen(layers::PAPER);
        out.rect(layers::PAPER, &spec.page.paper_rect(), p.weight_mm, p.rgb);
    }
    let frame = spec.page.frame_rect();
    if spec.page.frame.draw_frame {
        let p = styles.pens.pen(layers::FRAME);
        out.rect(layers::FRAME, &frame, p.weight_mm, p.rgb);
        if spec.title_block.placement == crate::titleblock::TitlePlacement::RightColumn {
            let x = spec.title_block.rect_in(&frame).x0;
            out.line(layers::FRAME, Pt2::new(x, frame.y0), Pt2::new(x, frame.y1), p.weight_mm, p.rgb);
        }
    }
    let area = spec.views_area();
    let ctx = Ctx { model, styles };
    for v in &spec.views {
        compose_view(&mut out, &ctx, v, &area);
    }
    let scale_text = spec.scale_text();
    let values = TitleValues { info, fields: &spec.fields, number: &sheet.number, title: &spec.title, scale_text: &scale_text, sheet: (position, total) };
    draw_title_block(&mut out, &spec.title_block, &spec.title_block.rect_in(&frame), &values, styles);
    out
}

/// A composed set.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedSet {
    pub sheets: Vec<ComposedSheet>,
    /// Everything that did not fit or could not be drawn, set-wide and per sheet (prefixed with
    /// the sheet number).
    pub warnings: Vec<String>,
}

/// Expands and composes a whole set from the live model, taking the project information from the
/// document.
pub fn compose_set(model: &Model, set: &SheetSet) -> ComposedSet {
    let info = project_info(model.doc);
    compose_set_with(model, set, &info)
}

/// The same with project information supplied by the caller.
pub fn compose_set_with(model: &Model, set: &SheetSet, info: &ProjectInfo) -> ComposedSet {
    let (expanded, mut warnings) = expand(model, set);
    let total = expanded.len();
    let mut sheets = Vec::with_capacity(total);
    for (i, e) in expanded.iter().enumerate() {
        let s = compose_sheet(model, &set.styles, info, e, i + 1, total);
        warnings.extend(s.warnings.iter().map(|w| format!("Sheet {}: {w}", s.number)));
        sheets.push(s);
    }
    ComposedSet { sheets, warnings }
}

impl ComposedSet {
    /// One PDF, a page per sheet.
    pub fn pdf(&self, title: &str) -> Vec<u8> {
        write_pdf(&self.sheets, title)
    }

    /// One DXF, the sheets side by side along X with `gap_mm` between them.
    pub fn dxf(&self, styles: &Styles, gap_mm: f64) -> String {
        write_dxf_set(&self.sheets, &styles.pens, gap_mm)
    }
}

/// The sheet-set element's module. It cannot compose sheets (a module sees only its own element,
/// not the document); it draws the sheets as outlines in a row and reports the set's findings.
/// Compose with [`compose_set`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SheetsModule;

impl Module for SheetsModule {
    fn category(&self) -> &str {
        SHEETS_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Drawing sheets"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let set = SheetSet::from_value(&element.data)?;
        let (mut primitives, mut x) = (Vec::new(), 0.0);
        let mut height: f64 = 1.0;
        for source in &set.sources {
            let spec = match source {
                SheetSource::Single(s) => s,
                SheetSource::PlanSeries(p) => &p.template,
                SheetSource::TableSeries(t) => &t.template,
                SheetSource::SectionSeries(t) => &t.template,
            };
            let (w, h) = spec.page.dimensions_mm();
            let q = [Pt2::new(x, 0.0), Pt2::new(x + w, 0.0), Pt2::new(x + w, h), Pt2::new(x, h)];
            for i in 0..4 {
                primitives.push(Primitive::Line { from: q[i], to: q[(i + 1) % 4], width: 0.0, colour: Some(Colour::Grey) });
            }
            x += w + 20.0;
            height = height.max(h);
        }
        let notes = set.validate().into_iter().map(|m| Note { severity: Severity::Warning, code: "sheets.definition".to_string(), message: m }).collect();
        let mut derived = BTreeMap::new();
        derived.insert("Sources".to_string(), set.sources.len().to_string());
        derived.insert("First number".to_string(), set.numbering.number(0));
        Ok(Generated { drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new((x - 20.0).max(1.0), height) } }, notes, derived, solids: Vec::new() })
    }
}

/// Adds the sheet set to the document as a `sheets` element and returns its id.
pub fn add_sheet_set(doc: &mut Document, name: &str, set: &SheetSet) -> ElementId {
    doc.ensure_layer(SHEETS_LAYER);
    doc.add(SHEETS_CATEGORY, name, Pt::new(0.0, 0.0), 1.0, 1.0, SHEETS_LAYER, set.to_value())
}

/// Replaces the set stored in an existing `sheets` element.
pub fn store_sheet_set(doc: &mut Document, id: ElementId, set: &SheetSet) -> Result<(), String> {
    let e = doc.find_mut(id).ok_or_else(|| format!("no element {}", id.0))?;
    if e.category != SHEETS_CATEGORY {
        return Err(format!("element {} is a '{}', not a sheet set", id.0, e.category));
    }
    e.data = set.to_value();
    Ok(())
}

/// Reads the set of a `sheets` element.
pub fn load_sheet_set(doc: &Document, id: ElementId) -> Result<SheetSet, String> {
    let e = doc.find(id).ok_or_else(|| format!("no element {}", id.0))?;
    if e.category != SHEETS_CATEGORY {
        return Err(format!("element {} is a '{}', not a sheet set", id.0, e.category));
    }
    SheetSet::from_value(&e.data)
}

/// The sheet-set elements of a document: id and name.
pub fn sheet_sets(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == SHEETS_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

/// A table is a view; this builds a one-table sheet for convenience.
pub fn table_sheet(title: &str, page: Page, table: Table) -> SheetSpec {
    SheetSpec::new(title, page, vec![ViewDef::full(ViewKind::Table(table))])
}
