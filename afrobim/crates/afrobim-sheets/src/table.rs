//! Tables and notes: a generic table with column widths and wrapping, builders from CSV text and
//! plain rows, and wrapped paragraph text.
//!
//! The builders take plain rows or CSV text, so this crate does not depend on the engine crates
//! that produce them. Feed them the CSV the engines write (`afrobim-cost`, `afrobim-schedule`,
//! the sign schedule, a clash report) or rows you assemble.

use afrobim_draw::Pt2;
use serde::{Deserialize, Serialize};

use crate::geom::Rect;
use crate::item::{layers, text as mk_text, ComposedSheet};
use crate::style::Styles;
use crate::text::{text_width, wrap, Align};

/// A table: headers, rows of text, and how the columns are sized.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Table {
    pub title: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Column widths in mm. A missing or zero entry is sized from the width of the column's text
    /// and shares out what the fixed columns leave.
    pub col_widths_mm: Vec<f64>,
    /// Alignment per column; missing entries are left aligned.
    pub align: Vec<Align>,
}

/// Cell padding left and right, mm.
const PAD_X: f64 = 1.2;
/// Cell padding top and bottom, mm.
const PAD_Y: f64 = 1.0;
/// Line pitch as a multiple of the text height.
const PITCH: f64 = 1.4;

impl Table {
    pub fn new(headers: &[&str], rows: Vec<Vec<String>>) -> Table {
        Table { headers: headers.iter().map(|s| s.to_string()).collect(), rows, ..Table::default() }
    }

    pub fn with_title(mut self, title: &str) -> Table {
        self.title = Some(title.to_string());
        self
    }

    pub fn columns(&self) -> usize {
        self.headers.len().max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0))
    }

    /// A table from CSV text (RFC 4180 style: quoted fields, doubled quotes, CRLF or LF). The first
    /// record is the header. Columns whose every cell is a number are right aligned.
    pub fn from_csv(title: &str, csv: &str) -> Table {
        let mut records = parse_csv(csv);
        let headers = if records.is_empty() { Vec::new() } else { records.remove(0) };
        let mut t = Table { title: if title.is_empty() { None } else { Some(title.to_string()) }, headers, rows: records, ..Table::default() };
        t.right_align_numeric_columns();
        t
    }

    /// Right aligns each column whose cells (blank ones allowed) all read as numbers.
    pub fn right_align_numeric_columns(&mut self) {
        let n = self.columns();
        self.align.resize(n, Align::Left);
        for c in 0..n {
            let cells: Vec<&str> = self.rows.iter().filter_map(|r| r.get(c)).map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if !cells.is_empty() && cells.iter().all(|s| s.replace(',', "").parse::<f64>().is_ok()) {
                self.align[c] = Align::Right;
            }
        }
    }

    /// Fixes column widths (mm) in order.
    pub fn with_widths(mut self, widths: &[f64]) -> Table {
        self.col_widths_mm = widths.to_vec();
        self
    }
}

/// Splits CSV text into records of fields.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                any = false;
            }
            c => field.push(c),
        }
    }
    if any || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    records
}

/// One row of a quantities table.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantityRow {
    pub item: String,
    pub unit: String,
    pub quantity: f64,
    pub decimals: usize,
}

/// A bill-style table of quantities from plain rows.
pub fn quantity_table(title: &str, rows: &[QuantityRow]) -> Table {
    Table {
        title: Some(title.to_string()),
        headers: vec!["Item".into(), "Unit".into(), "Quantity".into()],
        rows: rows.iter().map(|r| vec![r.item.clone(), r.unit.clone(), format!("{:.*}", r.decimals, r.quantity)]).collect(),
        col_widths_mm: vec![0.0, 18.0, 28.0],
        align: vec![Align::Left, Align::Left, Align::Right],
    }
}

/// The sign schedule from its CSV (one row per sign or sign type).
pub fn sign_schedule_table(csv: &str) -> Table {
    Table::from_csv("Sign schedule", csv)
}

/// The priced bill of quantities from its CSV.
pub fn priced_bill_table(csv: &str) -> Table {
    Table::from_csv("Bill of quantities", csv)
}

