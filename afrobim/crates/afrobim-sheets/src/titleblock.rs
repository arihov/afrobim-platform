// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The title block: a data-driven block of rows and cells, a revision table, a logo placeholder,
//! a north arrow and a scale bar.
//!
//! The layout is data. A [`TitleBlockLayout`] is a stack of [`Block`]s (rows of cells with sizes
//! in mm, a revision table, a logo rectangle, a spacer) and a [`TitlePlacement`]. The default is
//! one neutral layout; it reproduces no organisation's title block and a user defines their own
//! by building another `TitleBlockLayout` (it serialises, so it can be saved and loaded).

use afrobim_draw::Pt2;
use serde::{Deserialize, Serialize};

use crate::geom::Rect;
use crate::item::{layers, text as mk_text, ComposedSheet};
use crate::paper::Scale;
use crate::project::{ProjectInfo, UserField};
use crate::style::Styles;
use crate::text::{fit, wrap, Align};

/// What a title block cell can show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldKey {
    Project,
    Client,
    ContractNumber,
    /// The consulting engineer.
    Consultant,
    /// The contractor.
    Contractor,
    DrawingTitle,
    DrawingNumber,
    Revision,
    Date,
    Scale,
    DrawnBy,
    CheckedBy,
    ApprovedBy,
    /// "n of m": this sheet's place in the set.
    SheetOf,
    /// A user field by caption, taken from the sheet's own fields first and then the project's.
    User(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellContent {
    /// A field with a caption above it.
    Field { key: FieldKey, label: String },
    /// Fixed text with a caption.
    Text { label: String, text: String },
    Empty,
}

/// One cell of a row. `width_mm` of 0 shares what the other cells leave equally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub width_mm: f64,
    pub content: CellContent,
    /// Capital height of the value; `None` takes the style's normal text height.
    pub value_height_mm: Option<f64>,
    pub bold: bool,
}

