//! Road markings in the app: registering the module, the hand-over to `afrobim-markings` for
//! adding a longitudinal line along an alignment, and its quantities.
//!
//! Line types, patterns and widths come from the Traffic Signs Manual Chapter 5 (2018) as held
//! in `afrobim-markings`; nothing here adds a dimension.

use afrobim_kernel::{Document, ElementId, ModuleRegistry};
use afrobim_markings::{self as markings, LineSpec, MarkingDefinition, MarkingTakeoff};

pub use afrobim_markings::CATEGORY;

pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(markings::MarkingModule));
}

/// The line types the manual's catalogue offers, in its order.
pub fn line_types() -> &'static [LineSpec] {
    markings::catalogue()
}

/// What the "Road marking" dialog collects.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkingRequest {
    /// Index into [`line_types`].
    pub line: usize,
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// Distance from the alignment and the side, looking up-station.
    pub distance_m: f64,
    pub right: bool,
    /// Speed limit in mph, as the manual's tables are written.
    pub speed_mph: u32,
    /// A limit posted in km/h and the answer to an "ask each time" speed band rule. When set, the
    /// project's rule gives the mph the tables are read with (`projset::marking_thresholds`) and
    /// `speed_mph` is replaced by it; the rule's sentence is kept on the marking as "Speed band".
    pub speed_kmh: Option<(f64, Option<bool>)>,
    /// Width in mm; None leaves the choice to the manual's table where it gives exactly one.
    pub width_mm: Option<u32>,
    pub material: String,
}

/// Adds a marking along an alignment. Returns its mark and the module's notes about it
/// (warnings where the manual offers alternatives or the speed is outside a table column).
pub fn add(doc: &mut Document, alignment: ElementId, req: &MarkingRequest) -> Result<String, String> {
    let spec = line_types().get(req.line).ok_or("Choose a line type.")?;
    let offset = if req.right { req.distance_m.abs() } else { -req.distance_m.abs() };
    let mut speed_mph = req.speed_mph;
    let mut band_note = None;
    if let Some((kmh, answer)) = req.speed_kmh {
        let r = crate::projset::resolve_answered(doc, kmh, false, &crate::projset::marking_thresholds(), answer);
        if r.ask.is_some() {
            return Err(format!("The project's speed band rule asks each time: {} Answer in the dialog.", r.band_note));
        }
        speed_mph = r.mph_used.round().max(1.0) as u32;
        band_note = Some(format!("{}; the marking tables are read at {speed_mph} mph", r.band_note));
    }
    let mut def = MarkingDefinition::new(spec.line_type, req.start_station_m, req.end_station_m, offset, speed_mph).with_material(req.material.trim());
    if let Some(w) = req.width_mm {
        def = def.with_width(w);
    }
    let name = doc.unique_name("RM-001");
    markings::add_marking(doc, alignment, &name, &def).map_err(|e| e.to_string())?;
    let id = doc.elements().iter().find(|e| e.name == name).map(|e| e.id);
    if let (Some(note), Some(e)) = (band_note, id.and_then(|id| doc.find_mut(id))) {
        e.properties.insert("Speed band".to_string(), note);
    }
    Ok(name)
}

/// Adds the demo's centre line and two edge lines on the alignment, for `--demo`. Returns what
/// could not be added.
///
/// **Every number is an arbitrary demonstration value**: diagram 1008.1 at the centre and 1012.1
/// at 3.65 m either side, a speed limit of 60 mph, and a stretch from 55 % to 95 % of the
/// alignment's length. That stretch is the one the demo pavement covers (`road_glue::demo`, the
/// same two fractions), because the IFC export attaches a marking to a pavement only when the
/// same alignment's pavement covers all of the marking's stations and its offset: lines laid over
/// the whole alignment would not be inside a pavement that covers part of it, and would export as
/// generic proxies instead of `IfcSurfaceFeature`.
pub fn demo(doc: &mut Document, alignment_id: ElementId, a: &afrobim_alignment::Alignment) -> Vec<String> {
    let types = line_types();
    let (from, to) = crate::road_glue::DEMO_PAVEMENT_FRACTIONS;
    let (start, end) = (a.start_station_m + a.length() * from, a.start_station_m + a.length() * to);
    let mut problems = Vec::new();
    for (id, distance, right) in [("1008.1", 0.0, false), ("1012.1", 3.65, false), ("1012.1", 3.65, true)] {
        if let Some(line) = types.iter().position(|t| t.id == id) {
            let request = MarkingRequest { line, start_station_m: start, end_station_m: end, distance_m: distance, right, speed_mph: 60, speed_kmh: None, width_mm: Some(150), material: String::new() };
            if let Err(e) = add(doc, alignment_id, &request) {
                problems.push(e);
            }
        }
    }
    problems
}

