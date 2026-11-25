// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Delimited point files: reading, writing and format detection.
//!
//! Column orders use the same codes as `afrobim-corridor` (`survey.rs`): `P` point number,
//! `N` northing, `E` easting, `Z` level, `D` description. The orders `PNEZD`, `PENZD`, `NEZD` and
//! `ENZD` mean the same here. Differences from that reader: here `NEZ` and `ENZ` (and `PNEZ`, `PENZ`)
//! are separate orders **without** a description column (in `survey.rs` `NEZ` is an alias of
//! `NEZD`); a semicolon is read as a delimiter in both; the corridor reader drops the point number
//! while this one keeps it; and a level field that is empty or equal to the format's
//! `missing_elevation` marker gives a point without elevation instead of a skipped line.
//!
//! The order is **not** known from the file's content. [`detect_format`] ranks the orders by
//! heuristics and reports how sure it is; the caller must check the preview and pass the chosen
//! [`PointFileFormat`] to [`parse_points`].
//!
//! Units: metres only. A file in other units is rejected unless the caller supplies a factor to
//! metres ([`unit_factor`]).

use serde::{Deserialize, Serialize};

use crate::error::{CogoError, Result};
use crate::point::{CogoPoint, PointStore, SourceRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnOrder {
    Pnezd,
    Penzd,
    Pnez,
    Penz,
    Nezd,
    Enzd,
    Nez,
    Enz,
}

impl ColumnOrder {
    pub const ALL: [ColumnOrder; 8] = [
        ColumnOrder::Pnezd,
        ColumnOrder::Penzd,
        ColumnOrder::Nezd,
        ColumnOrder::Enzd,
        ColumnOrder::Pnez,
        ColumnOrder::Penz,
        ColumnOrder::Nez,
        ColumnOrder::Enz,
    ];
    pub fn code(self) -> &'static str {
        match self {
            ColumnOrder::Pnezd => "PNEZD",
            ColumnOrder::Penzd => "PENZD",
            ColumnOrder::Pnez => "PNEZ",
            ColumnOrder::Penz => "PENZ",
            ColumnOrder::Nezd => "NEZD",
            ColumnOrder::Enzd => "ENZD",
            ColumnOrder::Nez => "NEZ",
            ColumnOrder::Enz => "ENZ",
        }
    }
    pub fn parse(s: &str) -> Option<ColumnOrder> {
        let u = s.trim().to_ascii_uppercase();
        ColumnOrder::ALL.into_iter().find(|o| o.code() == u)
    }
    pub fn has_point(self) -> bool {
        matches!(self, ColumnOrder::Pnezd | ColumnOrder::Penzd | ColumnOrder::Pnez | ColumnOrder::Penz)
    }
    pub fn has_description(self) -> bool {
        matches!(self, ColumnOrder::Pnezd | ColumnOrder::Penzd | ColumnOrder::Nezd | ColumnOrder::Enzd)
    }
    pub fn northing_first(self) -> bool {
        matches!(self, ColumnOrder::Pnezd | ColumnOrder::Pnez | ColumnOrder::Nezd | ColumnOrder::Nez)
    }
    /// Columns before the description.
    pub fn numeric_columns(self) -> usize {
        if self.has_point() {
            4
        } else {
            3
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Delimiter {
    Comma,
    Space,
    Tab,
    Semicolon,
}

impl Delimiter {
    pub fn ch(self) -> char {
        match self {
            Delimiter::Comma => ',',
            Delimiter::Space => ' ',
            Delimiter::Tab => '\t',
            Delimiter::Semicolon => ';',
        }
    }
    const ALL: [Delimiter; 4] = [Delimiter::Comma, Delimiter::Tab, Delimiter::Semicolon, Delimiter::Space];
}

/// The factor to metres for a file's units. `"m"`, `"metre"`, `"meter"`, `"metres"`, `"meters"`
/// (any case) mean 1. Any other label needs an explicit `factor` (metres per file unit), which is
/// the caller's responsibility; without one the units are rejected.
pub fn unit_factor(label: &str, factor: Option<f64>) -> Result<f64> {
    let l = label.trim().to_ascii_lowercase();
    if matches!(l.as_str(), "m" | "metre" | "meter" | "metres" | "meters") {
        return Ok(1.0);
    }
    match factor {
        Some(f) if f.is_finite() && f > 0.0 => Ok(f),
        _ => Err(CogoError::UnsupportedUnits(label.to_string())),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointFileFormat {
    pub order: ColumnOrder,
    pub delimiter: Delimiter,
    /// Metres per file unit; 1 for metres. Applied to easting, northing and elevation.
    pub unit_factor: f64,
    /// A level field with this text means "no elevation" (an empty field always does).
    pub missing_elevation: Option<String>,
    /// First number given to points the file does not number (and to non-numeric point names).
    pub start_number: u32,
}

impl PointFileFormat {
    pub fn metres(order: ColumnOrder, delimiter: Delimiter) -> PointFileFormat {
        PointFileFormat { order, delimiter, unit_factor: 1.0, missing_elevation: None, start_number: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineIssue {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParsedPoints {
    pub points: Vec<CogoPoint>,
    pub issues: Vec<LineIssue>,
    pub header_skipped: bool,
}

/// Split a line into fields. Double quotes protect delimiters; `""` inside quotes is a quote.
/// With a space delimiter, runs of blanks (and tabs) separate fields.
pub fn split_fields(line: &str, d: Delimiter) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut inq = false;
    let mut had = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if inq {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    inq = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            inq = true;
            had = true;
        } else if (d == Delimiter::Space && (c == ' ' || c == '\t')) || (d != Delimiter::Space && c == d.ch()) {
            if d == Delimiter::Space {
                if had || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    had = false;
                }
            } else {
                out.push(std::mem::take(&mut cur).trim().to_string());
                had = false;
            }
        } else {
            cur.push(c);
        }
    }
    if d == Delimiter::Space {
        if had || !cur.is_empty() {
            out.push(cur);
        }
    } else {
        out.push(cur.trim().to_string());
    }
    out
}

fn num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn lines_of(text: &str) -> Vec<(usize, &str)> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    t.lines().enumerate().map(|(i, l)| (i + 1, l.trim_end_matches('\r'))).filter(|(_, l)| !l.trim().is_empty()).collect()
}

struct Row {
    p: Option<String>,
    e: f64,
    n: f64,
    z: Option<f64>,
    d: String,
}

fn read_row(fields: &[String], f: &PointFileFormat) -> std::result::Result<Row, String> {
    let o = f.order;
    let k = o.numeric_columns();
    if fields.len() < k {
        return Err(format!("{} fields, {} needs at least {}", fields.len(), o.code(), k));
    }
    if !o.has_description() && fields.len() > k {
        return Err(format!("{} fields, {} has exactly {}", fields.len(), o.code(), k));
    }
    let mut i = 0;
    let p = if o.has_point() {
        i = 1;
        Some(fields[0].clone())
    } else {
        None
    };
    let (a, b) = (num(&fields[i]), num(&fields[i + 1]));
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err("easting or northing is not a number".into()),
    };
    let (n, e) = if o.northing_first() { (a, b) } else { (b, a) };
    let zt = fields[i + 2].trim();
    let z = if zt.is_empty() || f.missing_elevation.as_deref() == Some(zt) {
        None
    } else {
        Some(num(zt).ok_or_else(|| "level is not a number".to_string())? * f.unit_factor)
    };
    let d = if o.has_description() { fields[k..].join(" ") } else { String::new() };
    Ok(Row { p, e: e * f.unit_factor, n: n * f.unit_factor, z, d })
}

/// Read points from text with a stated format. Unreadable lines (other than a heading line) are
/// reported in `issues`. Points without a numeric number get numbers from `start_number` upward,
/// skipping numbers the file uses; a non-numeric point field becomes the point's name. Duplicate
/// numbers in the file are kept here and dealt with by [`import_into`].
pub fn parse_points(text: &str, f: &PointFileFormat, file_name: Option<&str>) -> Result<ParsedPoints> {
    if !(f.unit_factor.is_finite() && f.unit_factor > 0.0) {
        return Err(CogoError::Invalid("unit factor must be positive".into()));
    }
    let mut out = ParsedPoints::default();
    let mut rows: Vec<(usize, &str, Row)> = Vec::new();
    for (idx, (ln, l)) in lines_of(text).into_iter().enumerate() {
        let fields = split_fields(l, f.delimiter);
        match read_row(&fields, f) {
            Ok(r) => rows.push((ln, l, r)),
            Err(m) => {
                if idx == 0 && rows.is_empty() && m.contains("not a number") {
                    out.header_skipped = true;
                } else {
                    out.issues.push(LineIssue { line: ln, message: m });
                }
            }
        }
    }
    let used: std::collections::BTreeSet<u32> = rows.iter().filter_map(|(_, _, r)| r.p.as_ref().and_then(|p| p.trim().parse::<u32>().ok())).collect();
    let mut next = f.start_number.max(1);
    for (ln, raw, r) in rows {
        let (number, name) = match r.p.as_deref().map(str::trim) {
            Some(p) if p.parse::<u32>().is_ok() => (p.parse::<u32>().unwrap(), None),
            other => {
                while used.contains(&next) {
                    next += 1;
                }
                let n = next;
                next += 1;
                (n, other.filter(|s| !s.is_empty()).map(str::to_string))
            }
        };
        let mut p = CogoPoint::new(number, r.e, r.n, r.z, &r.d);
        p.name = name;
        p.source = SourceRecord { file: file_name.map(str::to_string), line: Some(ln), raw: Some(raw.to_string()), note: None };
        out.points.push(p);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicatePolicy {
    /// Stop at the first clash and leave the store unchanged.
    Reject,
    /// Leave out points whose number is taken.
    Skip,
    /// Give clashing points the next free number.
    RenumberToFree,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ImportResult {
    pub added: Vec<u32>,
    pub skipped: Vec<u32>,
    /// (number in file, number given)
    pub renumbered: Vec<(u32, u32)>,
}

/// Put parsed points in a store, dealing with number clashes by `policy`.
pub fn import_into(store: &mut PointStore, parsed: Vec<CogoPoint>, policy: DuplicatePolicy) -> Result<ImportResult> {
    if policy == DuplicatePolicy::Reject {
        let mut seen = std::collections::BTreeSet::new();
        for p in &parsed {
            if store.get(p.number).is_some() || !seen.insert(p.number) {
                return Err(CogoError::DuplicateNumber(p.number));
            }
        }
    }
    let mut r = ImportResult::default();
    // Points whose number is free keep it; the clashing ones then take the next free numbers, so a
    // renumbered point never takes a number a later point in the file wants.
    let mut clashing = Vec::new();
    for p in parsed {
        let n = p.number;
        if store.get(n).is_none() {
            store.add(p)?;
            r.added.push(n);
        } else if policy == DuplicatePolicy::Skip {
            r.skipped.push(n);
        } else {
            clashing.push(p);
        }
    }
    for p in clashing {
        let n = p.number;
        let m = store.add_next_free(p)?;
        r.renumbered.push((n, m));
        r.added.push(m);
    }
    Ok(r)
}

/// How a point file is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportOptions {
    pub order: ColumnOrder,
    pub delimiter: Delimiter,
    /// Decimal places for coordinates and levels. 4 keeps 0.0001 m.
    pub decimals: usize,
    /// Text written for a missing elevation. When `None` a point without elevation is an error
    /// for orders that carry a level.
    pub missing_elevation: Option<String>,
    /// Write the order code as a heading line.
    pub header: bool,
}

fn quote(s: &str, d: Delimiter) -> String {
    let need = s.contains('"') || s.contains('\n') || (d != Delimiter::Space && s.contains(d.ch()));
    if need {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Write points (in the order given) as delimited text. Uses `full_description` when `use_full` is
/// set and it is not empty, otherwise the raw description.
pub fn export_points<'a, I: IntoIterator<Item = &'a CogoPoint>>(points: I, o: &ExportOptions, use_full: bool) -> Result<String> {
    let sep = o.delimiter.ch().to_string();
    let mut s = String::new();
    if o.header {
        s.push_str(o.order.code());
        s.push('\n');
    }
    for p in points {
        let z = match (p.elevation, &o.missing_elevation) {
            (Some(z), _) => format!("{:.*}", o.decimals, z),
            (None, Some(m)) => m.clone(),
            (None, None) => return Err(CogoError::MissingElevation(p.number)),
        };
        let e = format!("{:.*}", o.decimals, p.easting);
        let n = format!("{:.*}", o.decimals, p.northing);
        let mut f: Vec<String> = Vec::new();
        if o.order.has_point() {
            f.push(p.number.to_string());
        }
        if o.order.northing_first() {
            f.extend([n, e, z]);
        } else {
            f.extend([e, n, z]);
        }
        if o.order.has_description() {
            let d = if use_full && !p.full_description.is_empty() { &p.full_description } else { &p.raw_description };
            f.push(quote(d, o.delimiter));
        }
        s.push_str(&f.join(&sep));
        s.push('\n');
    }
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// Detection

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub order: ColumnOrder,
    pub score: f64,
    /// Fraction of sampled lines that read under this order.
    pub parse_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewRow {
    pub line: usize,
    pub raw: String,
    pub fields: Vec<String>,
    pub number: Option<String>,
    pub easting: Option<f64>,
    pub northing: Option<f64>,
    pub elevation: Option<f64>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionReport {
    pub delimiter: Delimiter,
    pub delimiter_consistency: f64,
    pub header_found: bool,
    /// Best first.
    pub candidates: Vec<Candidate>,
    pub best: Option<PointFileFormat>,
    /// 0 to 1. A heuristic, not a probability: see the README.
    pub confidence: f64,
    /// True when the swap of easting and northing (or another order) scores nearly as well.
    pub ambiguous: bool,
    pub notes: Vec<String>,
    /// The first rows read under the best order.
    pub preview: Vec<PreviewRow>,
    pub lines_sampled: usize,
}

const SAMPLE: usize = 200;

fn detect_delimiter(lines: &[(usize, &str)]) -> (Delimiter, f64) {
    // (delimiter, mode field count, share of lines with that count)
    let mut stats: Vec<(Delimiter, usize, f64)> = Vec::new();
    for d in Delimiter::ALL {
        let counts: Vec<usize> = lines.iter().map(|(_, l)| split_fields(l, d).len()).collect();
        let mut mode = (0usize, 0usize);
        for m in 1..=60 {
            let c = counts.iter().filter(|x| **x == m).count();
            if c >= mode.1 {
                mode = (m, c);
            }
        }
        stats.push((d, mode.0, mode.1 as f64 / counts.len().max(1) as f64));
    }
    // Comma, tab and semicolon win when they give a steady count of three or more fields.
    for (d, m, f) in &stats {
        if *d != Delimiter::Space && *m >= 3 && *f >= 0.9 {
            return (*d, *f);
        }
    }
    let sp = stats.iter().find(|s| s.0 == Delimiter::Space).unwrap();
    if sp.1 >= 3 {
        return (sp.0, sp.2);
    }
    let best = stats.iter().max_by(|a, b| a.2.partial_cmp(&b.2).unwrap()).unwrap();
    (best.0, best.2)
}

/// Rank the column orders for a file and report how sure the ranking is.
///
/// Heuristics (all documented as heuristics, none is a standard):
/// 1. the delimiter is the one (comma, tab, semicolon, blanks) giving a steady field count of three or
///    more on most lines;
/// 2. each order scores by the share of lines it can read, plus 0.15 when the first column looks like
///    a point number (whole, unique) and the order has one (minus 0.15 when it does not), plus 0.1
///    when trailing fields of a described order are mostly not numbers (minus 0.1 when all are);
/// 3. easting/northing swap: when the second numeric column is larger than the first on at least 90 %
///    of lines, the order putting it in the northing role gains 0.05 (true of UTM in the southern
///    hemisphere, not in general), and the other loses 0.05.
/// Confidence is `0.5 + 2.5 * margin` between the best and second best scores (capped to [0.05, 0.99]),
/// times the best parse fraction. A margin under 0.1 sets `ambiguous`.
pub fn detect_format(text: &str, preview_rows: usize) -> DetectionReport {
    let all = lines_of(text);
    let sample: Vec<(usize, &str)> = all.iter().take(SAMPLE).cloned().collect();
    let mut notes = Vec::new();
    if sample.is_empty() {
        return DetectionReport {
            delimiter: Delimiter::Comma,
            delimiter_consistency: 0.0,
            header_found: false,
            candidates: vec![],
            best: None,
            confidence: 0.0,
            ambiguous: false,
            notes: vec!["no data lines".into()],
            preview: vec![],
            lines_sampled: 0,
        };
    }
    let (delim, consistency) = detect_delimiter(&sample);
    if consistency < 0.9 {
        notes.push(format!("field count is steady on only {:.0} % of lines", consistency * 100.0));
    }
    // heading: first line where no field in positions 0..3 is numeric at all
    let first_fields = split_fields(sample[0].1, delim);
    let header_found = first_fields.iter().take(4).filter(|f| num(f).is_some()).count() < 2;
    let data: Vec<(usize, &str)> = if header_found { sample[1..].to_vec() } else { sample.clone() };
    if header_found {
        notes.push("first line looks like a heading and was left out".into());
    }
    let split: Vec<Vec<String>> = data.iter().map(|(_, l)| split_fields(l, delim)).collect();
    let n = split.len().max(1) as f64;

    // first column looks like a point number?
    let first_ints: Vec<Option<u64>> = split.iter().map(|f| f.first().and_then(|s| s.trim().parse::<u64>().ok())).collect();
    let all_int = !first_ints.is_empty() && first_ints.iter().all(|x| x.is_some());
    let unique = {
        let mut v: Vec<u64> = first_ints.iter().flatten().copied().collect();
        v.sort_unstable();
        let l = v.len();
        v.dedup();
        v.len() == l
    };
    let first_is_point = all_int && unique;

    let mut cands: Vec<Candidate> = Vec::new();
    for o in ColumnOrder::ALL {
        let fm = PointFileFormat::metres(o, delim);
        let mut ok = 0usize;
        let mut desc_text = 0usize;
        let mut desc_n = 0usize;
        let mut second_gt_first = 0usize;
        for fields in &split {
            if let Ok(r) = read_row(fields, &fm) {
                ok += 1;
                let k = o.numeric_columns();
                if o.has_description() && fields.len() > k {
                    desc_n += 1;
                    if fields[k..].iter().any(|s| num(s).is_none()) {
                        desc_text += 1;
                    }
                }
                let i = if o.has_point() { 1 } else { 0 };
                let (x, y) = (num(&fields[i]).unwrap_or(0.0), num(&fields[i + 1]).unwrap_or(0.0));
                if y > x {
                    second_gt_first += 1;
                }
                let _ = r;
            }
        }
        let pf = ok as f64 / n;
        let mut score = pf;
        if o.has_point() {
            score += if first_is_point { 0.15 } else { -0.15 };
        } else if first_is_point {
            score -= 0.15;
        }
        if o.has_description() && desc_n > 0 {
            score += if desc_text * 2 >= desc_n { 0.1 } else { -0.1 };
        }
        if ok > 0 && second_gt_first as f64 / ok as f64 >= 0.9 {
            // second numeric column larger than the first
            score += if o.northing_first() { -0.05 } else { 0.05 };
            // columns are (first, second) = (N, E) for northing-first: second larger means E > N
            // so northing-first loses; easting-first (second = N larger) gains.
        }
        cands.push(Candidate { order: o, score, parse_fraction: pf });
    }
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap().then(a.order.code().cmp(b.order.code())));
    let top = cands[0].clone();
    let second = cands.get(1).map(|c| c.score).unwrap_or(0.0);
    let margin = top.score - second;
    let confidence = if top.parse_fraction <= 0.0 { 0.0 } else { (0.5 + 2.5 * margin).clamp(0.05, 0.99) * top.parse_fraction };
    let ambiguous = margin < 0.1;
    if ambiguous {
        notes.push(format!("{} and {} score within {:.2}: check which column is easting before use", top.order.code(), cands[1].order.code(), margin));
    }
    if top.parse_fraction < 0.9 {
        notes.push(format!("only {:.0} % of sampled lines read under {}", top.parse_fraction * 100.0, top.order.code()));
    }
    notes.push("units are assumed to be metres; other units are not detected".into());
    let best = if top.parse_fraction > 0.0 { Some(PointFileFormat::metres(top.order, delim)) } else { None };
    let mut preview = Vec::new();
    if let Some(b) = &best {
        for (ln, l) in data.iter().take(preview_rows) {
            let fields = split_fields(l, delim);
            match read_row(&fields, b) {
                Ok(r) => preview.push(PreviewRow { line: *ln, raw: l.to_string(), fields, number: r.p, easting: Some(r.e), northing: Some(r.n), elevation: r.z, description: r.d }),
                Err(_) => preview.push(PreviewRow { line: *ln, raw: l.to_string(), fields, number: None, easting: None, northing: None, elevation: None, description: String::new() }),
            }
        }
    }
    DetectionReport { delimiter: delim, delimiter_consistency: consistency, header_found, candidates: cands, best, confidence, ambiguous, notes, preview, lines_sampled: sample.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTM: &str = "1,500000.1234,9999999.1234,12.345,TR 3\n2,500010.0001,9999990.0002,12.5,EP\n3,500020.5,9999980.25,,CL\n";

    #[test]
    fn parse_pnezd_without_loss() {
        let f = PointFileFormat::metres(ColumnOrder::Pnezd, Delimiter::Comma);
        let r = parse_points(UTM, &f, Some("a.csv")).unwrap();
        assert_eq!(r.points.len(), 3);
        // PNEZD: northing is the second column
        assert_eq!(r.points[0].northing, 500000.1234);
        assert_eq!(r.points[0].easting, 9999999.1234);
        let g = PointFileFormat::metres(ColumnOrder::Penzd, Delimiter::Comma);
        let r = parse_points(UTM, &g, None).unwrap();
        assert_eq!(r.points[0].easting, 500000.1234);
        assert_eq!(r.points[0].northing, 9999999.1234);
        assert_eq!(r.points[0].raw_description, "TR 3");
        assert_eq!(r.points[2].elevation, None);
        assert_eq!(r.points[1].source.line, Some(2));
    }

    #[test]
    fn export_round_trip_at_utm_magnitude() {
        let g = PointFileFormat::metres(ColumnOrder::Penzd, Delimiter::Comma);
        let r = parse_points(UTM, &g, None).unwrap();
        let o = ExportOptions { order: ColumnOrder::Penzd, delimiter: Delimiter::Tab, decimals: 4, missing_elevation: Some("".into()), header: false };
        let text = export_points(r.points.iter(), &o, false).unwrap();
        let g2 = PointFileFormat::metres(ColumnOrder::Penzd, Delimiter::Tab);
        let r2 = parse_points(&text, &g2, None).unwrap();
        for (a, b) in r.points.iter().zip(&r2.points) {
            assert!((a.easting - b.easting).abs() < 1e-9 && (a.northing - b.northing).abs() < 1e-9, "{text}");
            assert_eq!(a.elevation, b.elevation);
            assert_eq!(a.raw_description, b.raw_description);
        }
        let o2 = ExportOptions { missing_elevation: None, ..o };
        assert_eq!(export_points(r.points.iter(), &o2, false), Err(CogoError::MissingElevation(3)));
    }

    #[test]
    fn space_delimited_description_with_words_and_quotes() {
        let t = "1 100.5 200.25 3.0 TOP OF BANK\n2 101 201 4 \"EP, LEFT\"\n";
        let f = PointFileFormat::metres(ColumnOrder::Penzd, Delimiter::Space);
        let r = parse_points(t, &f, None).unwrap();
        assert_eq!(r.points[0].raw_description, "TOP OF BANK");
        assert_eq!(r.points[1].raw_description, "EP, LEFT");
    }

    #[test]
    fn header_bad_lines_and_numbering() {
        let t = "\u{feff}P,E,N,Z,D\r\n7,1,2,3,A\r\nxx,5,6,7,B\r\n8,oops,2,3,C\r\n";
        let f = PointFileFormat::metres(ColumnOrder::Penzd, Delimiter::Comma);
        let r = parse_points(t, &f, None).unwrap();
        assert!(r.header_skipped);
        assert_eq!(r.points.len(), 2);
        assert_eq!(r.points[1].name.as_deref(), Some("xx"));
        assert_eq!(r.points[1].number, 1);
        assert_eq!(r.issues.len(), 1);
        assert_eq!(r.issues[0].line, 4);
    }

    #[test]
    fn no_description_orders() {
        let f = PointFileFormat::metres(ColumnOrder::Enz, Delimiter::Comma);
        let r = parse_points("1.5,2.5,3.5\n4,5,6\n", &f, None).unwrap();
        assert_eq!(r.points[1].number, 2);
        assert_eq!(r.points[0].easting, 1.5);
        let r = parse_points("1.5,2.5,3.5,X\n", &f, None).unwrap();
        assert_eq!(r.issues.len(), 1);
    }

    #[test]
    fn units_rejected_without_factor() {
        assert_eq!(unit_factor("m", None), Ok(1.0));
        assert!(matches!(unit_factor("ft", None), Err(CogoError::UnsupportedUnits(_))));
        assert_eq!(unit_factor("ft", Some(0.3048)), Ok(0.3048));
        assert!(unit_factor("ft", Some(-1.0)).is_err());
        let mut f = PointFileFormat::metres(ColumnOrder::Penz, Delimiter::Comma);
        f.unit_factor = 0.5;
        let r = parse_points("1,10,20,30\n", &f, None).unwrap();
        assert_eq!((r.points[0].easting, r.points[0].northing, r.points[0].elevation), (5.0, 10.0, Some(15.0)));
    }

    #[test]
    fn import_policies() {
        let mut s = PointStore::new();
        s.add(CogoPoint::new(1, 0.0, 0.0, None, "")).unwrap();
        let f = PointFileFormat::metres(ColumnOrder::Penzd, Delimiter::Comma);
        let r = parse_points("1,1,1,1,A\n2,2,2,2,B\n", &f, None).unwrap();
        assert_eq!(import_into(&mut s, r.points.clone(), DuplicatePolicy::Reject), Err(CogoError::DuplicateNumber(1)));
        assert_eq!(s.len(), 1);
        let a = import_into(&mut s, r.points.clone(), DuplicatePolicy::Skip).unwrap();
        assert_eq!(a.skipped, vec![1]);
        let mut s = PointStore::new();
        s.add(CogoPoint::new(1, 0.0, 0.0, None, "")).unwrap();
        let a = import_into(&mut s, r.points, DuplicatePolicy::RenumberToFree).unwrap();
        assert_eq!(a.renumbered, vec![(1, 3)]);
    }

    #[test]
    fn detect_pnezd_comma() {
        let d = detect_format(UTM, 2);
        assert_eq!(d.delimiter, Delimiter::Comma);
        assert_eq!(d.best.as_ref().unwrap().order, ColumnOrder::Penzd);
        assert!(d.confidence > 0.5, "{d:?}");
        assert_eq!(d.preview.len(), 2);
        assert_eq!(d.preview[0].northing, Some(9999999.1234));
    }

    #[test]
    fn detect_tab_enz_and_space() {
        let t = "100.5\t200.5\t3\n101.5\t201.5\t4\n102.5\t203.5\t5\n";
        let d = detect_format(t, 5);
        assert_eq!(d.delimiter, Delimiter::Tab);
        assert!(!d.best.as_ref().unwrap().order.has_point());
        let t = "1 500000 9000000 10 TR\n2 500010 9000020 11 EP\n3 500020 9000040 12 CL\n";
        let d = detect_format(t, 5);
        assert_eq!(d.delimiter, Delimiter::Space);
        assert_eq!(d.best.as_ref().unwrap().order, ColumnOrder::Penzd);
    }

    #[test]
    fn detect_garbage_and_empty() {
        let d = detect_format("", 3);
        assert!(d.best.is_none());
        let d = detect_format("hello world\nfoo bar baz\n", 3);
        assert!(d.confidence < 0.3, "{d:?}");
    }
}