impl Cell {
    pub fn field(width_mm: f64, key: FieldKey, label: &str) -> Cell {
        Cell { width_mm, content: CellContent::Field { key, label: label.to_string() }, value_height_mm: None, bold: false }
    }
    pub fn with_height(mut self, mm: f64) -> Cell {
        self.value_height_mm = Some(mm);
        self
    }
    pub fn bold(mut self) -> Cell {
        self.bold = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub height_mm: f64,
    pub cells: Vec<Cell>,
}

/// Which revision field a revision table column prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevField {
    Rev,
    Date,
    Description,
    By,
    Checked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevColumn {
    pub header: String,
    pub width_mm: f64,
    pub field: RevField,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Rows(Vec<Row>),
    /// A revision table with room for `rows` revisions; when the project has more, the latest
    /// ones are shown.
    RevisionTable { rows: usize, header_height_mm: f64, row_height_mm: f64, columns: Vec<RevColumn> },
    /// A rectangle for a logo, with the project's `logo_text` centred in it.
    Logo { height_mm: f64 },
    /// A framed space for the north point, drawn pointing north on the sheet (or as far round as
    /// the first viewport's twist turns it).
    NorthPoint { height_mm: f64 },
    Spacer { height_mm: f64 },
}

impl Block {
    pub fn height_mm(&self) -> f64 {
        match self {
            Block::Rows(rows) => rows.iter().map(|r| r.height_mm).sum(),
            Block::RevisionTable { rows, header_height_mm, row_height_mm, .. } => header_height_mm + *rows as f64 * row_height_mm,
            Block::Logo { height_mm } | Block::NorthPoint { height_mm } | Block::Spacer { height_mm } => *height_mm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TitlePlacement {
    /// A column the full height of the frame on the right; the views get what is left of the width.
    #[default]
    RightColumn,
    /// A block in the lower right corner; the views get the full width above it.
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TitleBlockLayout {
    pub placement: TitlePlacement,
    pub width_mm: f64,
    /// From the top of the block down.
    pub blocks: Vec<Block>,
}

impl TitleBlockLayout {
    pub fn height_mm(&self) -> f64 {
        self.blocks.iter().map(Block::height_mm).sum()
    }

    /// The rectangle the block fills inside `frame`, bottom right.
    pub fn rect_in(&self, frame: &Rect) -> Rect {
        let h = self.height_mm().min(frame.height());
        Rect::new(frame.x1 - self.width_mm.min(frame.width()), frame.y0, frame.x1, frame.y0 + h)
    }

    /// What is left of the frame for views.
    pub fn views_area(&self, frame: &Rect) -> Rect {
        let b = self.rect_in(frame);
        match self.placement {
            TitlePlacement::RightColumn => Rect::new(frame.x0, frame.y0, b.x0, frame.y1),
            TitlePlacement::BottomRight => Rect::new(frame.x0, b.y1.max(frame.y0), frame.x1, frame.y1),
        }
    }
}

impl Default for TitleBlockLayout {
    /// The neutral default layout, 70 mm wide: revision table, logo placeholder, then project,
    /// client, contract number, drawing title, drawing number, scale / revision / date, drawn /
    /// checked / approved and sheet n of m. Sizes are defaults the user can change freely.
    fn default() -> Self {
        let w = 70.0;
        let third = w / 3.0;
        TitleBlockLayout {
            placement: TitlePlacement::RightColumn,
            width_mm: w,
            blocks: vec![
                Block::RevisionTable {
                    rows: 5,
                    header_height_mm: 5.0,
                    row_height_mm: 5.0,
                    columns: vec![
                        RevColumn { header: "Rev".into(), width_mm: 9.0, field: RevField::Rev },
                        RevColumn { header: "Description".into(), width_mm: 31.0, field: RevField::Description },
                        RevColumn { header: "Date".into(), width_mm: 15.0, field: RevField::Date },
                        RevColumn { header: "By".into(), width_mm: 7.5, field: RevField::By },
                        RevColumn { header: "Chk".into(), width_mm: 7.5, field: RevField::Checked },
                    ],
                },
                Block::Logo { height_mm: 18.0 },
                Block::Rows(vec![
                    Row { height_mm: 16.0, cells: vec![Cell::field(w, FieldKey::Project, "Project")] },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w, FieldKey::Client, "Client")] },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w, FieldKey::ContractNumber, "Contract number")] },
                    Row { height_mm: 22.0, cells: vec![Cell::field(w, FieldKey::DrawingTitle, "Drawing title").with_height(3.5).bold()] },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w, FieldKey::DrawingNumber, "Drawing number").bold()] },
                    Row {
                        height_mm: 10.0,
                        cells: vec![
                            Cell::field(third, FieldKey::Scale, "Scale"),
                            Cell::field(third, FieldKey::Revision, "Revision"),
                            Cell::field(third, FieldKey::Date, "Date"),
                        ],
                    },
                    Row {
                        height_mm: 10.0,
                        cells: vec![
                            Cell::field(third, FieldKey::DrawnBy, "Drawn"),
                            Cell::field(third, FieldKey::CheckedBy, "Checked"),
                            Cell::field(third, FieldKey::ApprovedBy, "Approved"),
                        ],
                    },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w, FieldKey::SheetOf, "Sheet")] },
                ]),
            ],
        }
    }
}

/// What Ministry of Works and Transport (Uganda) is called in its title block.
pub const MOWT_NAME: &str = "Ministry of Works and Transport";

