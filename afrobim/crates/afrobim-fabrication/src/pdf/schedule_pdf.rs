// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The printable sign-schedule report. Port of C# `SchedulePdfWriter`.
//!
//! A4 landscape, one row per sign (code, type, size, x-height, face area, and quantity, total
//! area and location when the schedule has them), with a totals row. Face area is the gross
//! panel rectangle, as in the BoQ CSV.

use crate::composer::format_mm;
use crate::num::{fixed, utf16_len, utf16_prefix};
use crate::quantities::SignQuantity;
use crate::sheet::{SheetText, SheetTextAlign};

use super::document::Document;
use super::gfx::{Pen, BLACK};
use super::{Canvas, PdfFonts, PdfStamp};

/// One sign in the schedule report: its measured face, type label, and the optional quantity
/// and location columns of the schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRow {
    pub q: SignQuantity,
    /// The type label, see [`crate::composer::type_name`].
    pub type_name: String,
    pub quantity: Option<u32>,
    pub location: Option<String>,
}

impl ScheduleRow {
    /// Signs of this code: the quantity, or one when the schedule gives none.
    pub fn count(&self) -> u32 {
        self.quantity.unwrap_or(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduleTotals {
    pub rows: usize,
    pub count: u32,
    pub area_m2: f64,
}

/// Header fields printed above the schedule table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleInfo {
    pub project: String,
    pub date: String,
    pub standard: String,
}

pub const ROWS_PER_PAGE: usize = 26;
const PAGE_W: f64 = 297.0;
const PAGE_H: f64 = 210.0;
const LEFT: f64 = 12.0;
const RIGHT: f64 = 285.0;
const TOP_ROW: f64 = 172.0;
const ROW_H: f64 = 5.6;

pub fn schedule_totals(rows: &[ScheduleRow]) -> ScheduleTotals {
    ScheduleTotals {
        rows: rows.len(),
        count: rows.iter().map(ScheduleRow::count).sum(),
        area_m2: rows.iter().map(|r| r.q.area_m2() * f64::from(r.count())).sum(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Number,
    Code,
    Type,
    Size,
    XHeight,
    Area,
    Quantity,
    TotalArea,
    Location,
}

impl Column {
    fn head(self) -> &'static str {
        match self {
            Column::Number => "No.",
            Column::Code => "Code",
            Column::Type => "Type",
            Column::Size => "Size W x H (mm)",
            Column::XHeight => "x-height (mm)",
            Column::Area => "Area (m2)",
            Column::Quantity => "Qty",
            Column::TotalArea => "Total area (m2)",
            Column::Location => "Location",
        }
    }

    /// Numeric columns are set flush right.
    fn numeric(self) -> bool {
        !matches!(self, Column::Code | Column::Type | Column::Location)
    }

    fn cell(self, index: usize, r: &ScheduleRow) -> String {
        match self {
            Column::Number => (index + 1).to_string(),
            Column::Code => r.q.code.clone(),
            Column::Type => r.type_name.clone(),
            Column::Size => format!("{} x {}", format_mm(r.q.width_mm), format_mm(r.q.height_mm)),
            Column::XHeight => format_mm(r.q.x_height_mm),
            Column::Area => f3(r.q.area_m2()),
            Column::Quantity => r.count().to_string(),
            Column::TotalArea => f3(r.q.area_m2() * f64::from(r.count())),
            Column::Location => r.location.clone().unwrap_or_default(),
        }
    }
}

/// The schedule as PDF bytes, stamped with the current time.
pub fn write_schedule_pdf(rows: &[ScheduleRow], info: &ScheduleInfo, fonts: &PdfFonts) -> Vec<u8> {
    write_schedule_pdf_stamped(rows, info, fonts, &PdfStamp::now())
}

/// The schedule as PDF bytes with a given stamp; the same stamp gives the same bytes.
pub fn write_schedule_pdf_stamped(rows: &[ScheduleRow], info: &ScheduleInfo, fonts: &PdfFonts, stamp: &PdfStamp) -> Vec<u8> {
    let has_qty = rows.iter().any(|r| r.quantity.is_some());
    let has_loc = rows.iter().any(|r| r.location.as_deref().is_some_and(|l| !l.trim().is_empty()));

    // Column set: (column, width); a zero width takes what is left.
    let mut cols: Vec<(Column, f64)> = vec![
        (Column::Number, 10.0),
        (Column::Code, 30.0),
        (Column::Type, if has_loc { 52.0 } else { 70.0 }),
        (Column::Size, 40.0),
        (Column::XHeight, 28.0),
        (Column::Area, 24.0),
    ];
    if has_qty {
        cols.push((Column::Quantity, 14.0));
        cols.push((Column::TotalArea, 32.0));
    }
    if has_loc {
        cols.push((Column::Location, 0.0));
    }

    // Lay columns out left to right; the last column takes what is left.
    let mut xs: Vec<(f64, f64)> = Vec::new();
    let mut x = LEFT;
    for (c, (_, width)) in cols.iter().enumerate() {
        let w = if c == cols.len() - 1 && *width == 0.0 { RIGHT - x } else { *width };
        xs.push((x, x + w));
        x += w;
    }
    let table_right = xs[xs.len() - 1].1;
    let column_x = |column: Column| cols.iter().position(|(c, _)| *c == column).map(|i| xs[i]);

    let totals = schedule_totals(rows);
    let pages = usize::max(1, (rows.len() + 1).div_ceil(ROWS_PER_PAGE)); // + totals row

    let mut doc = Document::new(fonts, "Sign schedule");
    for p in 0..pages {
        doc.page(PAGE_W, PAGE_H, |g| {
            let mut pdf = Canvas::new(g, PAGE_H);
            let thin = Pen { colour: BLACK, width: 0.15 };
            let heavy = Pen { colour: BLACK, width: 0.4 };

            pdf.text(&SheetText::new(LEFT, 196.0, 5.0, "SIGN SCHEDULE", "").bold(), BLACK);
            pdf.text(&SheetText::new(LEFT, 189.0, 2.8, format!("Project: {}", info.project), ""), BLACK);
            pdf.text(&SheetText::new(LEFT, 184.0, 2.4, format!("Date: {}    Standard: {}", info.date, info.standard), ""), BLACK);
            pdf.text(&SheetText::new(RIGHT, 196.0, 2.4, format!("Page {} of {pages}", p + 1), "").aligned(SheetTextAlign::Right), BLACK);

            let mut y = TOP_ROW;
            pdf.line(LEFT, y + ROW_H, table_right, y + ROW_H, &heavy);
            for (c, (column, _)) in cols.iter().enumerate() {
                cell(&mut pdf, xs[c], y, column.head(), column.numeric(), true, false);
            }
            pdf.line(LEFT, y - 1.6, table_right, y - 1.6, &heavy);
            y -= ROW_H;

            let first = p * ROWS_PER_PAGE;
            let last = usize::min(rows.len(), first + ROWS_PER_PAGE);
            for (i, row) in rows.iter().enumerate().take(last).skip(first) {
                for (c, (column, _)) in cols.iter().enumerate() {
                    cell(&mut pdf, xs[c], y, &column.cell(i, row), column.numeric(), c == 1, true);
                }
                pdf.line(LEFT, y - 1.6, table_right, y - 1.6, &thin);
                y -= ROW_H;
            }

            if p == pages - 1 {
                pdf.line(LEFT, y + ROW_H - 1.6, table_right, y + ROW_H - 1.6, &heavy);
                cell(&mut pdf, xs[1], y, "TOTAL", false, true, true);
                cell(&mut pdf, xs[2], y, &format!("{} sign(s), {} code(s)", totals.count, totals.rows), false, true, true);
                if let Some(qty) = column_x(Column::Quantity) {
                    cell(&mut pdf, qty, y, &totals.count.to_string(), true, true, true);
                }
                let area = column_x(if has_qty { Column::TotalArea } else { Column::Area }).expect("the area columns are always present");
                cell(&mut pdf, area, y, &f3(totals.area_m2), true, true, true);
                pdf.line(LEFT, y - 1.6, table_right, y - 1.6, &heavy);
                y -= ROW_H * 2.0;
                pdf.text(
                    &SheetText::new(
                        LEFT,
                        y,
                        2.2,
                        "Face area is the gross panel rectangle (width x height), not reduced for corner radii. \
                         Posts, foundations and fixings are excluded.",
                        "",
                    ),
                    BLACK,
                );
            }
        });
    }
    doc.finish(stamp)
}

fn cell(pdf: &mut Canvas, col: (f64, f64), y: f64, text: &str, numeric: bool, bold: bool, truncate: bool) {
    const PAD: f64 = 1.5;
    let h = if truncate { 2.6 } else { 2.3 }; // headers a touch smaller
    // Truncate to the column (estimated advance about 0.75 x cap height per character).
    let max = i64::max(3, ((col.1 - col.0 - 2.0 * PAD) / (0.75 * h)) as i64) as usize;
    let text = if truncate && utf16_len(text) > max { utf16_prefix(text, max - 3) + "..." } else { text.to_string() };
    let mut t = if numeric {
        SheetText::new(col.1 - PAD, y, h, text, "").aligned(SheetTextAlign::Right)
    } else {
        SheetText::new(col.0 + PAD, y, h, text, "")
    };
    t.bold = bold;
    pdf.text(&t, BLACK);
}

fn f3(v: f64) -> String {
    fixed(v, 3, 3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_signs::{Colour, ColourScheme, SignKind};

    fn row(code: &str, w: f64, h: f64, quantity: Option<u32>) -> ScheduleRow {
        ScheduleRow {
            q: SignQuantity {
                code: code.into(),
                catalog_key: None,
                kind: SignKind::StackType,
                scheme: ColourScheme::Auto,
                ground: Colour::Green,
                speed_kph: 60,
                x_height_mm: 125.0,
                sw_mm: 31.25,
                width_mm: w,
                height_mm: h,
                legend_characters: 0,
                symbols: 0,
                warnings: 0,
                errors: 0,
            },
            type_name: "Stack-type".into(),
            quantity,
            location: None,
        }
    }

    #[test]
    fn totals_weight_area_by_quantity() {
        let rows = [row("A", 1000.0, 1000.0, Some(3)), row("B", 2000.0, 500.0, None), row("C", 1000.0, 1000.0, Some(0))];
        let t = schedule_totals(&rows);
        assert_eq!((t.rows, t.count, t.area_m2), (3, 4, 4.0));
    }
}
