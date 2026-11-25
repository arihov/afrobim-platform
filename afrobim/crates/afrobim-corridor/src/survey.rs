// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A ground surface from a file of survey points: reading the file, and triangulating the points.
//!
//! # The file
//!
//! A text file with one point a line, as total stations, GNSS controllers and Civil 3D write
//! them: an optional point number, two plan coordinates, a level and an optional description (the
//! field code). The order of the columns is **not** guessed; the caller says which it is
//! ([`ColumnOrder`]). Surveyors' files most often put the northing before the easting (`PNEZD`),
//! and reading one as easting first mirrors the survey about the diagonal.
//!
//! - Delimiter: the comma, the semicolon or the tab, whichever the first lines use; otherwise
//!   blanks. A field may be in double quotes.
//! - A byte order mark and blank lines are skipped. `\r\n` and `\n` line ends both read.
//! - A first line whose coordinates are not numbers is taken as a heading. Any other line that
//!   cannot be read is left out and reported with its line number.
//! - Numbers use the point as the decimal mark. Coordinates and levels are multiplied by
//!   [`SurveyFormat::unit_m`] (1 for metres).
//! - Everything after the level is the description, joined with a blank when the file splits it.
//!
//! # The surface
//!
//! [`build_survey_surface`] makes a TIN from the points:
//!
//! 1. Points whose description is on the caller's list are left out ([`SurfaceOptions::exclude`]).
//!    A survey holds levels that are not ground: culvert inverts, tops of walls, pipe soffits.
//!    Which codes those are is the surveyor's convention, so the list is the engineer's and
//!    nothing is left out unless asked.
//! 2. Points at the same plan position (to the millimetre) are merged: the first one read is
//!    kept. The number merged and the largest difference in level among them are reported.
//! 3. The points are triangulated by Delaunay's rule (the `delaunator` crate, which uses exact
//!    predicates, so strings of points on one line and corners on one circle are safe).
//! 4. Triangles with a side longer than [`SurfaceOptions::max_side_m`] are removed. A Delaunay
//!    triangulation fills the convex hull of the points, so across the inside of a bend, or
//!    between two roads, it spans ground nobody measured; the limit takes those out. No limit is
//!    applied unless one is given, and the right value depends on how densely the ground was shot.
//! 5. Points no triangle uses are dropped from the surface.
//!
//! **Not done:** breaklines. Every triangle side follows Delaunay's rule only, so a string of
//! points along a kerb, a channel or a top of bank is not forced to be a triangle side, and the
//! surface can cut across it. Compare sections against the survey before using volumes.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::tin::{Tin, TinError};

/// Which column is which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnOrder {
    /// Point, northing, easting, level, description.
    Pnezd,
    /// Point, easting, northing, level, description.
    Penzd,
    /// Northing, easting, level, description (no point number).
    Nezd,
    /// Easting, northing, level, description (no point number).
    Enzd,
}

impl ColumnOrder {
    pub const ALL: [ColumnOrder; 4] = [ColumnOrder::Pnezd, ColumnOrder::Penzd, ColumnOrder::Nezd, ColumnOrder::Enzd];

    /// `PNEZD` and so on.
    pub fn code(self) -> &'static str {
        match self {
            ColumnOrder::Pnezd => "PNEZD",
            ColumnOrder::Penzd => "PENZD",
            ColumnOrder::Nezd => "NEZD",
            ColumnOrder::Enzd => "ENZD",
        }
    }

    /// The columns in words.
    pub fn label(self) -> &'static str {
        match self {
            ColumnOrder::Pnezd => "Point, Northing, Easting, Level, Description",
            ColumnOrder::Penzd => "Point, Easting, Northing, Level, Description",
            ColumnOrder::Nezd => "Northing, Easting, Level, Description",
            ColumnOrder::Enzd => "Easting, Northing, Level, Description",
        }
    }

    /// From `PNEZD`, `penzd`, `NEZ`, `ENZ` … (the trailing `D` may be left off).
    pub fn parse(text: &str) -> Option<ColumnOrder> {
        match text.trim().to_ascii_uppercase().as_str() {
            "PNEZD" | "PNEZ" => Some(ColumnOrder::Pnezd),
            "PENZD" | "PENZ" => Some(ColumnOrder::Penzd),
            "NEZD" | "NEZ" => Some(ColumnOrder::Nezd),
            "ENZD" | "ENZ" => Some(ColumnOrder::Enzd),
            _ => None,
        }
    }

    fn has_point(self) -> bool {
        matches!(self, ColumnOrder::Pnezd | ColumnOrder::Penzd)
    }

    fn northing_first(self) -> bool {
        matches!(self, ColumnOrder::Pnezd | ColumnOrder::Nezd)
    }
}