impl TitleBlockLayout {
    /// The title block of Ministry of Works and Transport (Uganda) drawings, 80 mm wide down the
    /// right of the frame: revision table, north point space, project, contract number, the
    /// client (fixed: MoWT), consultant, contractor, drawing title, drawing number with
    /// "sheet n of m", scale and date, and drawn / checked / approved. Sizes are working
    /// defaults, adjustable like every other `TitleBlockLayout`; the names and the order follow
    /// what a MoWT drawing carries, not a copy of any one form.
    pub fn mowt() -> Self {
        let w = 80.0;
        let third = w / 3.0;
        let fixed_client = Cell { width_mm: w, content: CellContent::Text { label: "Client".into(), text: MOWT_NAME.into() }, value_height_mm: None, bold: true };
        TitleBlockLayout {
            placement: TitlePlacement::RightColumn,
            width_mm: w,
            blocks: vec![
                Block::RevisionTable {
                    rows: 5,
                    header_height_mm: 5.0,
                    row_height_mm: 5.0,
                    columns: vec![
                        RevColumn { header: "Rev".into(), width_mm: 9.0, field: RevField::Rev },
                        RevColumn { header: "Description".into(), width_mm: 37.0, field: RevField::Description },
                        RevColumn { header: "Date".into(), width_mm: 17.0, field: RevField::Date },
                        RevColumn { header: "By".into(), width_mm: 8.5, field: RevField::By },
                        RevColumn { header: "Chk".into(), width_mm: 8.5, field: RevField::Checked },
                    ],
                },
                Block::NorthPoint { height_mm: 30.0 },
                Block::Rows(vec![
                    Row { height_mm: 16.0, cells: vec![Cell::field(w, FieldKey::Project, "Project")] },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w, FieldKey::ContractNumber, "Contract no.")] },
                    Row { height_mm: 10.0, cells: vec![fixed_client] },
                    Row { height_mm: 12.0, cells: vec![Cell::field(w, FieldKey::Consultant, "Consultant")] },
                    Row { height_mm: 12.0, cells: vec![Cell::field(w, FieldKey::Contractor, "Contractor")] },
                    Row { height_mm: 22.0, cells: vec![Cell::field(w, FieldKey::DrawingTitle, "Drawing title").with_height(3.5).bold()] },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w * 0.6, FieldKey::DrawingNumber, "Drawing no.").bold(), Cell::field(w * 0.4, FieldKey::SheetOf, "Sheet no. of total")] },
                    Row { height_mm: 10.0, cells: vec![Cell::field(w * 0.5, FieldKey::Scale, "Scale"), Cell::field(w * 0.5, FieldKey::Date, "Date")] },
                    Row {
                        height_mm: 10.0,
                        cells: vec![
                            Cell::field(third, FieldKey::DrawnBy, "Drawn"),
                            Cell::field(third, FieldKey::CheckedBy, "Checked"),
                            Cell::field(third, FieldKey::ApprovedBy, "Approved"),
                        ],
                    },
                ]),
            ],
        }
    }

    /// The layout of a style (`afrobim_kernel::TitleStyle`).
    pub fn for_style(style: afrobim_kernel::TitleStyle) -> Self {
        match style {
            afrobim_kernel::TitleStyle::Afrobim => TitleBlockLayout::default(),
            afrobim_kernel::TitleStyle::Mowt => TitleBlockLayout::mowt(),
        }
    }
}

/// Values a sheet sets for itself: what the project information does not say, or says otherwise.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SheetFields {
    /// Overrides the project's current revision.
    pub revision: Option<String>,
    pub date: Option<String>,
    pub drawn_by: Option<String>,
    pub checked_by: Option<String>,
    pub approved_by: Option<String>,
    /// Overrides the printed scale text (for example "AS SHOWN" or "NTS").
    pub scale_text: Option<String>,
    pub user_fields: Vec<UserField>,
    /// The direction north points on the sheet, degrees anticlockwise from +X, for the north
    /// point block. `None` is straight up the sheet (90).
    pub north_deg: Option<f64>,
}

/// Everything a title block needs to resolve its fields.
pub struct TitleValues<'a> {
    pub info: &'a ProjectInfo,
    pub fields: &'a SheetFields,
    pub number: &'a str,
    pub title: &'a str,
    /// The scale of the first view that has one, as text.
    pub scale_text: &'a str,
    /// 1-based position and the number of sheets.
    pub sheet: (usize, usize),
}