/// The activity table of a construction programme from its CSV.
pub fn activity_table(csv: &str) -> Table {
    Table::from_csv("Activities", csv)
}

/// Clash results from their CSV.
pub fn clash_table(csv: &str) -> Table {
    Table::from_csv("Clash results", csv)
}

/// A table of `Element`, `Quantity`, `Value` rows, for the derived values modules report
/// (`afrobim_kernel::Generated::derived`). See `view::module_quantities` to collect the rows from
/// a document.
pub fn derived_values_table(title: &str, rows: &[(String, String, String)]) -> Table {
    Table {
        title: Some(title.to_string()),
        headers: vec!["Element".into(), "Quantity".into(), "Value".into()],
        rows: rows.iter().map(|(a, b, c)| vec![a.clone(), b.clone(), c.clone()]).collect(),
        col_widths_mm: vec![0.0, 0.0, 0.0],
        align: vec![Align::Left, Align::Left, Align::Right],
    }
}

/// How a table lays out at a given width.
#[derive(Debug, Clone, PartialEq)]
pub struct TableLayout {
    /// Left edge of each column and the right edge of the last, relative to the table's left.
    pub col_x: Vec<f64>,
    pub header_height_mm: f64,
    /// Height of each body row.
    pub row_heights_mm: Vec<f64>,
    /// Wrapped lines of the header cells.
    pub header_lines: Vec<Vec<String>>,
    /// Wrapped lines of every body cell: `cells[row][col]`.
    pub cells: Vec<Vec<Vec<String>>>,
    pub title_height_mm: f64,
}

impl TableLayout {
    pub fn total_height_mm(&self) -> f64 {
        self.title_height_mm + self.header_height_mm + self.row_heights_mm.iter().sum::<f64>()
    }
}

/// Lays `table` out in `width_mm` with the style's text heights: column widths first, then each
/// cell wrapped to its column, then row heights from the tallest cell.
pub fn layout_table(table: &Table, width_mm: f64, styles: &Styles) -> TableLayout {
    let n = table.columns().max(1);
    let h = styles.text.normal;
    let cell_text = |r: usize, c: usize| table.rows[r].get(c).map(String::as_str).unwrap_or("");
    // Widths: fixed ones as given; the rest share what remains by their widest text.
    let mut widths: Vec<f64> = (0..n).map(|c| table.col_widths_mm.get(c).copied().unwrap_or(0.0)).collect();
    let fixed: f64 = widths.iter().sum();
    let auto: Vec<usize> = (0..n).filter(|c| widths[*c] <= 0.0).collect();
    if !auto.is_empty() {
        let remaining = (width_mm - fixed).max(width_mm * 0.1);
        let want: Vec<f64> = auto
            .iter()
            .map(|&c| {
                let header = table.headers.get(c).map_or(0.0, |t| text_width(t, h, true));
                let body = (0..table.rows.len()).map(|r| text_width(cell_text(r, c), h, false)).fold(0.0, f64::max);
                header.max(body) + 2.0 * PAD_X
            })
            .collect();
        let total: f64 = want.iter().sum();
        let mut share: Vec<f64> = vec![0.0; auto.len()];
        if total <= remaining {
            // Everything fits at its natural width: spread the slack in proportion.
            for (i, w) in want.iter().enumerate() {
                share[i] = w * remaining / total;
            }
        } else {
            // Water-fill: a column that wants less than an equal share of what is left keeps what
            // it wants; the others split the rest in proportion to their wants.
            let mut open: Vec<usize> = (0..auto.len()).collect();
            let mut pool = remaining;
            while let Some(&i) = open.iter().find(|&&i| want[i] <= pool / open.len() as f64) {
                share[i] = want[i];
                pool -= want[i];
                open.retain(|&j| j != i);
            }
            let open_total: f64 = open.iter().map(|&i| want[i]).sum();
            for &i in &open {
                share[i] = pool * want[i] / open_total;
            }
        }
        for (slot, &c) in auto.iter().enumerate() {
            widths[c] = share[slot];
        }
    }
    let mut col_x = vec![0.0];
    for w in &widths {
        col_x.push(col_x.last().copied().unwrap_or(0.0) + w);
    }
    let pitch = h * PITCH;
    let height_of = |lines: &Vec<Vec<String>>| lines.iter().map(|l| l.len().max(1)).max().unwrap_or(1) as f64 * pitch + 2.0 * PAD_Y;
    let header_lines: Vec<Vec<String>> = (0..n).map(|c| wrap(table.headers.get(c).map_or("", String::as_str), widths[c] - 2.0 * PAD_X, h, true)).collect();
    let header_height_mm = if table.headers.is_empty() { 0.0 } else { height_of(&header_lines) };
    let cells: Vec<Vec<Vec<String>>> = (0..table.rows.len()).map(|r| (0..n).map(|c| wrap(cell_text(r, c), widths[c] - 2.0 * PAD_X, h, false)).collect()).collect();
    let row_heights_mm = cells.iter().map(height_of).collect();
    let title_height_mm = if table.title.is_some() { styles.text.heading * 2.0 } else { 0.0 };
    TableLayout { col_x, header_height_mm, row_heights_mm, header_lines, cells, title_height_mm }
}