/// How a point file is laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurveyFormat {
    pub order: ColumnOrder,
    /// Metres in one unit of the file's coordinates and levels (1 for metres, 0.3048 for feet).
    pub unit_m: f64,
}

impl SurveyFormat {
    /// The given order, in metres.
    pub fn metres(order: ColumnOrder) -> SurveyFormat {
        SurveyFormat { order, unit_m: 1.0 }
    }
}

/// One point of a survey, in metres.
#[derive(Debug, Clone, PartialEq)]
pub struct SurveyPoint {
    /// The point number or name; empty when the file has none.
    pub id: String,
    pub easting: f64,
    pub northing: f64,
    pub level: f64,
    /// The description (field code); empty when the file has none.
    pub code: String,
    /// The line of the file, from 1.
    pub line: usize,
}

/// The points read from a file, and what the reader has to say.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurveyPoints {
    pub points: Vec<SurveyPoint>,
    pub notes: Vec<String>,
}

impl SurveyPoints {
    /// The descriptions in the file and how many points carry each, most frequent first (then by
    /// name). Descriptions that differ only in case are counted together, under the spelling met
    /// first.
    pub fn codes(&self) -> Vec<(String, usize)> {
        let mut seen: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for p in &self.points {
            let entry = seen.entry(p.code.to_ascii_uppercase()).or_insert_with(|| (p.code.clone(), 0));
            entry.1 += 1;
        }
        let mut out: Vec<(String, usize)> = seen.into_values().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_ascii_uppercase().cmp(&b.0.to_ascii_uppercase())));
        out
    }

    /// South-west and north-east corners `(easting, northing)` of the points, in metres.
    pub fn plan_extent(&self) -> Option<((f64, f64), (f64, f64))> {
        let first = self.points.first()?;
        let mut min = (first.easting, first.northing);
        let mut max = min;
        for p in &self.points {
            min = (min.0.min(p.easting), min.1.min(p.northing));
            max = (max.0.max(p.easting), max.1.max(p.northing));
        }
        Some((min, max))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SurveyError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("the unit must be a positive number of metres, not {0}")]
    Unit(f64),
    #[error("no point could be read from the file{0}")]
    NoPoints(String),
    #[error("a surface needs at least three points that are not on one line; {0} point(s) are left after the exclusions and merging")]
    TooFew(usize),
    #[error("no triangle is left: every one has a side longer than the limit of {0} m")]
    AllTooLong(f64),
    #[error("the triangulation could not be used: {0}")]
    Tin(#[from] TinError),
}

fn split_line(line: &str, delimiter: Option<char>) -> Vec<&str> {
    fn unquote(f: &str) -> &str {
        let f = f.trim();
        f.strip_prefix('"').and_then(|g| g.strip_suffix('"')).unwrap_or(f).trim()
    }
    match delimiter {
        Some(d) => line.split(d).map(unquote).collect(),
        None => line.split_whitespace().map(unquote).collect(),
    }
}