impl TitleValues<'_> {
    pub fn get(&self, key: &FieldKey) -> String {
        let pick = |own: &Option<String>, project: &str| own.clone().unwrap_or_else(|| project.to_string());
        match key {
            FieldKey::Project => self.info.project.clone(),
            FieldKey::Client => self.info.client.clone(),
            FieldKey::ContractNumber => self.info.contract_number.clone(),
            FieldKey::Consultant => self.info.consultant.clone(),
            FieldKey::Contractor => self.info.contractor.clone(),
            FieldKey::DrawingTitle => self.title.to_string(),
            FieldKey::DrawingNumber => self.number.to_string(),
            FieldKey::Revision => pick(&self.fields.revision, &self.info.current_revision()),
            FieldKey::Date => pick(&self.fields.date, &self.info.date),
            FieldKey::Scale => self.fields.scale_text.clone().unwrap_or_else(|| self.scale_text.to_string()),
            FieldKey::DrawnBy => pick(&self.fields.drawn_by, &self.info.drawn_by),
            FieldKey::CheckedBy => pick(&self.fields.checked_by, &self.info.checked_by),
            FieldKey::ApprovedBy => pick(&self.fields.approved_by, &self.info.approved_by),
            FieldKey::SheetOf => format!("{} of {}", self.sheet.0, self.sheet.1),
            FieldKey::User(label) => self
                .fields
                .user_fields
                .iter()
                .chain(&self.info.user_fields)
                .find(|f| &f.label == label)
                .map(|f| f.value.clone())
                .unwrap_or_default(),
        }
    }
}

/// Draws the title block into `rect` (see [`TitleBlockLayout::rect_in`]); blocks stack down from
/// the top of `rect`. Warns when the stack is taller than the rectangle.
pub fn draw_title_block(sheet: &mut ComposedSheet, layout: &TitleBlockLayout, rect: &Rect, values: &TitleValues, styles: &Styles) {
    let pen = styles.pens.pen(layers::TITLE_BLOCK);
    let total = layout.height_mm();
    if total > rect.height() + 1e-6 {
        sheet.warn(format!("The title block is {total:.0} mm tall but the frame leaves {:.0} mm: it is cut off at the top.", rect.height()));
    }
    let mut y = rect.y1.min(rect.y0 + total);
    for block in &layout.blocks {
        let h = block.height_mm();
        let area = Rect::new(rect.x0, y - h, rect.x1, y);
        match block {
            Block::Rows(rows) => {
                let mut ry = area.y1;
                for row in rows {
                    draw_row(sheet, row, &Rect::new(area.x0, ry - row.height_mm, area.x1, ry), values, styles);
                    ry -= row.height_mm;
                }
            }
            Block::RevisionTable { rows, header_height_mm, row_height_mm, columns } => {
                let revs = &values.info.revisions;
                let shown: Vec<_> = revs.iter().skip(revs.len().saturating_sub(*rows)).collect();
                let mut x = area.x0;
                for col in columns {
                    let cell = Rect::new(x, area.y1 - header_height_mm, x + col.width_mm, area.y1);
                    sheet.rect(layers::TITLE_BLOCK, &cell, pen.weight_mm, pen.rgb);
                    sheet.text(mk_text(layers::TEXT, Pt2::new(x + 1.0, cell.y0 + (header_height_mm - styles.text.small) / 2.0), styles.text.small, col.header.clone(), Align::Left, 0.0, true, pen.rgb));
                    for r in 0..*rows {
                        let top = area.y1 - header_height_mm - r as f64 * row_height_mm;
                        let c = Rect::new(x, top - row_height_mm, x + col.width_mm, top);
                        sheet.rect(layers::TITLE_BLOCK, &c, pen.weight_mm, pen.rgb);
                        if let Some(rev) = shown.get(r) {
                            let text = match col.field {
                                RevField::Rev => &rev.rev,
                                RevField::Date => &rev.date,
                                RevField::Description => &rev.description,
                                RevField::By => &rev.by,
                                RevField::Checked => &rev.checked,
                            };
                            let hh = styles.text.small;
                            let t = fit(text, col.width_mm - 1.6, hh, false);
                            sheet.text(mk_text(layers::TEXT, Pt2::new(x + 0.8, c.y0 + (row_height_mm - hh) / 2.0), hh, t, Align::Left, 0.0, false, pen.rgb));
                        }
                    }
                    x += col.width_mm;
                }
            }
            Block::Logo { .. } => {
                sheet.rect(layers::TITLE_BLOCK, &area, pen.weight_mm, pen.rgb);
                let c = area.centre();
                sheet.text(mk_text(layers::TEXT, Pt2::new(c.x, c.y - styles.text.normal / 2.0), styles.text.normal, values.info.logo_text.clone(), Align::Centre, 0.0, false, pen.rgb));
            }
            Block::NorthPoint { .. } => {
                sheet.rect(layers::TITLE_BLOCK, &area, pen.weight_mm, pen.rgb);
                let size = (area.height() * 0.5).min(area.width() * 0.5).max(4.0);
                north_arrow(sheet, area.centre(), size, values.fields.north_deg.unwrap_or(90.0), styles);
            }
            Block::Spacer { .. } => {}
        }
        y -= h;
    }
}

