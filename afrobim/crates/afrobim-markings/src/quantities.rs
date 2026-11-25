// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Take-off for road markings, by geometry only.
//!
//! Every figure is the layout of [`crate::layout`] measured in closed form: lengths along each
//! line (station difference plus offset × change of heading), areas as painted length × width.
//! Nothing comes from the drawn chords or the display solids, and there are no rates or costs.

use afrobim_kernel::{Element, Note, Severity};

use crate::catalogue::MarkingColour;
use crate::definition::MarkingDefinition;
use crate::module::CATEGORY;
use crate::MarkingError;

/// One marking measured.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkingQuantities {
    pub mark: String,
    /// The catalogue identifier, e.g. `"1008.1"` or `"1013.1D-L"`.
    pub line_type: String,
    /// TSRGD diagram number.
    pub diagram: String,
    pub description: String,
    /// Width of the line; of each line for a double line.
    pub width_mm: u32,
    pub colour: MarkingColour,
    pub material: String,
    pub alignment: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub offset_m: f64,
    pub speed_limit_mph: u32,
    /// Length of the marking's reference line between its stations (for a double line, the
    /// middle of the pair): station difference + offset × change of heading.
    pub overall_length_m: f64,
    /// Whole marks. A continuous line is one mark; a double line counts both of its lines.
    pub marks: usize,
    /// Marks cut by the end of the length. Their length is in `painted_length_m`.
    pub part_marks: usize,
    /// Sum of the lengths of all marks, whole and part, of every line.
    pub painted_length_m: f64,
    /// Painted length × width.
    pub painted_area_m2: f64,
}

impl MarkingQuantities {
    /// Measures one marking. Fails when the definition has an Error note.
    pub fn measure(mark: &str, def: &MarkingDefinition) -> Result<MarkingQuantities, MarkingError> {
        let (laid, notes) = def.lay_out();
        let Some((resolved, layout)) = laid else {
            return Err(MarkingError::InvalidDefinition(error_text(&notes)));
        };
        Ok(MarkingQuantities {
            mark: mark.to_string(),
            line_type: resolved.spec.id.to_string(),
            diagram: resolved.spec.diagram.to_string(),
            description: resolved.spec.name.to_string(),
            width_mm: resolved.width_mm,
            colour: def.colour,
            material: def.material.clone(),
            alignment: def.alignment.clone(),
            start_station_m: def.start_station_m,
            end_station_m: def.end_station_m,
            offset_m: def.offset_m,
            speed_limit_mph: def.speed_limit_mph,
            overall_length_m: layout.overall_length_m,
            marks: layout.whole_marks(),
            part_marks: layout.part_marks(),
            painted_length_m: layout.painted_length_m(),
            painted_area_m2: layout.painted_area_m2(),
        })
    }
}

fn error_text(notes: &[Note]) -> String {
    let errors: Vec<&str> = notes.iter().filter(|n| n.severity == Severity::Error).map(|n| n.message.as_str()).collect();
    if errors.is_empty() {
        "the marking cannot be laid out".to_string()
    } else {
        errors.join(" ")
    }
}

/// A marking element that could not be measured, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedMarking {
    pub mark: String,
    pub reason: String,
}

/// The markings of one line type, width, colour and material, added up.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkingGroup {
    pub line_type: String,
    pub diagram: String,
    pub description: String,
    pub width_mm: u32,
    pub colour: MarkingColour,
    pub material: String,
    /// Number of marking elements in the group.
    pub markings: usize,
    pub overall_length_m: f64,
    pub marks: usize,
    pub part_marks: usize,
    pub painted_length_m: f64,
    pub painted_area_m2: f64,
}

/// The take-off of a set of elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarkingTakeoff {
    /// One row per marking, in document order.
    pub rows: Vec<MarkingQuantities>,
    /// Marking elements left out because their definition is invalid. Never dropped silently:
    /// the summary CSV counts them.
    pub skipped: Vec<SkippedMarking>,
}