/// How many body rows from `first` fit in `height_mm` below the title and header; at least one
/// when any rows remain, so a very tall row cannot stall pagination.
pub fn rows_that_fit(layout: &TableLayout, first: usize, height_mm: f64) -> usize {
    let mut used = layout.title_height_mm + layout.header_height_mm;
    let mut n = 0;
    for h in layout.row_heights_mm.iter().skip(first) {
        if used + h > height_mm + 1e-9 && n > 0 {
            break;
        }
        used += h;
        n += 1;
        if used > height_mm {
            break;
        }
    }
    n
}

/// Draws rows `first .. first + count` of the table (header and title above them) with its top
/// left corner at the top left of `area`. Returns the height used.
pub fn draw_table(sheet: &mut ComposedSheet, table: &Table, layout: &TableLayout, first: usize, count: usize, area: &Rect, styles: &Styles) -> f64 {
    let pen = styles.pens.pen(layers::TABLE);
    let h = styles.text.normal;
    let pitch = h * PITCH;
    let n = layout.col_x.len() - 1;
    let x0 = area.x0;
    let mut y = area.y1;
    if let Some(title) = &table.title {
        sheet.text(mk_text(layers::TABLE_TEXT, Pt2::new(x0, y - styles.text.heading * 1.3), styles.text.heading, title.clone(), Align::Left, 0.0, true, pen.rgb));
        y -= layout.title_height_mm;
    }
    let align_of = |c: usize| table.align.get(c).copied().unwrap_or(Align::Left);
    let place = |sheet: &mut ComposedSheet, lines: &[String], c: usize, top: f64, bold: bool, align: Align| {
        for (i, line) in lines.iter().enumerate() {
            let (cx0, cx1) = (x0 + layout.col_x[c], x0 + layout.col_x[c + 1]);
            let (x, a) = match align {
                Align::Left => (cx0 + PAD_X, Align::Left),
                Align::Centre => ((cx0 + cx1) / 2.0, Align::Centre),
                Align::Right => (cx1 - PAD_X, Align::Right),
            };
            sheet.text(mk_text(layers::TABLE_TEXT, Pt2::new(x, top - PAD_Y - h - i as f64 * pitch), h, line.clone(), a, 0.0, bold, pen.rgb));
        }
    };
    let draw_row = |sheet: &mut ComposedSheet, top: f64, height: f64, cells: &dyn Fn(usize) -> Vec<String>, bold: bool, header: bool| {
        let r = Rect::new(x0, top - height, x0 + layout.col_x[n], top);
        if header {
            sheet.fill_rect(layers::TABLE, &r, [225, 225, 225]);
        }
        for c in 0..n {
            let cell = Rect::new(x0 + layout.col_x[c], top - height, x0 + layout.col_x[c + 1], top);
            sheet.rect(layers::TABLE, &cell, pen.weight_mm, pen.rgb);
            place(sheet, &cells(c), c, top, bold, if header { Align::Left } else { align_of(c) });
        }
    };
    if layout.header_height_mm > 0.0 {
        draw_row(sheet, y, layout.header_height_mm, &|c| layout.header_lines[c].clone(), true, true);
        y -= layout.header_height_mm;
    }
    for r in first..(first + count).min(layout.row_heights_mm.len()) {
        draw_row(sheet, y, layout.row_heights_mm[r], &|c| layout.cells[r][c].clone(), false, false);
        y -= layout.row_heights_mm[r];
    }
    area.y1 - y
}