fn draw_row(sheet: &mut ComposedSheet, row: &Row, area: &Rect, values: &TitleValues, styles: &Styles) {
    let pen = styles.pens.pen(layers::TITLE_BLOCK);
    let fixed: f64 = row.cells.iter().map(|c| c.width_mm).sum();
    let flexible = row.cells.iter().filter(|c| c.width_mm <= 0.0).count();
    let share = if flexible > 0 { ((area.width() - fixed) / flexible as f64).max(0.0) } else { 0.0 };
    let mut x = area.x0;
    for cell in &row.cells {
        let w = if cell.width_mm > 0.0 { cell.width_mm } else { share };
        let r = Rect::new(x, area.y0, (x + w).min(area.x1), area.y1);
        x += w;
        sheet.rect(layers::TITLE_BLOCK, &r, pen.weight_mm, pen.rgb);
        let (label, value) = match &cell.content {
            CellContent::Field { key, label } => (label.clone(), values.get(key)),
            CellContent::Text { label, text } => (label.clone(), text.clone()),
            CellContent::Empty => continue,
        };
        let small = styles.text.small;
        let caption_base = r.y1 - 0.8 - small;
        sheet.text(mk_text(layers::TEXT, Pt2::new(r.x0 + 1.0, caption_base), small, label, Align::Left, 0.0, false, [90, 90, 90]));
        let vh = cell.value_height_mm.unwrap_or(styles.text.normal);
        let inner_w = r.width() - 2.4;
        let line_pitch = vh * 1.45;
        let first_base = caption_base - 1.2 - vh;
        let max_lines = (((first_base - r.y0 - 0.8) / line_pitch).floor() as i64 + 1).max(1) as usize;
        let mut lines = wrap(&value, inner_w, vh, cell.bold);
        if lines.len() > max_lines {
            lines.truncate(max_lines);
            if let Some(last) = lines.last_mut() {
                let joined = format!("{last} ...");
                *last = fit(&joined, inner_w, vh, cell.bold);
            }
        }
        for (i, line) in lines.into_iter().enumerate() {
            sheet.text(mk_text(layers::TEXT, Pt2::new(r.x0 + 1.2, first_base - i as f64 * line_pitch), vh, line, Align::Left, 0.0, cell.bold, pen.rgb));
        }
    }
}

/// A north arrow centred on `centre`, `size_mm` tall. `north_deg` is the direction north points
/// on the sheet, anticlockwise from +X (90 for north up the sheet).
pub fn north_arrow(sheet: &mut ComposedSheet, centre: Pt2, size_mm: f64, north_deg: f64, styles: &Styles) {
    let pen = styles.pens.pen(layers::PLAN_FURNITURE);
    let (s, c) = north_deg.to_radians().sin_cos();
    // Local frame: u along north, v to the left of it.
    let at = |u: f64, v: f64| Pt2::new(centre.x + u * c - v * s, centre.y + u * s + v * c);
    let (tip, tail) = (size_mm * 0.5, -size_mm * 0.5);
    let half = size_mm * 0.14;
    let notch = -size_mm * 0.22;
    let outline = vec![at(tip, 0.0), at(tail, -half), at(notch, 0.0), at(tail, half)];
    sheet.fill(layers::PLAN_FURNITURE, vec![vec![at(tip, 0.0), at(tail, -half), at(notch, 0.0)]], pen.rgb);
    sheet.stroke(layers::PLAN_FURNITURE, outline, true, pen.weight_mm, pen.rgb);
    let label = at(tip + styles.text.normal * 0.9, 0.0);
    // Keep the N upright and centred on the point above the tip.
    sheet.text(mk_text(layers::PLAN_FURNITURE, Pt2::new(label.x, label.y - styles.text.normal / 2.0), styles.text.normal, "N", Align::Centre, 0.0, true, pen.rgb));
}