impl MarkingTakeoff {
    /// Measures every element of category `"marking"`; other elements are ignored.
    pub fn measure(elements: &[Element]) -> MarkingTakeoff {
        let mut out = MarkingTakeoff::default();
        for e in elements.iter().filter(|e| e.category == CATEGORY) {
            match MarkingDefinition::from_value(&e.data).and_then(|def| MarkingQuantities::measure(&e.name, &def)) {
                Ok(row) => out.rows.push(row),
                Err(reason) => out.skipped.push(SkippedMarking { mark: e.name.clone(), reason: reason.to_string() }),
            }
        }
        out
    }

    /// Totals by line type, width, colour and material, ordered by line type (catalogue
    /// order), then width, colour and material.
    pub fn groups(&self) -> Vec<MarkingGroup> {
        let mut groups: Vec<MarkingGroup> = Vec::new();
        for r in &self.rows {
            let found = groups.iter_mut().find(|g| g.line_type == r.line_type && g.width_mm == r.width_mm && g.colour == r.colour && g.material == r.material);
            let g = match found {
                Some(g) => g,
                None => {
                    groups.push(MarkingGroup {
                        line_type: r.line_type.clone(),
                        diagram: r.diagram.clone(),
                        description: r.description.clone(),
                        width_mm: r.width_mm,
                        colour: r.colour,
                        material: r.material.clone(),
                        markings: 0,
                        overall_length_m: 0.0,
                        marks: 0,
                        part_marks: 0,
                        painted_length_m: 0.0,
                        painted_area_m2: 0.0,
                    });
                    groups.last_mut().expect("just pushed")
                }
            };
            g.markings += 1;
            g.overall_length_m += r.overall_length_m;
            g.marks += r.marks;
            g.part_marks += r.part_marks;
            g.painted_length_m += r.painted_length_m;
            g.painted_area_m2 += r.painted_area_m2;
        }
        let order = |id: &str| crate::catalogue::LineType::from_id(id).map_or(usize::MAX, |t| crate::catalogue::LineType::ALL.iter().position(|x| *x == t).unwrap_or(usize::MAX));
        groups.sort_by(|a, b| (order(&a.line_type), a.width_mm, a.colour, &a.material).cmp(&(order(&b.line_type), b.width_mm, b.colour, &b.material)));
        groups
    }

    pub fn overall_length_m(&self) -> f64 {
        self.rows.iter().map(|r| r.overall_length_m).sum()
    }

    pub fn marks(&self) -> usize {
        self.rows.iter().map(|r| r.marks).sum()
    }

    pub fn part_marks(&self) -> usize {
        self.rows.iter().map(|r| r.part_marks).sum()
    }

    pub fn painted_length_m(&self) -> f64 {
        self.rows.iter().map(|r| r.painted_length_m).sum()
    }

    pub fn painted_area_m2(&self) -> f64 {
        self.rows.iter().map(|r| r.painted_area_m2).sum()
    }
}

pub const MARKINGS_CSV_HEADER: &str = "mark,line_type,diagram,description,width_mm,colour,material,alignment,start_station_m,end_station_m,offset_m,speed_limit_mph,overall_length_m,marks,part_marks,painted_length_m,painted_area_m2";