/// Refreshes every marking's copy of its alignment. Call after alignments change.
pub fn sync(doc: &mut Document) -> usize {
    markings::sync_markings(doc)
}

/// The markings schedule and summary CSVs, or None when the document has no markings.
pub fn quantities(doc: &Document) -> Option<(String, String, usize)> {
    if !doc.elements().iter().any(|e| e.category == CATEGORY) {
        return None;
    }
    let takeoff = MarkingTakeoff::measure(doc.elements());
    Some((markings::markings_to_csv(&takeoff), markings::summary_to_csv(&takeoff), takeoff.rows.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{Alignment, Pi};

    #[test]
    fn a_kmh_limit_is_read_through_the_project_rule_and_the_note_stays_on_the_marking() {
        let mut doc = Document::default();
        let a = Alignment::from_pis("MC10", 0.0, &[Pi { x: 0.0, y: 0.0, radius: 0.0, spiral_in: 0.0, spiral_out: 0.0 }, Pi { x: 300.0, y: 0.0, radius: 0.0, spiral_in: 0.0, spiral_out: 0.0 }]).unwrap();
        let alignment = crate::alignment_glue::add(&mut doc, &a).unwrap();
        let line = line_types().iter().position(|s| s.id == "1008.1").unwrap();
        let req = MarkingRequest { line, start_station_m: 0.0, end_station_m: 90.0, distance_m: 0.0, right: false, speed_mph: 30, speed_kmh: Some((80.0, None)), width_mm: Some(100), material: String::new() };
        let mark = add(&mut doc, alignment, &req).unwrap();
        let note = doc.elements().iter().find(|e| e.name == mark).unwrap().properties.get("Speed band").cloned().unwrap();
        assert!(note.contains("treats it as 50 mph") && note.contains("read at 50 mph"), "{note}");
        // "Ask each time" refuses until the engineer answers.
        afrobim_kernel::set_speed_band_rule(&mut doc, &afrobim_kernel::SpeedBandRule::AskEachTime);
        assert!(add(&mut doc, alignment, &req).unwrap_err().contains("asks each time"));
        let yes = MarkingRequest { speed_kmh: Some((80.0, Some(true))), ..req };
        assert!(add(&mut doc, alignment, &yes).is_ok());
    }

    #[test]
    fn a_centre_line_is_added_and_measured() {
        let mut doc = Document::default();
        let a = Alignment::from_pis("MC10", 0.0, &[Pi { x: 0.0, y: 0.0, radius: 0.0, spiral_in: 0.0, spiral_out: 0.0 }, Pi { x: 300.0, y: 0.0, radius: 0.0, spiral_in: 0.0, spiral_out: 0.0 }]).unwrap();
        let alignment = crate::alignment_glue::add(&mut doc, &a).unwrap();
        let line = line_types().iter().position(|s| s.id == "1008.1").unwrap();
        let req = MarkingRequest { line, start_station_m: 0.0, end_station_m: 90.0, distance_m: 0.0, right: false, speed_mph: 60, speed_kmh: None, width_mm: Some(100), material: String::new() };
        let mark = add(&mut doc, alignment, &req).unwrap();
        assert_eq!(mark, "RM-001");
        let (schedule, summary, rows) = quantities(&doc).unwrap();
        assert_eq!(rows, 1);
        assert!(schedule.contains("RM-001") && !summary.is_empty());
        // 1008.1 is a 3 m mark and a 6 m gap: 90 m holds ten modules, so 30 m is painted.
        assert!(schedule.contains("30"), "{schedule}");
        // A station beyond the alignment is refused.
        let bad = MarkingRequest { end_station_m: 900.0, ..req };
        assert!(add(&mut doc, alignment, &bad).is_err());
    }
}