/// The largest of 1, 2, 5 times a power of ten that is at most `x`.
pub fn round_down_1_2_5(x: f64) -> f64 {
    if !(x.is_finite() && x > 0.0) {
        return 1.0;
    }
    let p = 10f64.powf(x.log10().floor());
    for m in [5.0, 2.0, 1.0] {
        if m * p <= x * (1.0 + 1e-12) {
            return m * p;
        }
    }
    p
}

/// A scale bar for `scale` whose left end is at `at`, no longer than `max_len_mm`. The length is
/// a round number of metres (1, 2 or 5 times a power of ten) in four alternating divisions.
/// Returns the bar's length in metres and in mm.
pub fn scale_bar(sheet: &mut ComposedSheet, at: Pt2, scale: Scale, max_len_mm: f64, styles: &Styles) -> (f64, f64) {
    let pen = styles.pens.pen(layers::PLAN_FURNITURE);
    let metres = round_down_1_2_5(scale.ground_m(max_len_mm));
    let len = metres * scale.paper_mm_per_m();
    let (h, n) = (1.4, 4usize);
    let seg = len / n as f64;
    for i in 0..n {
        let r = Rect::new(at.x + i as f64 * seg, at.y, at.x + (i + 1) as f64 * seg, at.y + h);
        if i % 2 == 0 {
            sheet.fill_rect(layers::PLAN_FURNITURE, &r, pen.rgb);
        }
        sheet.rect(layers::PLAN_FURNITURE, &r, pen.weight_mm, pen.rgb);
    }
    let label_y = at.y + h + 0.8;
    for (i, frac) in [(0usize, 0.0), (2, 0.5), (4, 1.0)] {
        let value = metres * frac;
        let t = if i == 4 { format!("{} m", crate::paper::trim_number(value, 2)) } else { crate::paper::trim_number(value, 2) };
        sheet.text(mk_text(layers::PLAN_FURNITURE, Pt2::new(at.x + len * frac, label_y), styles.text.small, t, Align::Centre, 0.0, false, pen.rgb));
    }
    (metres, len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Revision;

    fn values<'a>(info: &'a ProjectInfo, fields: &'a SheetFields) -> TitleValues<'a> {
        TitleValues { info, fields, number: "R-002", title: "Plan sta 0+000 to 0+400", scale_text: "1:500", sheet: (2, 7) }
    }

    #[test]
    fn fields_resolve_from_the_project_and_the_sheet() {
        let info = ProjectInfo {
            project: "Rural road".into(),
            client: "Client".into(),
            drawn_by: "AB".into(),
            date: "2026-03-01".into(),
            revisions: vec![Revision { rev: "A".into(), ..Revision::default() }, Revision { rev: "B".into(), ..Revision::default() }],
            user_fields: vec![UserField { label: "Phase".into(), value: "Design".into() }],
            ..ProjectInfo::default()
        };
        let fields = SheetFields { drawn_by: Some("ZZ".into()), user_fields: vec![UserField { label: "Zone".into(), value: "North".into() }], ..SheetFields::default() };
        let v = values(&info, &fields);
        assert_eq!(v.get(&FieldKey::Project), "Rural road");
        assert_eq!(v.get(&FieldKey::DrawingNumber), "R-002");
        assert_eq!(v.get(&FieldKey::Revision), "B"); // the last revision row
        assert_eq!(v.get(&FieldKey::DrawnBy), "ZZ"); // the sheet's own value wins
        assert_eq!(v.get(&FieldKey::CheckedBy), "");
        assert_eq!(v.get(&FieldKey::SheetOf), "2 of 7");
        assert_eq!(v.get(&FieldKey::Scale), "1:500");
        assert_eq!(v.get(&FieldKey::User("Phase".into())), "Design");
        assert_eq!(v.get(&FieldKey::User("Zone".into())), "North");
        assert_eq!(v.get(&FieldKey::User("Nope".into())), "");
    }

    #[test]
    fn the_default_layout_adds_up_and_leaves_the_rest_for_views() {
        let l = TitleBlockLayout::default();
        // Revision table 5 + 5 x 5 = 30, logo 18, rows 16 + 10 + 10 + 22 + 10 + 10 + 10 + 10 = 98.
        assert_eq!(l.height_mm(), 30.0 + 18.0 + 98.0);
        for b in &l.blocks {
            match b {
                Block::Rows(rows) => {
                    for r in rows {
                        assert!((r.cells.iter().map(|c| c.width_mm).sum::<f64>() - l.width_mm).abs() < 1e-9);
                    }
                }
                Block::RevisionTable { columns, .. } => assert!((columns.iter().map(|c| c.width_mm).sum::<f64>() - l.width_mm).abs() < 1e-9),
                _ => {}
            }
        }
        let frame = Rect::new(10.0, 10.0, 410.0, 287.0);
        let v = l.views_area(&frame);
        assert_eq!((v.x0, v.x1, v.y0, v.y1), (10.0, 340.0, 10.0, 287.0));
        let mut bottom = l.clone();
        bottom.placement = TitlePlacement::BottomRight;
        let v = bottom.views_area(&frame);
        assert_eq!((v.x1, v.y0), (410.0, 10.0 + 146.0));
    }

    #[test]
    fn the_title_block_prints_the_field_values_and_sheet_n_of_m() {
        let info = ProjectInfo { project: "Rural road".into(), revisions: vec![Revision { rev: "A".into(), description: "First issue".into(), ..Revision::default() }], ..ProjectInfo::default() };
        let fields = SheetFields::default();
        let l = TitleBlockLayout::default();
        let mut sheet = ComposedSheet::new("R-002", "t", 420.0, 297.0);
        let frame = Rect::new(10.0, 10.0, 410.0, 287.0);
        draw_title_block(&mut sheet, &l, &l.rect_in(&frame), &values(&info, &fields), &Styles::default());
        let texts: Vec<&str> = sheet.texts().map(|t| t.text.as_str()).collect();
        for want in ["Rural road", "R-002", "2 of 7", "1:500", "A", "First issue", "Project", "Drawing number", "Scale", "Sheet"] {
            assert!(texts.contains(&want), "missing {want}: {texts:?}");
        }
        assert!(sheet.warnings.is_empty());
        // Everything of the block lies inside its rectangle.
        let r = l.rect_in(&frame);
        for t in sheet.texts() {
            assert!(r.contains(t.at), "{t:?}");
        }
    }

    #[test]
    fn a_long_title_wraps_in_its_cell_and_a_too_tall_block_warns() {
        let info = ProjectInfo::default();
        let fields = SheetFields::default();
        let l = TitleBlockLayout::default();
        let mut v = values(&info, &fields);
        v.title = "A very long drawing title that has to wrap over several lines inside the title block cell";
        let mut sheet = ComposedSheet::new("1", "t", 420.0, 297.0);
        let frame = Rect::new(10.0, 10.0, 410.0, 287.0);
        draw_title_block(&mut sheet, &l, &l.rect_in(&frame), &v, &Styles::default());
        assert!(sheet.texts().filter(|t| t.layer == layers::TEXT && t.bold && t.height_mm == 3.5).count() >= 2);
        let small = Rect::new(10.0, 10.0, 410.0, 100.0);
        let mut s2 = ComposedSheet::new("1", "t", 420.0, 297.0);
        draw_title_block(&mut s2, &l, &l.rect_in(&small), &v, &Styles::default());
        assert!(!s2.warnings.is_empty());
    }

    #[test]
    fn the_scale_bar_is_a_round_length_that_fits() {
        // At 1:500 one metre is 2 mm; 40 mm is 20 m, already round, so the bar is 20 m = 40 mm.
        let mut sheet = ComposedSheet::new("1", "t", 100.0, 100.0);
        let (m, mm) = scale_bar(&mut sheet, Pt2::new(10.0, 10.0), Scale(500.0), 40.0, &Styles::default());
        assert_eq!((m, mm), (20.0, 40.0));
        // 1:1000, 45 mm is 45 m, rounded down to 20 m = 20 mm.
        let (m, mm) = scale_bar(&mut sheet, Pt2::new(10.0, 10.0), Scale(1000.0), 45.0, &Styles::default());
        assert_eq!((m, mm), (20.0, 20.0));
        assert!(sheet.texts().any(|t| t.text == "20 m"));
        assert_eq!(round_down_1_2_5(7.0), 5.0);
        assert_eq!(round_down_1_2_5(0.3), 0.2);
    }

    #[test]
    fn the_north_arrow_points_where_it_is_told() {
        let mut sheet = ComposedSheet::new("1", "t", 100.0, 100.0);
        north_arrow(&mut sheet, Pt2::new(50.0, 50.0), 10.0, 90.0, &Styles::default());
        // Pointing up the sheet: the tip is 5 mm above the centre.
        let top = sheet.strokes().find_map(|i| if let crate::item::Item::Stroke { points, .. } = i { Some(points[0]) } else { None }).unwrap();
        assert!(top.distance_to(Pt2::new(50.0, 55.0)) < 1e-9);
        assert!(sheet.texts().any(|t| t.text == "N"));
    }

    #[test]
    fn the_mowt_layout_adds_up_has_the_required_fields_and_no_em_dash() {
        let l = TitleBlockLayout::mowt();
        assert_eq!(l.width_mm, 80.0);
        // Revision table 30, north point 30, rows 16 + 10 + 10 + 12 + 12 + 22 + 10 + 10 + 10 = 112.
        assert_eq!(l.height_mm(), 172.0);
        for b in &l.blocks {
            match b {
                Block::Rows(rows) => {
                    for r in rows {
                        assert!((r.cells.iter().map(|c| c.width_mm).sum::<f64>() - l.width_mm).abs() < 1e-9);
                    }
                }
                Block::RevisionTable { columns, .. } => assert!((columns.iter().map(|c| c.width_mm).sum::<f64>() - l.width_mm).abs() < 1e-9),
                _ => {}
            }
        }
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains('\u{2014}') && !json.contains('\u{2013}'));
        for key in [FieldKey::Project, FieldKey::ContractNumber, FieldKey::Consultant, FieldKey::Contractor, FieldKey::DrawingTitle, FieldKey::DrawingNumber, FieldKey::SheetOf, FieldKey::Scale, FieldKey::Date, FieldKey::DrawnBy, FieldKey::CheckedBy, FieldKey::ApprovedBy] {
            assert!(
                l.blocks.iter().any(|b| matches!(b, Block::Rows(rows) if rows.iter().any(|r| r.cells.iter().any(|c| matches!(&c.content, CellContent::Field { key: k, .. } if *k == key))))),
                "{key:?} is in the MoWT block"
            );
        }
        assert!(l.blocks.iter().any(|b| matches!(b, Block::RevisionTable { .. })) && l.blocks.iter().any(|b| matches!(b, Block::NorthPoint { .. })));
        assert!(json.contains(MOWT_NAME), "the client is MoWT");
        let back: TitleBlockLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, l);
        assert_eq!(TitleBlockLayout::for_style(afrobim_kernel::TitleStyle::Mowt), l);
        assert_eq!(TitleBlockLayout::for_style(afrobim_kernel::TitleStyle::Afrobim), TitleBlockLayout::default());
    }

    #[test]
    fn consultant_and_contractor_resolve_and_old_project_information_still_reads() {
        let info = ProjectInfo { consultant: "C1".into(), contractor: "C2".into(), ..ProjectInfo::default() };
        let f = SheetFields::default();
        let v = values(&info, &f);
        assert_eq!((v.get(&FieldKey::Consultant).as_str(), v.get(&FieldKey::Contractor).as_str()), ("C1", "C2"));
        let old = serde_json::json!({"project":"P","client":"C","contract_number":"1","drawn_by":"","checked_by":"","approved_by":"","date":"","revisions":[],"user_fields":[],"logo_text":""});
        let back = ProjectInfo::from_value(&old).unwrap();
        assert_eq!((back.consultant.as_str(), back.contractor.as_str()), ("", ""));
    }
}