/// Draws a table in `area`, as many rows as fit; warns about the rest. Returns the number of
/// rows drawn.
pub fn draw_table_in(sheet: &mut ComposedSheet, table: &Table, area: &Rect, styles: &Styles) -> usize {
    let layout = layout_table(table, area.width(), styles);
    let fit = rows_that_fit(&layout, 0, area.height()).min(table.rows.len());
    draw_table(sheet, table, &layout, 0, fit, area, styles);
    if fit < table.rows.len() {
        sheet.warn(format!("Table '{}': {} of {} rows fit; the rest need another sheet (use a table sheet series).", table.title.as_deref().unwrap_or(""), fit, table.rows.len()));
    }
    fit
}

/// Notes: a heading and paragraphs wrapped to the area width, optionally numbered.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Notes {
    pub heading: Option<String>,
    pub paragraphs: Vec<String>,
    pub numbered: bool,
}

/// Draws notes from the top left of `area` down; warns when they run past the bottom. Returns the
/// number of text lines set.
pub fn draw_notes(sheet: &mut ComposedSheet, notes: &Notes, area: &Rect, styles: &Styles) -> usize {
    let pen = styles.pens.pen(layers::NOTES);
    let h = styles.text.normal;
    let pitch = h * PITCH;
    let mut y = area.y1;
    let mut count = 0;
    if let Some(head) = &notes.heading {
        y -= styles.text.heading * 1.3;
        sheet.text(mk_text(layers::NOTES, Pt2::new(area.x0, y), styles.text.heading, head.clone(), Align::Left, 0.0, true, pen.rgb));
        y -= styles.text.heading * 0.7;
    }
    let indent = if notes.numbered { text_width("99. ", h, false) } else { 0.0 };
    for (i, p) in notes.paragraphs.iter().enumerate() {
        let lines = wrap(p, area.width() - indent, h, false);
        for (j, line) in lines.iter().enumerate() {
            y -= pitch;
            if y < area.y0 {
                sheet.warn("The notes run past the bottom of their area and are cut off.");
                return count;
            }
            if notes.numbered && j == 0 {
                sheet.text(mk_text(layers::NOTES, Pt2::new(area.x0, y), h, format!("{}.", i + 1), Align::Left, 0.0, false, pen.rgb));
            }
            sheet.text(mk_text(layers::NOTES, Pt2::new(area.x0 + indent, y), h, line.clone(), Align::Left, 0.0, false, pen.rgb));
            count += 1;
        }
        y -= pitch * 0.5;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parses_quotes_commas_and_crlf() {
        let r = parse_csv("a,b,c\r\n1,\"x, y\",\"say \"\"hi\"\"\"\r\n\r\n3,,\n");
        assert_eq!(r, vec![vec!["a", "b", "c"], vec!["1", "x, y", "say \"hi\""], vec!["3", "", ""]]);
        assert_eq!(parse_csv("").len(), 0);
        assert_eq!(parse_csv("a,b"), vec![vec!["a", "b"]]);
    }

    #[test]
    fn numeric_columns_are_right_aligned() {
        let t = Table::from_csv("T", "Item,Unit,Qty\nCut,m3,1200.5\nFill,m3,30\n");
        assert_eq!(t.headers, vec!["Item", "Unit", "Qty"]);
        assert_eq!(t.align, vec![Align::Left, Align::Left, Align::Right]);
        assert_eq!(t.title.as_deref(), Some("T"));
    }

    #[test]
    fn column_widths_fill_the_table_and_fixed_ones_stay() {
        let t = Table::new(&["Description", "Unit", "Qty"], vec![vec!["Excavation in soft material".into(), "m3".into(), "1200".into()]]).with_widths(&[0.0, 20.0, 0.0]);
        let l = layout_table(&t, 150.0, &Styles::default());
        assert!((l.col_x[3] - 150.0).abs() < 1e-6, "{:?}", l.col_x);
        assert!((l.col_x[2] - l.col_x[1] - 20.0).abs() < 1e-9);
    }

    #[test]
    fn long_cells_wrap_and_make_the_row_taller() {
        let long = "Granular sub-base of crushed stone compacted in layers and tested to the specified density".to_string();
        let t = Table::new(&["Description", "Qty"], vec![vec![long.clone(), "10".into()], vec!["Short".into(), "2".into()]]).with_widths(&[60.0, 20.0]);
        let styles = Styles::default();
        let l = layout_table(&t, 80.0, &styles);
        assert!(l.cells[0][0].len() >= 2, "{:?}", l.cells[0][0]);
        assert_eq!(l.cells[0][0].join(" "), long);
        for line in &l.cells[0][0] {
            assert!(text_width(line, styles.text.normal, false) <= 60.0 - 2.4 + 1e-9);
        }
        // Row height = lines x 1.4 x 2.5 mm + 2 mm padding.
        let pitch = 2.5 * 1.4;
        assert!((l.row_heights_mm[0] - (l.cells[0][0].len() as f64 * pitch + 2.0)).abs() < 1e-9);
        assert!(l.row_heights_mm[0] > l.row_heights_mm[1]);
        assert!((l.row_heights_mm[1] - (pitch + 2.0)).abs() < 1e-9);
    }

    #[test]
    fn rows_that_fit_counts_rows_below_the_header() {
        let rows: Vec<Vec<String>> = (0..30).map(|i| vec![format!("r{i}"), "x".into()]).collect();
        let t = Table::new(&["A", "B"], rows).with_widths(&[30.0, 30.0]);
        let l = layout_table(&t, 60.0, &Styles::default());
        // Header and each row are 3.5 + 2 = 5.5 mm tall. 60 mm leaves 54.5 for rows: 9 rows.
        assert_eq!(rows_that_fit(&l, 0, 60.0), 9);
        assert_eq!(rows_that_fit(&l, 25, 60.0), 5);
        let mut sheet = ComposedSheet::new("1", "t", 100.0, 100.0);
        let drawn = draw_table_in(&mut sheet, &t, &Rect::new(0.0, 0.0, 60.0, 60.0), &Styles::default());
        assert_eq!(drawn, 9);
        assert_eq!(sheet.warnings.len(), 1);
    }

    #[test]
    fn notes_wrap_number_and_warn_on_overflow() {
        let notes = Notes { heading: Some("NOTES".into()), paragraphs: vec!["First note that is long enough to need wrapping onto a second line when the area is narrow.".into(), "Second.".into()], numbered: true };
        let mut sheet = ComposedSheet::new("1", "t", 200.0, 200.0);
        let n = draw_notes(&mut sheet, &notes, &Rect::new(10.0, 10.0, 70.0, 190.0), &Styles::default());
        assert!(n >= 3);
        let t: Vec<&str> = sheet.texts().map(|t| t.text.as_str()).collect();
        assert!(t.contains(&"1.") && t.contains(&"2.") && t.contains(&"NOTES"));
        let mut tight = ComposedSheet::new("1", "t", 200.0, 200.0);
        draw_notes(&mut tight, &notes, &Rect::new(10.0, 10.0, 70.0, 30.0), &Styles::default());
        assert_eq!(tight.warnings.len(), 1);
    }

    #[test]
    fn quantity_helper_formats_numbers() {
        let t = quantity_table("Q", &[QuantityRow { item: "Cut".into(), unit: "m3".into(), quantity: 1234.5678, decimals: 1 }]);
        assert_eq!(t.rows[0], vec!["Cut", "m3", "1234.6"]);
        assert_eq!(derived_values_table("D", &[("E".into(), "k".into(), "v".into())]).rows.len(), 1);
    }
}
