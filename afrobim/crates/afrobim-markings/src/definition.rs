//! The marking element's definition and its checks against the manual.

use afrobim_alignment::Alignment;
use afrobim_kernel::{Note, Severity};
use serde::{Deserialize, Serialize};

use crate::catalogue::{applications_for, spec, speed_label, LineGroup, LineSpec, LineType, MarkingColour, Pattern, SpeedBand};
use crate::layout::{lay_out, Layout, LayoutError, PatternAnchor, StrandSpec};
use crate::MarkingError;

/// A longitudinal road marking: the `data` of an element of category [`crate::CATEGORY`].
///
/// The marking follows an alignment between two stations at a constant offset. The kernel hands
/// a module one element at a time, so the definition carries a copy of the alignment's geometry
/// (`geometry`) next to its name; [`crate::sync_markings`] refreshes the copy when the alignment
/// element changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkingDefinition {
    /// A [`LineType`] identifier, e.g. `"1008.1"`. Kept as text so that a document written by a
    /// later version, with a type this one does not know, still opens and says so.
    pub line_type: String,
    /// Name of the alignment element the marking follows.
    #[serde(default)]
    pub alignment: String,
    /// The alignment's geometry as it was when the marking was last synchronised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Alignment>,
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// Offset of the marking from the alignment, m, positive to the right looking up-station
    /// (the convention of `afrobim-alignment`). For a single line: its centre. For a double
    /// line: the middle of the gap between the two lines.
    #[serde(default)]
    pub offset_m: f64,
    /// The speed limit the line type and width are checked against, mph as in the manual.
    pub speed_limit_mph: u32,
    /// Width of the line, mm; of each line for a double line. `None` takes the width the
    /// manual's table gives for the speed limit, where it gives exactly one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_mm: Option<u32>,
    /// Double lines only: the clear gap between the two lines, mm. `None` is the manual's
    /// normal 175 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separation_mm: Option<u32>,
    #[serde(default)]
    pub colour: MarkingColour,
    /// Free text, never interpreted: e.g. "thermoplastic screed, reflectorised".
    #[serde(default)]
    pub material: String,
    /// Which end the pattern is laid from; see [`crate::layout`].
    #[serde(default)]
    pub anchor: PatternAnchor,
}

/// A definition resolved against the catalogue: what will actually be laid.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub spec: &'static LineSpec,
    /// Width of each line, mm.
    pub width_mm: u32,
    /// Double lines: the gap between the lines, mm.
    pub separation_mm: Option<u32>,
    pub strands: Vec<StrandSpec>,
}

impl MarkingDefinition {
    /// A white marking of a catalogue type with the table width, not yet tied to an alignment:
    /// [`crate::add_marking`] fills in `alignment` and `geometry`.
    pub fn new(line_type: LineType, start_station_m: f64, end_station_m: f64, offset_m: f64, speed_limit_mph: u32) -> Self {
        MarkingDefinition {
            line_type: line_type.id().to_string(),
            alignment: String::new(),
            geometry: None,
            start_station_m,
            end_station_m,
            offset_m,
            speed_limit_mph,
            width_mm: None,
            separation_mm: None,
            colour: spec(line_type).colour,
            material: String::new(),
            anchor: PatternAnchor::Start,
        }
    }

    pub fn with_width(mut self, width_mm: u32) -> Self {
        self.width_mm = Some(width_mm);
        self
    }