fn number(field: Option<&&str>) -> Option<f64> {
    field?.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Reads a point file held in a string. See the module documentation.
pub fn parse_survey_points(text: &str, format: &SurveyFormat) -> Result<SurveyPoints, SurveyError> {
    if !(format.unit_m.is_finite() && format.unit_m > 0.0) {
        return Err(SurveyError::Unit(format.unit_m));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delimiter = {
        let sample: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(20).collect();
        [',', ';', '\t'].into_iter().find(|d| !sample.is_empty() && sample.iter().all(|l| l.contains(*d)))
    };
    let first = usize::from(format.order.has_point());
    let (northing_at, easting_at) = if format.order.northing_first() { (first, first + 1) } else { (first + 1, first) };

    let mut out = SurveyPoints::default();
    let mut unreadable: Vec<usize> = Vec::new();
    let mut seen_any_line = false;
    for (index, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let line = index + 1;
        let fields = split_line(raw, delimiter);
        let read = (number(fields.get(northing_at)), number(fields.get(easting_at)), number(fields.get(first + 2)));
        let heading = !seen_any_line;
        seen_any_line = true;
        let (Some(northing), Some(easting), Some(level)) = read else {
            if heading {
                out.notes.push(format!("Line {line} was taken as a heading: its coordinates are not numbers."));
            } else {
                unreadable.push(line);
            }
            continue;
        };
        out.points.push(SurveyPoint {
            id: if format.order.has_point() { fields[0].to_string() } else { String::new() },
            easting: easting * format.unit_m,
            northing: northing * format.unit_m,
            level: level * format.unit_m,
            code: fields.get(first + 3..).map(|rest| rest.join(" ")).unwrap_or_default().trim().to_string(),
            line,
        });
    }
    if !unreadable.is_empty() {
        let shown: Vec<String> = unreadable.iter().take(8).map(usize::to_string).collect();
        out.notes.push(format!(
            "{} line(s) were left out because they do not hold a northing, an easting and a level as numbers in the order {}: line {}{}.",
            unreadable.len(),
            format.order.code(),
            shown.join(", "),
            if unreadable.len() > shown.len() { " and others" } else { "" }
        ));
    }
    if out.points.is_empty() {
        let why = if unreadable.is_empty() { String::new() } else { format!(" (is the column order {} right?)", format.order.code()) };
        return Err(SurveyError::NoPoints(why));
    }
    Ok(out)
}

/// Reads a point file. The file must be UTF-8 or plain ASCII; other bytes are replaced, which
/// only ever touches descriptions.
pub fn read_survey_points(path: &Path, format: &SurveyFormat) -> Result<SurveyPoints, SurveyError> {
    parse_survey_points(&String::from_utf8_lossy(&std::fs::read(path)?), format)
}

/// What the engineer decides about the surface. Nothing here has a built-in value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurfaceOptions {
    /// Descriptions whose points are left out. Compared without regard to case; a trailing `*`
    /// matches any ending (`CULV-*`). An empty entry matches points with no description.
    pub exclude: Vec<String>,
    /// The longest side a triangle may have, in metres. `None` keeps every triangle.
    pub max_side_m: Option<f64>,
}

impl SurfaceOptions {
    /// Whether a description is on the exclusion list.
    pub fn excludes(&self, code: &str) -> bool {
        let code = code.trim().to_ascii_uppercase();
        self.exclude.iter().any(|pattern| {
            let pattern = pattern.trim().to_ascii_uppercase();
            match pattern.strip_suffix('*') {
                Some(prefix) => code.starts_with(prefix),
                None => code == pattern,
            }
        })
    }
}

/// A surface made from survey points, with the account of how.
#[derive(Debug, Clone, PartialEq)]
pub struct SurveySurface {
    pub tin: Tin,
    /// Points in the file.
    pub points_read: usize,
    /// Points left out by description.
    pub points_excluded: usize,
    /// Points merged into another at the same plan position.
    pub points_merged: usize,
    /// Points left that no triangle uses (after the side limit).
    pub points_unused: usize,
    /// Triangles removed by the side limit.
    pub triangles_too_long: usize,
    pub notes: Vec<String>,
}

/// Twice the signed plan area, as [`Tin::new`] tests it.
fn area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Builds the surface of a set of points. See the module documentation for the steps.
pub fn build_survey_surface(survey: &SurveyPoints, options: &SurfaceOptions) -> Result<SurveySurface, SurveyError> {
    let mut notes = Vec::new();

    // 1. Exclusions.
    let mut excluded: BTreeMap<String, usize> = BTreeMap::new();
    let kept: Vec<&SurveyPoint> = survey
        .points
        .iter()
        .filter(|p| {
            let out = options.excludes(&p.code);
            if out {
                *excluded.entry(p.code.to_ascii_uppercase()).or_default() += 1;
            }
            !out
        })
        .collect();
    let points_excluded = survey.points.len() - kept.len();
    if points_excluded > 0 {
        let list: Vec<String> = excluded.iter().map(|(code, n)| format!("{} ({n})", if code.is_empty() { "no description" } else { code })).collect();
        notes.push(format!("{points_excluded} point(s) were left out by description: {}.", list.join(", ")));
    }
    for pattern in &options.exclude {
        let probe = SurfaceOptions { exclude: vec![pattern.clone()], max_side_m: None };
        if !survey.points.iter().any(|p| probe.excludes(&p.code)) {
            notes.push(format!("No point has the description '{}', which was to be left out.", pattern.trim()));
        }
    }

    // 2. One point a plan position, to the millimetre.
    let mut at: HashMap<(i64, i64), usize> = HashMap::with_capacity(kept.len());
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(kept.len());
    let (mut points_merged, mut worst_level_gap) = (0usize, 0.0_f64);
    for p in &kept {
        let key = ((p.easting * 1000.0).round() as i64, (p.northing * 1000.0).round() as i64);
        match at.get(&key) {
            Some(&i) => {
                points_merged += 1;
                worst_level_gap = worst_level_gap.max((points[i][2] - p.level).abs());
            }
            None => {
                at.insert(key, points.len());
                points.push([p.easting, p.northing, p.level]);
            }
        }
    }
    if points_merged > 0 {
        notes.push(format!(
            "{points_merged} point(s) share a plan position with an earlier point and were merged into it (the first one read is kept). The largest difference in level among them is {worst_level_gap:.3} m."
        ));
    }
    if points.len() < 3 {
        return Err(SurveyError::TooFew(points.len()));
    }

    // 3. Delaunay, on coordinates measured from the south-west corner.
    let (x0, y0) = points.iter().fold((f64::INFINITY, f64::INFINITY), |m, p| (m.0.min(p[0]), m.1.min(p[1])));
    let local: Vec<delaunator::Point> = points.iter().map(|p| delaunator::Point { x: p[0] - x0, y: p[1] - y0 }).collect();
    let found = delaunator::triangulate(&local);
    if found.triangles.is_empty() {
        return Err(SurveyError::TooFew(points.len()));
    }

    // 4. The side limit, and triangles with no area in plan.
    let limit2 = options.max_side_m.filter(|m| m.is_finite() && *m > 0.0).map(|m| m * m);
    let (mut too_long, mut flat) = (0usize, 0usize);
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(found.triangles.len() / 3);
    for t in found.triangles.chunks_exact(3) {
        let [a, b, c] = [points[t[0]], points[t[1]], points[t[2]]];
        if area2(a, b, c) == 0.0 {
            flat += 1;
            continue;
        }
        if let Some(limit2) = limit2 {
            let side2 = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
            if side2(a, b) > limit2 || side2(b, c) > limit2 || side2(c, a) > limit2 {
                too_long += 1;
                continue;
            }
        }
        triangles.push([t[0] as u32, t[1] as u32, t[2] as u32]);
    }
    if flat > 0 {
        notes.push(format!("{flat} triangle(s) with no area in plan were left out."));
    }
    match options.max_side_m {
        Some(limit) if limit2.is_some() => {
            if triangles.is_empty() {
                return Err(SurveyError::AllTooLong(limit));
            }
            notes.push(format!("{too_long} triangle(s) with a side longer than {limit} m were removed."));
        }
        _ => notes.push(
            "No limit was set on the length of a triangle side, so the surface fills the convex hull of the points, including ground that was not surveyed.".to_string(),
        ),
    }
    if triangles.is_empty() {
        return Err(SurveyError::TooFew(points.len()));
    }

    // 5. Only the points in use.
    let mut renumbered: Vec<u32> = vec![u32::MAX; points.len()];
    let mut used: Vec<[f64; 3]> = Vec::with_capacity(points.len());
    for corner in triangles.iter_mut().flatten() {
        let old = *corner as usize;
        if renumbered[old] == u32::MAX {
            renumbered[old] = used.len() as u32;
            used.push(points[old]);
        }
        *corner = renumbered[old];
    }
    let points_unused = points.len() - used.len();
    if points_unused > 0 {
        notes.push(format!("{points_unused} point(s) are in no triangle and are not part of the surface."));
    }
    notes.push("Breaklines are not applied: triangle sides follow Delaunay's rule only and may cross a kerb, channel or bank line.".to_string());

    Ok(SurveySurface {
        tin: Tin::new(used, triangles)?,
        points_read: survey.points.len(),
        points_excluded,
        points_merged,
        points_unused,
        triangles_too_long: too_long,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pnezd() -> SurveyFormat {
        SurveyFormat::metres(ColumnOrder::Pnezd)
    }

    #[test]
    fn a_pnezd_file_reads_with_northing_before_easting() {
        let text = "1,9887742.645,213761.191,1329.41,ACCL\r\n2,9887745.115,213758.683,1329.35,ACRE\r\n\r\n3,9887746.408,213757.415,1329.252,DRN Sp\r\n";
        let s = parse_survey_points(text, &pnezd()).unwrap();
        assert!(s.notes.is_empty(), "{:?}", s.notes);
        assert_eq!(s.points.len(), 3);
        let p = &s.points[0];
        assert_eq!((p.id.as_str(), p.easting, p.northing, p.level, p.code.as_str(), p.line), ("1", 213761.191, 9887742.645, 1329.41, "ACCL", 1));
        assert_eq!((s.points[2].code.as_str(), s.points[2].line), ("DRN Sp", 4));
        assert_eq!(s.plan_extent(), Some(((213757.415, 9887742.645), (213761.191, 9887746.408))));
    }

    #[test]
    fn each_column_order_puts_the_coordinates_where_it_says() {
        let p = |text: &str, order| parse_survey_points(text, &SurveyFormat::metres(order)).unwrap().points.remove(0);
        let a = p("7,100,200,5,CL", ColumnOrder::Pnezd);
        assert_eq!((a.id.as_str(), a.northing, a.easting, a.level), ("7", 100.0, 200.0, 5.0));
        let b = p("7,100,200,5,CL", ColumnOrder::Penzd);
        assert_eq!((b.id.as_str(), b.easting, b.northing), ("7", 100.0, 200.0));
        let c = p("100,200,5,CL", ColumnOrder::Nezd);
        assert_eq!((c.id.as_str(), c.northing, c.easting, c.level, c.code.as_str()), ("", 100.0, 200.0, 5.0, "CL"));
        let d = p("100,200,5", ColumnOrder::Enzd);
        assert_eq!((d.easting, d.northing, d.code.as_str()), (100.0, 200.0, ""));
        for order in ColumnOrder::ALL {
            assert_eq!(ColumnOrder::parse(order.code()), Some(order));
        }
        assert_eq!(ColumnOrder::parse(" enz "), Some(ColumnOrder::Enzd));
        assert_eq!(ColumnOrder::parse("XYZ"), None);
    }

    #[test]
    fn delimiters_headings_quotes_units_and_bad_lines() {
        // Semicolons, a heading, a byte order mark, quotes.
        let s = parse_survey_points("\u{feff}Pt;N;E;Z;Code\n1;10;20;3;\"TOP OF BANK\"\n", &pnezd()).unwrap();
        assert_eq!(s.notes, vec!["Line 1 was taken as a heading: its coordinates are not numbers.".to_string()]);
        assert_eq!((s.points[0].northing, s.points[0].code.as_str()), (10.0, "TOP OF BANK"));
        // Tabs.
        assert_eq!(parse_survey_points("1\t10\t20\t3\tCL\n", &pnezd()).unwrap().points[0].easting, 20.0);
        // Blanks, with a description of two words.
        let s = parse_survey_points("1 10 20 3 EDGE ROAD\n2 11 21 3\n", &pnezd()).unwrap();
        assert_eq!((s.points[0].code.as_str(), s.points[1].code.as_str()), ("EDGE ROAD", ""));
        // A comma inside a description stays in it.
        assert_eq!(parse_survey_points("1,10,20,3,WALL, TOP\n", &pnezd()).unwrap().points[0].code, "WALL TOP");
        // Feet.
        let s = parse_survey_points("1,100,200,10,CL\n", &SurveyFormat { order: ColumnOrder::Pnezd, unit_m: 0.3048 }).unwrap();
        assert!((s.points[0].northing - 30.48).abs() < 1e-12 && (s.points[0].level - 3.048).abs() < 1e-12);
        // Lines that cannot be read are reported, not guessed at.
        let s = parse_survey_points("1,10,20,3,CL\n2,ten,20,3,CL\n3,10,20\n4,11,21,3,CL\n5,1e999,2,3,CL\n", &pnezd()).unwrap();
        assert_eq!(s.points.len(), 2);
        assert!(s.notes[0].starts_with("3 line(s) were left out") && s.notes[0].ends_with("line 2, 3, 5."), "{:?}", s.notes);
        // Nothing readable: the order is questioned.
        let e = parse_survey_points("a,b,c\nd,e,f\n", &pnezd()).unwrap_err();
        assert!(e.to_string().contains("is the column order PNEZD right?"), "{e}");
        assert!(matches!(parse_survey_points("", &pnezd()), Err(SurveyError::NoPoints(_))));
        assert!(matches!(parse_survey_points("1,1,1,1", &SurveyFormat { order: ColumnOrder::Pnezd, unit_m: 0.0 }), Err(SurveyError::Unit(_))));
    }

    #[test]
    fn codes_are_counted_without_regard_to_case() {
        let s = parse_survey_points("1,0,0,1,SH\n2,0,1,1,sh\n3,1,0,1,CL\n4,1,1,1,SH\n5,2,2,1\n", &pnezd()).unwrap();
        assert_eq!(s.codes(), vec![("SH".to_string(), 3), ("".to_string(), 1), ("CL".to_string(), 1)]);
        let o = SurfaceOptions { exclude: vec!["culv-*".into(), "TR".into()], max_side_m: None };
        assert!(o.excludes("CULV-INV") && o.excludes("Culv-Top") && o.excludes("tr") && !o.excludes("TREE") && !o.excludes("CL") && !o.excludes(""));
        assert!(SurfaceOptions { exclude: vec!["".into()], max_side_m: None }.excludes(""));
    }

    /// A grid of `nx` by `ny` points one metre apart, the level a plane, as PNEZD text. The rows
    /// are sheared by an eighth of a metre each (exact in binary, so the edges stay straight) so
    /// that no four points lie on one circle and the triangulation is the only one Delaunay's
    /// rule allows.
    fn grid(nx: usize, ny: usize) -> SurveyPoints {
        let mut text = String::new();
        for j in 0..ny {
            for i in 0..nx {
                let (e, n) = (1000.0 + i as f64 + 0.125 * j as f64, 5000.0 + j as f64);
                text.push_str(&format!("{},{n},{e},{},SH\n", j * nx + i + 1, 100.0 + 0.5 * e + 0.25 * n));
            }
        }
        parse_survey_points(&text, &pnezd()).unwrap()
    }

    #[test]
    fn a_grid_becomes_the_triangles_euler_requires_and_carries_the_plane() {
        let s = build_survey_surface(&grid(6, 4), &SurfaceOptions::default()).unwrap();
        // A triangulation of n points with h on the hull has 2n - 2 - h triangles: 24 points, 16
        // on the hull, 30 triangles. Its area is that of the sheared grid: 5 by 3.
        assert_eq!((s.tin.points().len(), s.tin.triangles().len()), (24, 30));
        assert!((s.tin.plan_area() - 15.0).abs() < 1e-9, "{}", s.tin.plan_area());
        assert_eq!((s.points_read, s.points_excluded, s.points_merged, s.points_unused, s.triangles_too_long), (24, 0, 0, 0, 0));
        // The level anywhere inside is the plane the points lie on.
        for (e, n) in [(1002.3, 5001.7), (1000.4, 5000.2), (1004.9, 5002.9)] {
            let z = s.tin.elevation_at(e, n).unwrap();
            assert!((z - (100.0 + 0.5 * e + 0.25 * n)).abs() < 1e-9, "{z}");
        }
        assert!(s.notes.iter().any(|n| n.starts_with("No limit was set")), "{:?}", s.notes);
        assert!(s.notes.iter().any(|n| n.starts_with("Breaklines are not applied")), "{:?}", s.notes);
    }

    /// Delaunay's rule, tested the slow way: no point lies strictly inside the circle through the
    /// corners of any triangle.
    #[test]
    fn no_point_lies_inside_the_circle_of_a_triangle() {
        // 400 points from a fixed linear congruential sequence, on a 1 mm lattice like a survey.
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) % 200_000) as f64 / 1000.0
        };
        let mut text = String::new();
        for i in 0..400 {
            let (e, n) = (213_000.0 + next(), 9_887_000.0 + next());
            text.push_str(&format!("{i},{n:.3},{e:.3},{:.3},SH\n", 1300.0 + next() / 10.0));
        }
        let survey = parse_survey_points(&text, &pnezd()).unwrap();
        let s = build_survey_surface(&survey, &SurfaceOptions::default()).unwrap();
        let p = s.tin.points();
        assert_eq!(p.len() + s.points_merged, 400);
        // The in-circle determinant on coordinates from a local origin: positive when `q` is
        // inside the circle through a, b, c taken counter-clockwise. With sides up to 200 m its
        // terms reach 1.6e9, so rounding is far below the 0.001 allowed here.
        let (ox, oy) = (213_000.0, 9_887_000.0);
        for t in s.tin.triangles() {
            let [a, b, c] = t.map(|i| (p[i as usize][0] - ox, p[i as usize][1] - oy));
            let turn = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            for q in p {
                let q = (q[0] - ox, q[1] - oy);
                let row = |v: (f64, f64)| (v.0 - q.0, v.1 - q.1, (v.0 - q.0).powi(2) + (v.1 - q.1).powi(2));
                let (ra, rb, rc) = (row(a), row(b), row(c));
                let det = ra.0 * (rb.1 * rc.2 - rb.2 * rc.1) - ra.1 * (rb.0 * rc.2 - rb.2 * rc.0) + ra.2 * (rb.0 * rc.1 - rb.1 * rc.0);
                assert!(det * turn.signum() <= 1e-3, "a point lies inside the circle of a triangle (determinant {det})");
            }
        }
        // Every point is a corner of some triangle.
        assert_eq!(s.points_unused, 0);
    }

    #[test]
    fn exclusions_duplicates_and_the_side_limit_are_applied_and_reported() {
        // The grid, an invert well below it, a repeated point with another level, and one far-away point.
        let mut survey = grid(4, 4);
        let extra = parse_survey_points("900,5001.5,1001.6,50,CULV-INV\n901,5000,1000,1856.25,SH\n902,5001,1100,300,SH\n", &pnezd()).unwrap();
        survey.points.extend(extra.points);

        let all = build_survey_surface(&survey, &SurfaceOptions::default()).unwrap();
        assert_eq!((all.points_excluded, all.points_merged), (0, 1));
        assert!(all.tin.elevation_at(1001.6, 5001.5).unwrap() < 60.0, "the invert is in the surface when nothing is left out");
        assert!(all.notes.iter().any(|n| n.contains("1 point(s) share a plan position") && n.contains("is 6.250 m")), "{:?}", all.notes);

        let options = SurfaceOptions { exclude: vec!["culv-*".into(), "NOSUCH".into()], max_side_m: Some(5.0) };
        let s = build_survey_surface(&survey, &options).unwrap();
        assert_eq!((s.points_read, s.points_excluded, s.points_merged, s.points_unused), (19, 1, 1, 1));
        assert!(s.triangles_too_long > 0);
        // The grid alone is left: 16 points, 9 squares.
        assert_eq!((s.tin.points().len(), s.tin.triangles().len()), (16, 18));
        let z = s.tin.elevation_at(1001.6, 5001.5).unwrap();
        assert!((z - (100.0 + 0.5 * 1001.6 + 0.25 * 5001.5)).abs() < 1e-9, "{z}");
        assert!(s.tin.elevation_at(1050.0, 5001.0).is_none(), "nothing spans to the far point");
        let has = |start: &str| s.notes.iter().any(|n| n.starts_with(start));
        assert!(has("1 point(s) were left out by description: CULV-INV (1)."), "{:?}", s.notes);
        assert!(has("No point has the description 'NOSUCH'"), "{:?}", s.notes);
        assert!(has("1 point(s) are in no triangle"), "{:?}", s.notes);
        assert!(s.notes.iter().any(|n| n.contains("with a side longer than 5 m were removed")), "{:?}", s.notes);
    }

    #[test]
    fn too_few_points_points_on_one_line_and_an_impossible_limit_are_errors() {
        let two = parse_survey_points("1,0,0,1\n2,1,1,1\n", &pnezd()).unwrap();
        assert!(matches!(build_survey_surface(&two, &SurfaceOptions::default()), Err(SurveyError::TooFew(2))));
        let line = parse_survey_points("1,0,0,1\n2,1,1,1\n3,2,2,1\n4,3,3,1\n", &pnezd()).unwrap();
        assert!(matches!(build_survey_surface(&line, &SurfaceOptions::default()), Err(SurveyError::TooFew(4))));
        let e = build_survey_surface(&grid(3, 3), &SurfaceOptions { exclude: vec![], max_side_m: Some(0.5) }).unwrap_err();
        assert_eq!(e.to_string(), "no triangle is left: every one has a side longer than the limit of 0.5 m");
        let all_out = build_survey_surface(&grid(3, 3), &SurfaceOptions { exclude: vec!["SH".into()], max_side_m: None });
        assert!(matches!(all_out, Err(SurveyError::TooFew(0))));
    }
}