/// Marking schedule, one row per marking, then a `TOTAL` row carrying the number of markings in
/// the second column and the summed lengths, marks and area. Lines end with `\n`.
///
/// Rounding: stations, offsets, lengths and areas to 3 decimals. Totals are sums of the
/// unrounded values, rounded once.
pub fn markings_to_csv(takeoff: &MarkingTakeoff) -> String {
    let mut out = String::from(MARKINGS_CSV_HEADER);
    out.push('\n');
    for r in &takeoff.rows {
        let fields = [
            csv(&r.mark),
            csv(&r.line_type),
            csv(&r.diagram),
            csv(&r.description),
            r.width_mm.to_string(),
            r.colour.label().to_string(),
            csv(&r.material),
            csv(&r.alignment),
            fixed(r.start_station_m, 3, 3),
            fixed(r.end_station_m, 3, 3),
            fixed(r.offset_m, 3, 3),
            r.speed_limit_mph.to_string(),
            fixed(r.overall_length_m, 3, 3),
            r.marks.to_string(),
            r.part_marks.to_string(),
            fixed(r.painted_length_m, 3, 3),
            fixed(r.painted_area_m2, 3, 3),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str(&format!(
        "TOTAL,{},,,,,,,,,,,{},{},{},{},{}\n",
        takeoff.rows.len(),
        fixed(takeoff.overall_length_m(), 3, 3),
        takeoff.marks(),
        takeoff.part_marks(),
        fixed(takeoff.painted_length_m(), 3, 3),
        fixed(takeoff.painted_area_m2(), 3, 3)
    ));
    out
}

pub const SUMMARY_CSV_HEADER: &str = "line_type,diagram,description,width_mm,colour,material,markings,overall_length_m,marks,part_marks,painted_length_m,painted_area_m2";

/// Totals by line type, width, colour and material (see [`MarkingTakeoff::groups`]), then a
/// `TOTAL` row, then a `NOT MEASURED` row counting the skipped elements when there are any.
/// Lines end with `\n`. Rounding as [`markings_to_csv`].
pub fn summary_to_csv(takeoff: &MarkingTakeoff) -> String {
    let mut out = String::from(SUMMARY_CSV_HEADER);
    out.push('\n');
    for g in takeoff.groups() {
        let fields = [
            csv(&g.line_type),
            csv(&g.diagram),
            csv(&g.description),
            g.width_mm.to_string(),
            g.colour.label().to_string(),
            csv(&g.material),
            g.markings.to_string(),
            fixed(g.overall_length_m, 3, 3),
            g.marks.to_string(),
            g.part_marks.to_string(),
            fixed(g.painted_length_m, 3, 3),
            fixed(g.painted_area_m2, 3, 3),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str(&format!(
        "TOTAL,,,,,,{},{},{},{},{},{}\n",
        takeoff.rows.len(),
        fixed(takeoff.overall_length_m(), 3, 3),
        takeoff.marks(),
        takeoff.part_marks(),
        fixed(takeoff.painted_length_m(), 3, 3),
        fixed(takeoff.painted_area_m2(), 3, 3)
    ));
    if !takeoff.skipped.is_empty() {
        out.push_str(&format!("NOT MEASURED (invalid definition),,,,,,{},,,,,\n", takeoff.skipped.len()));
    }
    out
}

/// `v` rounded to `max_decimals`, with trailing zeros removed down to `min_decimals`.
pub(crate) fn fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    let mut s = format!("{v:.max_decimals$}");
    if let Some(dot) = s.find('.') {
        while s.len() > dot + 1 + min_decimals && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.starts_with('-') && s.chars().all(|c| matches!(c, '-' | '0' | '.')) {
        s.remove(0);
    }
    s
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::LineType;
    use crate::module::{add_marking, LAYER};
    use afrobim_alignment::{AlignmentBuilder, Turn};
    use afrobim_kernel::{Document, ElementId, Pt};

    /// 100 m east from the origin, then 200 m of left-hand curve of radius 400 m.
    fn document() -> (Document, ElementId) {
        let alignment = AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(100.0).arc(400.0, 200.0, Turn::Left).build().unwrap();
        let mut doc = Document::default();
        let id = doc.add(afrobim_alignment::CATEGORY, "A1", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&alignment).unwrap());
        (doc, id)
    }

    fn populated() -> Document {
        let (mut doc, a) = document();
        // Centre line 1008.1 (3 m mark, 6 m gap), 100 mm, on the alignment, 0 to 300: line
        // length 300 m, marks lead at 0, 9, ..., 297; the last is whole (ends at 300).
        add_marking(&mut doc, a, "RM-001", &MarkingDefinition::new(LineType::Centre1008_1, 0.0, 300.0, 0.0, 60).with_width(100).with_material("thermoplastic")).unwrap();
        // Edge line 1012.1, 150 mm, 3.65 m right (outside of the left-hand curve), 0 to 300:
        // 100 + 200 x 403.65 / 400 = 301.825 m.
        add_marking(&mut doc, a, "RM-002", &MarkingDefinition::new(LineType::Edge1012_1, 0.0, 300.0, 3.65, 60).with_width(150).with_material("thermoplastic")).unwrap();
        // The same on the left (inside): 100 + 200 x 396.35 / 400 = 298.175 m.
        add_marking(&mut doc, a, "RM-003", &MarkingDefinition::new(LineType::Edge1012_1, 0.0, 300.0, -3.65, 60).with_width(150).with_material("thermoplastic")).unwrap();
        // A second length of centre line, 0 to 50 on the straight: marks lead at 0, 9, ..., 45;
        // the sixth is whole (45 to 48).
        add_marking(&mut doc, a, "RM-004", &MarkingDefinition::new(LineType::Centre1008_1, 0.0, 50.0, 0.0, 60).with_width(100).with_material("thermoplastic")).unwrap();
        // Warning line 1004.1 (6 m mark, 3 m gap), 100 mm, 100 to 140 on the curve at offset 0:
        // marks lead at 0, 9, 18, 27, 36; the fifth is cut at 40 m, 4 m long.
        add_marking(&mut doc, a, "RM-005", &MarkingDefinition::new(LineType::Warning1004_1, 100.0, 140.0, 0.0, 60).with_width(100).with_material("paint, \"type B\"")).unwrap();
        doc
    }

    #[test]
    fn each_marking_is_measured_in_closed_form() {
        let doc = populated();
        let q = MarkingTakeoff::measure(doc.elements());
        assert!(q.skipped.is_empty());
        assert_eq!(q.rows.len(), 5);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;

        let centre = &q.rows[0];
        assert_eq!((centre.line_type.as_str(), centre.diagram.as_str(), centre.width_mm), ("1008.1", "1008.1", 100));
        assert!(close(centre.overall_length_m, 300.0));
        assert_eq!((centre.marks, centre.part_marks), (34, 0));
        assert!(close(centre.painted_length_m, 102.0) && close(centre.painted_area_m2, 10.2));

        let outside = &q.rows[1];
        assert!(close(outside.overall_length_m, 301.825));
        assert_eq!((outside.marks, outside.part_marks), (1, 0));
        assert!(close(outside.painted_length_m, 301.825) && close(outside.painted_area_m2, 301.825 * 0.15));

        let inside = &q.rows[2];
        assert!(close(inside.overall_length_m, 298.175) && close(inside.painted_area_m2, 298.175 * 0.15));

        let short = &q.rows[3];
        assert_eq!((short.marks, short.part_marks), (6, 0));
        assert!(close(short.overall_length_m, 50.0) && close(short.painted_length_m, 18.0));

        let warning = &q.rows[4];
        assert_eq!((warning.marks, warning.part_marks), (4, 1));
        assert!(close(warning.painted_length_m, 28.0) && close(warning.painted_area_m2, 2.8));
    }

    #[test]
    fn totals_are_by_line_type_width_and_material() {
        let q = MarkingTakeoff::measure(populated().elements());
        let groups = q.groups();
        assert_eq!(groups.iter().map(|g| (g.line_type.as_str(), g.width_mm, g.markings)).collect::<Vec<_>>(), [("1008.1", 100, 2), ("1004.1", 100, 1), ("1012.1", 150, 2)]);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(groups[0].overall_length_m, 350.0) && close(groups[0].painted_length_m, 120.0) && close(groups[0].painted_area_m2, 12.0));
        assert_eq!((groups[0].marks, groups[0].part_marks), (40, 0));
        // The two edge lines together are exactly twice the centre length: the curve cancels.
        assert!(close(groups[2].overall_length_m, 600.0) && close(groups[2].painted_area_m2, 90.0));
        assert!(close(q.overall_length_m(), 990.0));
        assert_eq!((q.marks(), q.part_marks()), (46, 1));
        assert!(close(q.painted_length_m(), 748.0) && close(q.painted_area_m2(), 104.8));
    }

    #[test]
    fn the_csv_schedules() {
        let q = MarkingTakeoff::measure(populated().elements());
        let schedule = markings_to_csv(&q);
        let lines: Vec<&str> = schedule.lines().collect();
        assert_eq!(lines[0], MARKINGS_CSV_HEADER);
        assert_eq!(lines[1], "RM-001,1008.1,1008.1,Centre line,100,white,thermoplastic,A1,0.000,300.000,0.000,60,300.000,34,0,102.000,10.200");
        assert_eq!(lines[2], "RM-002,1012.1,1012.1,Edge of carriageway line (continuous),150,white,thermoplastic,A1,0.000,300.000,3.650,60,301.825,1,0,301.825,45.274");
        assert_eq!(lines[5], "RM-005,1004.1,1004.1,Warning line,100,white,\"paint, \"\"type B\"\"\",A1,100.000,140.000,0.000,60,40.000,4,1,28.000,2.800");
        assert_eq!(lines[6], "TOTAL,5,,,,,,,,,,,990.000,46,1,748.000,104.800");
        assert_eq!(lines.len(), 7);
        assert!(schedule.ends_with('\n'));
        assert!(lines.iter().all(|l| l.split(',').count() >= MARKINGS_CSV_HEADER.split(',').count()));

        let summary = summary_to_csv(&q);
        let lines: Vec<&str> = summary.lines().collect();
        assert_eq!(lines[0], SUMMARY_CSV_HEADER);
        assert_eq!(lines[1], "1008.1,1008.1,Centre line,100,white,thermoplastic,2,350.000,40,0,120.000,12.000");
        assert_eq!(lines[2], "1004.1,1004.1,Warning line,100,white,\"paint, \"\"type B\"\"\",1,40.000,4,1,28.000,2.800");
        assert_eq!(lines[3], "1012.1,1012.1,Edge of carriageway line (continuous),150,white,thermoplastic,2,600.000,2,0,600.000,90.000");
        assert_eq!(lines[4], "TOTAL,,,,,,5,990.000,46,1,748.000,104.800");
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn double_lines_count_both_lines() {
        let (mut doc, a) = document();
        add_marking(&mut doc, a, "DWL-1", &MarkingDefinition::new(LineType::Double1013_1BrokenLeft, 0.0, 90.0, 0.0, 60)).unwrap();
        let q = MarkingTakeoff::measure(doc.elements());
        let r = &q.rows[0];
        assert_eq!((r.line_type.as_str(), r.diagram.as_str(), r.width_mm), ("1013.1D-L", "1013.1", 150));
        // Left line: 1 m marks at 6 m centres over 90 m = 15. Right line: one continuous 90 m.
        assert_eq!((r.marks, r.part_marks), (16, 0));
        assert!((r.overall_length_m - 90.0).abs() < 1e-9);
        assert!((r.painted_length_m - 105.0).abs() < 1e-9 && (r.painted_area_m2 - 15.75).abs() < 1e-9);
    }

    #[test]
    fn invalid_markings_are_skipped_and_reported_not_silently_dropped() {
        let mut doc = populated();
        let mut bad = MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30);
        bad.line_type = "1099".into();
        doc.add(CATEGORY, "RM-BAD", Pt::new(0.0, 0.0), 1.0, 1.0, LAYER, bad.to_value());
        doc.add(CATEGORY, "RM-JUNK", Pt::new(0.0, 0.0), 1.0, 1.0, LAYER, serde_json::json!({"nonsense": true}));
        let q = MarkingTakeoff::measure(doc.elements());
        assert_eq!(q.rows.len(), 5);
        assert_eq!(q.skipped.iter().map(|s| s.mark.as_str()).collect::<Vec<_>>(), ["RM-BAD", "RM-JUNK"]);
        assert!(q.skipped[0].reason.contains("'1099' is not a line type"));
        assert!(summary_to_csv(&q).ends_with("NOT MEASURED (invalid definition),,,,,,2,,,,,\n"));
        // The alignment element itself is not a marking and is not counted either way.
        assert_eq!(MarkingTakeoff::measure(&doc.elements()[..1]), MarkingTakeoff::default());
    }

    #[test]
    fn numbers_are_written_to_a_fixed_number_of_decimals() {
        assert_eq!(fixed(301.825, 3, 3), "301.825");
        assert_eq!(fixed(2.0, 3, 3), "2.000");
        assert_eq!(fixed(-0.0001, 3, 3), "0.000");
        assert_eq!(fixed(3.650, 0, 3), "3.65");
    }
}