    pub fn with_material(mut self, material: &str) -> Self {
        self.material = material.to_string();
        self
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, MarkingError> {
        serde_json::from_value(data.clone()).map_err(|e| MarkingError::InvalidDefinition(format!("the marking definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a marking definition always serialises")
    }

    /// The catalogue type, or `None` when `line_type` is not one this version knows.
    pub fn line(&self) -> Option<LineType> {
        LineType::from_id(&self.line_type)
    }

    /// Checks the definition against the catalogue and the manual's tables.
    ///
    /// Returns what will be laid (`None` when an Error note prevents it) and the notes.
    /// Severity: **Error** where the marking cannot be laid or would not be the prescribed
    /// diagram (unknown type, a width not prescribed for the diagram, a double line outside the
    /// limits of 3.2.1); **Warning** where it can be laid but departs from the manual's table or
    /// the table had to be read for the user (a width the table does not give for that speed, a
    /// diagram the table gives for other speeds, an unchosen width where rows differ, a colour
    /// other than the prescribed one); **Info** for conditions the manual attaches to a row.
    pub fn resolve(&self) -> (Option<Resolved>, Vec<Note>) {
        let mut notes = Vec::new();
        let Some(line_type) = self.line() else {
            notes.push(note(
                Severity::Error,
                "marking.type.unknown",
                format!("'{}' is not a line type in the catalogue. The catalogue holds: {}.", self.line_type, LineType::ALL.map(LineType::id).join(", ")),
            ));
            return (None, notes);
        };
        let spec = spec(line_type);
        let speed = self.speed_limit_mph;
        if speed == 0 {
            notes.push(note(Severity::Error, "marking.speed.missing", "No speed limit is given; the manual selects the diagram and width by speed limit.".to_string()));
            return (None, notes);
        }
        if self.colour != spec.colour {
            notes.push(note(
                Severity::Warning,
                "marking.colour",
                format!(
                    "Diagram {} is {}; {} is set. Chapter 5 (2018) shows these lines white (Figures 2-1 to 2-6, 3-1) and gives yellow to waiting and loading restrictions only (12.5.3).",
                    spec.diagram,
                    spec.colour.label(),
                    self.colour.label()
                ),
            ));
        }
        let rows = applications_for(line_type, speed);
        let table = rows.first().map_or(spec.citation, |r| r.citation);

        // The diagram against the speed limit.
        if rows.is_empty() {
            if spec.speed == SpeedBand::Any {
                notes.push(note(
                    Severity::Warning,
                    "marking.speed.no_column",
                    format!(
                        "Chapter 5 (2018) Table 2-5 gives widths for diagram {} at 40 mph or less, 50 or 60 mph and 70 mph; it has no column for {}.",
                        spec.diagram,
                        speed_label(speed)
                    ),
                ));
            } else {
                notes.push(note(
                    Severity::Warning,
                    "marking.speed.diagram",
                    format!(
                        "The manual gives diagram {} for speed limits of {} ({}); the limit here is {}.",
                        spec.diagram,
                        spec.speed.label(),
                        spec.citation,
                        speed_label(speed)
                    ),
                ));
            }
        } else if rows.iter().all(|r| r.exception.is_some()) {
            let conditions: Vec<&str> = rows.iter().filter_map(|r| r.exception).collect();
            notes.push(note(
                Severity::Info,
                "marking.speed.exception",
                format!("Diagram {} at {} is for use only {}.", spec.diagram, speed_label(speed), conditions.join("; or ")),
            ));
        }

        // The width.
        let mut width_mm = self.width_mm;
        let mut separation_mm = None;
        let mut blocked = false;
        if let Some(double) = spec.double {
            let width = width_mm.unwrap_or(double.normal_width_mm);
            let separation = self.separation_mm.unwrap_or(double.normal_separation_mm);
            if width < double.min_width_mm {
                blocked = true;
                notes.push(note(
                    Severity::Error,
                    "marking.double.width",
                    format!("Each line of a double line must be at least {} mm wide (Chapter 5 (2018) para 3.2.1); {width} mm is set.", double.min_width_mm),
                ));
            } else if width != double.normal_width_mm {
                notes.push(note(
                    Severity::Info,
                    "marking.double.width_not_normal",
                    format!("{} mm should normally be used for each line of a double line (Chapter 5 (2018) para 3.2.1); {width} mm is set.", double.normal_width_mm),
                ));
            }
            if separation < double.min_separation_mm {
                blocked = true;
                notes.push(note(
                    Severity::Error,
                    "marking.double.separation",
                    format!(
                        "The two lines must be at least {} mm apart and are normally {} mm apart (Chapter 5 (2018) para 3.2.1); {separation} mm is set.",
                        double.min_separation_mm, double.normal_separation_mm
                    ),
                ));
            }
            let overall = 2 * width + separation;
            if overall > double.max_overall_mm {
                blocked = true;
                notes.push(note(
                    Severity::Error,
                    "marking.double.overall",
                    format!(
                        "Both lines and the gap between them must not exceed {} mm overall (Chapter 5 (2018) para 3.2.1, Figure 3-1 version A); {width} + {separation} + {width} = {overall} mm. The wider version B of diagram 1013.1 is not in this crate.",
                        double.max_overall_mm
                    ),
                ));
            }
            width_mm = Some(width);
            separation_mm = Some(separation);
        } else {
            if self.separation_mm.is_some() {
                notes.push(note(Severity::Info, "marking.separation.unused", format!("Diagram {} is a single line; the separation is not used.", spec.diagram)));
            }
            match width_mm {
                Some(width) if !spec.widths_mm.contains(&width) => {
                    blocked = true;
                    notes.push(note(
                        Severity::Error,
                        "marking.width.not_prescribed",
                        format!("{width} mm is not a width prescribed for diagram {}: the alternatives are {} mm ({}).", spec.diagram, list(spec.widths_mm), spec.citation),
                    ));
                }
                Some(width) => {
                    let offered: Vec<u32> = table_widths(&rows);
                    if !offered.is_empty() && !offered.contains(&width) {
                        notes.push(note(
                            Severity::Warning,
                            "marking.width.not_in_table",
                            format!(
                                "{width} mm is prescribed for diagram {} but {table} gives {} mm at {}: {}.",
                                spec.diagram,
                                list(&offered),
                                speed_label(speed),
                                describe(&rows)
                            ),
                        ));
                    }
                }
                None => {
                    let offered = table_widths(&rows);
                    match offered.first() {
                        None => {
                            blocked = true;
                            let why = if rows.is_empty() {
                                format!("the manual's table has no row for diagram {} at {}", spec.diagram, speed_label(speed))
                            } else {
                                format!("{table} gives no width for diagram {} (see para 2.5.3)", spec.diagram)
                            };
                            notes.push(note(
                                Severity::Error,
                                "marking.width.required",
                                format!("No width is set and {why}. Choose one of the prescribed widths: {} mm.", list(spec.widths_mm)),
                            ));
                        }
                        Some(&first) => {
                            width_mm = Some(first);
                            let primary: Vec<u32> = rows.iter().filter_map(|r| r.width_mm).fold(Vec::new(), |mut v, w| {
                                if !v.contains(&w) {
                                    v.push(w);
                                }
                                v
                            });
                            if primary.len() > 1 {
                                notes.push(note(
                                    Severity::Warning,
                                    "marking.width.default",
                                    format!(
                                        "No width is set and {table} gives more than one for diagram {} at {}: {}. {first} mm (the first row) is drawn; choose the width for the road layout.",
                                        spec.diagram,
                                        speed_label(speed),
                                        describe(&rows)
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
            if spec.group == LineGroup::Lane && speed == 70 && width_mm == Some(100) {
                notes.push(note(
                    Severity::Info,
                    "marking.width.lane_70",
                    "On 70 mph dual carriageway all-purpose roads and on motorways the wider (150 mm) lane line should be used (Chapter 5 (2018) para 2.3.5).".to_string(),
                ));
            }
        }

        let Some(width_mm) = width_mm.filter(|_| !blocked) else {
            return (None, notes);
        };
        let width_m = width_mm as f64 / 1000.0;
        let strands = match (spec.double, separation_mm) {
            (Some(double), Some(separation)) => {
                // Centre of each line from the middle of the pair: half the gap plus half a line.
                let shift = (separation as f64 / 2.0 + width_mm as f64 / 2.0) / 1000.0;
                vec![
                    StrandSpec { name: "left line".to_string(), lateral_m: self.offset_m - shift, width_m, pattern: double.left },
                    StrandSpec { name: "right line".to_string(), lateral_m: self.offset_m + shift, width_m, pattern: double.right },
                ]
            }
            _ => vec![StrandSpec { name: "line".to_string(), lateral_m: self.offset_m, width_m, pattern: spec.pattern }],
        };
        (Some(Resolved { spec, width_mm, separation_mm, strands }), notes)
    }

    /// Resolves and lays out the marking.
    ///
    /// Returns the resolved definition and its layout (`None` when an Error note prevents it)
    /// and every note: those of [`MarkingDefinition::resolve`], the station and geometry
    /// errors, and remarks on the result (a part mark, fewer marks than Table 2-3's minimum).
    pub fn lay_out(&self) -> (Option<(Resolved, Layout)>, Vec<Note>) {
        let (resolved, mut notes) = self.resolve();
        let Some(geometry) = &self.geometry else {
            notes.push(note(
                Severity::Error,
                "marking.alignment.missing",
                if self.alignment.is_empty() {
                    "The marking is not tied to an alignment.".to_string()
                } else {
                    format!("The marking has no geometry for alignment '{}'; synchronise it with the alignment.", self.alignment)
                },
            ));
            return (None, notes);
        };
        let Some(resolved) = resolved else {
            return (None, notes);
        };
        let layout = match lay_out(geometry, self.start_station_m, self.end_station_m, self.offset_m, &resolved.strands, self.anchor) {
            Ok(layout) => layout,
            Err(e) => {
                let code = match e {
                    LayoutError::StationOutside { .. } => "marking.stations.outside",
                    LayoutError::StationOrder { .. } => "marking.stations.order",
                    LayoutError::InsideCurve { .. } => "marking.offset.inside_curve",
                    LayoutError::EmptyAlignment => "marking.alignment.empty",
                    LayoutError::BadWidth => "marking.width.invalid",
                };
                let mut message = e.to_string();
                message[..1].make_ascii_uppercase();
                notes.push(note(Severity::Error, code, format!("{message}.")));
                return (None, notes);
            }
        };

        for strand in &layout.strands {
            let Pattern::Broken { mark_mm, .. } = strand.pattern else { continue };
            if let Some(part) = strand.marks.iter().find(|m| !m.whole) {
                notes.push(note(
                    Severity::Info,
                    "marking.part_mark",
                    format!(
                        "The {} ends in a part mark {:.3} m long (a whole mark is {:.3} m), at the {} of the length. Move a station or change the anchor if a whole mark is wanted there.",
                        strand.name,
                        part.length_m(),
                        mark_mm as f64 / 1000.0,
                        if self.anchor == PatternAnchor::Start { "end" } else { "start" }
                    ),
                ));
            }
        }
        if resolved.spec.group == LineGroup::Warning {
            let rows = applications_for(resolved.spec.line_type, self.speed_limit_mph);
            let minimum = rows.iter().filter(|r| r.offers(resolved.width_mm)).filter_map(|r| r.min_marks?.for_speed(self.speed_limit_mph)).min();
            let marks = layout.whole_marks();
            if let Some(minimum) = minimum.filter(|m| marks < *m as usize) {
                notes.push(note(
                    Severity::Info,
                    "marking.marks.minimum",
                    format!(
                        "This warning line has {marks} whole marks. Chapter 5 (2018) Table 2-3 gives a minimum of {minimum} marks on each approach for a {} mm line at {} (more on four or six-lane roads); see paras 2.4.11 to 2.4.14 for where fewer are accepted.",
                        resolved.width_mm,
                        speed_label(self.speed_limit_mph)
                    ),
                ));
            }
        }
        (Some((resolved, layout)), notes)
    }
}

fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

fn list(widths: &[u32]) -> String {
    let parts: Vec<String> = widths.iter().map(u32::to_string).collect();
    match parts.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    }
}

/// The widths the rows give or allow, in table order, without repeats.
fn table_widths(rows: &[&crate::catalogue::Application]) -> Vec<u32> {
    let mut out = Vec::new();
    for w in rows.iter().flat_map(|r| [r.width_mm, r.alternative_width_mm]).flatten() {
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// `"100 mm: Two-lane roads...; 150 mm: Four-lanes or more..."` for a note.
fn describe(rows: &[&crate::catalogue::Application]) -> String {
    rows.iter()
        .map(|r| {
            let width = match (r.width_mm, r.alternative_width_mm) {
                (Some(w), Some(alt)) => format!("{w} ({alt}) mm"),
                (Some(w), None) => format!("{w} mm"),
                _ => "no width".to_string(),
            };
            format!("{width}: {}", r.description)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::AlignmentBuilder;

    fn on_straight(mut def: MarkingDefinition) -> MarkingDefinition {
        def.alignment = "A".to_string();
        def.geometry = Some(AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(500.0).build().unwrap());
        def
    }

    fn codes(def: &MarkingDefinition, severity: Severity) -> Vec<String> {
        def.lay_out().1.into_iter().filter(|n| n.severity == severity).map(|n| n.code).collect()
    }

    #[test]
    fn a_table_width_is_taken_when_there_is_exactly_one() {
        // Lane line at 30 mph: Table 2-2 gives 100 (150); 100 is the row's width.
        let def = on_straight(MarkingDefinition::new(LineType::Lane1005, 0.0, 60.0, 1.8, 30));
        let (laid, notes) = def.lay_out();
        assert_eq!(laid.unwrap().0.width_mm, 100);
        assert!(notes.is_empty(), "{notes:?}");
        // 1012.1 has three rows at 60 mph (100, 150, 200): the first is drawn and it is flagged.
        let def = on_straight(MarkingDefinition::new(LineType::Edge1012_1, 0.0, 60.0, 3.5, 60));
        let (laid, notes) = def.lay_out();
        assert_eq!(laid.unwrap().0.width_mm, 100);
        assert_eq!(notes.iter().map(|n| (n.severity, n.code.as_str())).collect::<Vec<_>>(), [(Severity::Warning, "marking.width.default")]);
        assert!(notes[0].message.contains("150 mm: All-purpose roads with hard shoulder or hard strip"));
    }

    #[test]
    fn a_width_the_manual_does_not_offer_is_flagged() {
        // 125 mm is not prescribed for diagram 1008 at all.
        let def = on_straight(MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(125));
        assert_eq!(codes(&def, Severity::Error), ["marking.width.not_prescribed"]);
        assert!(def.lay_out().0.is_none());
        // 300 mm is prescribed for diagram 1010 but Table 2-5 gives 100 or 200 mm at 30 mph.
        let def = on_straight(MarkingDefinition::new(LineType::Edge1010, 0.0, 60.0, 3.5, 30).with_width(300));
        assert_eq!(codes(&def, Severity::Warning), ["marking.width.not_in_table"]);
        assert!(def.lay_out().0.is_some());
        // 150 mm at 60 mph is what the table gives.
        let def = on_straight(MarkingDefinition::new(LineType::Edge1010, 0.0, 60.0, 3.5, 60).with_width(150));
        assert!(def.lay_out().1.is_empty());
        // 1009A: the table gives no width, so one must be chosen.
        let def = on_straight(MarkingDefinition::new(LineType::Edge1009A, 0.0, 9.0, 3.5, 30));
        assert_eq!(codes(&def, Severity::Error), ["marking.width.required"]);
        assert!(on_straight(MarkingDefinition::new(LineType::Edge1009A, 0.0, 9.0, 3.5, 30).with_width(100)).lay_out().1.is_empty());
    }

    #[test]
    fn the_diagram_is_checked_against_the_speed_limit() {
        // 1008.1 at 30 mph: the table gives it for over 40 only.
        let def = on_straight(MarkingDefinition::new(LineType::Centre1008_1, 0.0, 90.0, 0.0, 30).with_width(100));
        assert_eq!(codes(&def, Severity::Warning), ["marking.speed.diagram"]);
        // 1008 at 60 mph: allowed under NOTE 1, reported as a condition.
        let def = on_straight(MarkingDefinition::new(LineType::Centre1008, 0.0, 90.0, 0.0, 60));
        let (laid, notes) = def.lay_out();
        assert_eq!(laid.unwrap().0.width_mm, 100);
        assert_eq!(notes.iter().map(|n| (n.severity, n.code.as_str())).collect::<Vec<_>>(), [(Severity::Info, "marking.speed.exception")]);
        assert!(notes[0].message.contains("85th percentile"));
        // 1012.1 at 45 mph: Table 2-5 has no column.
        let def = on_straight(MarkingDefinition::new(LineType::Edge1012_1, 0.0, 90.0, 3.5, 45).with_width(100));
        assert_eq!(codes(&def, Severity::Warning), ["marking.speed.no_column"]);
        // Lane line at 70 mph, 100 mm: para 2.3.5.
        let def = on_straight(MarkingDefinition::new(LineType::Lane1005_1, 0.0, 90.0, 3.65, 70));
        assert_eq!(codes(&def, Severity::Info), ["marking.width.lane_70"]);
        assert!(on_straight(MarkingDefinition::new(LineType::Lane1005_1, 0.0, 90.0, 3.65, 70).with_width(150)).lay_out().1.is_empty());
    }

    #[test]
    fn unknown_types_missing_alignments_and_bad_stations_are_errors() {
        let mut def = on_straight(MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30));
        def.line_type = "1012.2".to_string();
        assert_eq!(codes(&def, Severity::Error), ["marking.type.unknown"]);

        let loose = MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30);
        assert_eq!(codes(&loose, Severity::Error), ["marking.alignment.missing"]);

        let def = on_straight(MarkingDefinition::new(LineType::Centre1008, 450.0, 520.0, 0.0, 30).with_width(100));
        let notes = def.lay_out().1;
        assert_eq!(notes[0].code, "marking.stations.outside");
        assert!(notes[0].message.contains("520.000") && notes[0].message.contains("from 0.000 m to 500.000 m"));

        let def = on_straight(MarkingDefinition::new(LineType::Centre1008, 80.0, 20.0, 0.0, 30));
        assert_eq!(codes(&def, Severity::Error), ["marking.stations.order"]);

        let mut def = on_straight(MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30));
        def.speed_limit_mph = 0;
        assert_eq!(codes(&def, Severity::Error), ["marking.speed.missing"]);
    }

    #[test]
    fn yellow_is_flagged_on_a_white_line() {
        let mut def = on_straight(MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(100));
        def.colour = MarkingColour::Yellow;
        assert_eq!(codes(&def, Severity::Warning), ["marking.colour"]);
    }

    #[test]
    fn double_lines_follow_paragraph_3_2_1() {
        // Defaults: two 150 mm lines 175 mm apart, 475 mm overall; centres 162.5 mm either side.
        let def = on_straight(MarkingDefinition::new(LineType::Double1013_1BrokenLeft, 0.0, 60.0, 0.0, 60));
        let (laid, notes) = def.lay_out();
        let (resolved, layout) = laid.unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((resolved.width_mm, resolved.separation_mm), (150, Some(175)));
        assert!((resolved.strands[0].lateral_m + 0.1625).abs() < 1e-12 && (resolved.strands[1].lateral_m - 0.1625).abs() < 1e-12);
        // Left strand: 1 m marks at 6 m centres over 60 m = 10 marks. Right strand: one continuous line.
        assert_eq!((layout.strands[0].whole_marks(), layout.strands[1].whole_marks()), (10, 1));
        assert!((layout.painted_length_m() - 70.0).abs() < 1e-9);
        assert!((layout.painted_area_m2() - 10.5).abs() < 1e-9);
        // The inner edges are 175 mm apart: left line's right edge at y = +0.0875 on a line heading east.
        assert!((layout.strands[0].marks[0].right[0].1 - 0.0875).abs() < 1e-12);
        assert!((layout.strands[1].marks[0].left[0].1 + 0.0875).abs() < 1e-12);

        let narrow = on_straight(MarkingDefinition::new(LineType::Double1013_1Continuous, 0.0, 60.0, 0.0, 60).with_width(90));
        assert_eq!(codes(&narrow, Severity::Error), ["marking.double.width"]);
        let mut close = on_straight(MarkingDefinition::new(LineType::Double1013_1Continuous, 0.0, 60.0, 0.0, 60));
        close.separation_mm = Some(80);
        assert_eq!(codes(&close, Severity::Error), ["marking.double.separation"]);
        let mut wide = on_straight(MarkingDefinition::new(LineType::Double1013_1Continuous, 0.0, 60.0, 0.0, 60).with_width(300));
        wide.separation_mm = Some(301);
        assert_eq!(codes(&wide, Severity::Error), ["marking.double.overall"]);
        wide.separation_mm = Some(300); // 900 mm exactly is allowed
        assert!(codes(&wide, Severity::Error).is_empty());
        let minimum = on_straight(MarkingDefinition::new(LineType::Double1013_1Continuous, 0.0, 60.0, 0.0, 60).with_width(100));
        assert_eq!(codes(&minimum, Severity::Info), ["marking.double.width_not_normal"]);
    }

    #[test]
    fn part_marks_and_short_warning_lines_are_remarked_on() {
        // 1004 at 40 mph, 100 mm: Table 2-3 minimum 7 marks. 30 m gives five 6 m modules.
        let def = on_straight(MarkingDefinition::new(LineType::Warning1004, 0.0, 30.0, 0.0, 40).with_width(100));
        assert_eq!(codes(&def, Severity::Info), ["marking.marks.minimum"]);
        // 42 m gives seven.
        let def = on_straight(MarkingDefinition::new(LineType::Warning1004, 0.0, 42.0, 0.0, 40).with_width(100));
        assert!(def.lay_out().1.is_empty());
        // 44 m ends 2 m into the eighth mark.
        let def = on_straight(MarkingDefinition::new(LineType::Warning1004, 0.0, 44.0, 0.0, 40).with_width(100));
        let notes = def.lay_out().1;
        assert_eq!(notes.iter().map(|n| n.code.as_str()).collect::<Vec<_>>(), ["marking.part_mark"]);
        assert!(notes[0].message.contains("2.000 m long"));
    }

    #[test]
    fn the_definition_round_trips_through_json() {
        let mut def = on_straight(MarkingDefinition::new(LineType::Double1013_1BrokenRight, 12.5, 180.0, -0.25, 50).with_width(150).with_material("thermoplastic"));
        def.separation_mm = Some(175);
        def.anchor = PatternAnchor::End;
        let text = serde_json::to_string(&def).unwrap();
        let back: MarkingDefinition = serde_json::from_str(&text).unwrap();
        assert_eq!(back, def);
        assert_eq!(MarkingDefinition::from_value(&def.to_value()).unwrap(), def);
        // Optional fields may be absent.
        let minimal: MarkingDefinition = serde_json::from_str(r#"{"line_type":"1008","start_station_m":0,"end_station_m":60,"speed_limit_mph":30}"#).unwrap();
        assert_eq!(minimal, MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30));
        assert!(!MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).to_value().as_object().unwrap().contains_key("width_mm"));
    }
}
