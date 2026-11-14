//! The construction programme in the app: where it is kept, how model elements are offered to
//! links, and the conversion between the date types of the schedule and cost crates.
//!
//! The programme is one document element of category `schedule` (the schedule crate's
//! `add_programme`, `store_programme` and `load_programme`), so it is saved, opened and undone
//! with the document.

use std::collections::HashSet;

use afrobim_kernel::{Document, Element, ElementId};
use afrobim_schedule::{
    add_programme, export_xer_with_report, import_xer_bytes, load_programme, store_programme, Calendar, CriticalBasis, Date, ElementState, ElementStates, LocationRange, Playback, Programme, RollupBasis, Schedule, SummaryTask, WorkDirection,
    Weekday, SCHEDULE_CATEGORY,
};

use crate::coordination_glue::is_record;

/// The document's programme element and its programme, if there is one.
pub fn load(doc: &Document) -> Option<(ElementId, Programme)> {
    let id = doc.elements().iter().find(|e| e.category == SCHEDULE_CATEGORY)?.id;
    load_programme(doc, id).ok().map(|p| (id, p))
}

/// Writes the programme into its element, or adds the element the first time.
pub fn store(doc: &mut Document, id: Option<ElementId>, p: &Programme) -> Result<ElementId, String> {
    match id.filter(|i| doc.find(*i).is_some()) {
        Some(id) => store_programme(doc, id, p).map(|_| id),
        None => Ok(add_programme(doc, &p.name, p)),
    }
}

/// A new programme with the working weekdays the user ticked. None are assumed.
pub fn new_programme(name: &str, start: Date, calendar_name: &str, days: &[Weekday]) -> Result<Programme, String> {
    if name.trim().is_empty() {
        return Err("Give the programme a name.".into());
    }
    if calendar_name.trim().is_empty() {
        return Err("Name the calendar.".into());
    }
    if days.is_empty() {
        return Err("Tick at least one working weekday. AfroBIM assumes none.".into());
    }
    Ok(Programme::new(name.trim(), start, Calendar::new(calendar_name.trim(), days)))
}

/// Elements a link can point to: everything but the document's own records and the alignments
/// (an alignment is not built by an activity).
pub fn linkable(e: &Element) -> bool {
    !is_record(&e.category) && e.category != crate::alignment_glue::CATEGORY
}

// ─── how the programme is viewed, kept on its element ───────────────────────────────────

/// Properties of the programme element that remember the user's choices of method. They are not
/// part of the programme (the exchange formats do not carry them).
pub const KEY_CRITICAL: &str = "view.critical_basis";
pub const KEY_ROLLUP: &str = "view.rollup_basis";
pub const KEY_TL_ALIGNMENT: &str = "view.time_location_alignment";

pub fn setting(doc: &Document, key: &str) -> Option<String> {
    let id = doc.elements().iter().find(|e| e.category == SCHEDULE_CATEGORY)?.id;
    doc.find(id)?.properties.get(key).cloned()
}

pub fn set_setting(doc: &mut Document, key: &str, value: &str) {
    if let Some(id) = doc.elements().iter().find(|e| e.category == SCHEDULE_CATEGORY).map(|e| e.id) {
        if let Some(e) = doc.find_mut(id) {
            e.properties.insert(key.to_string(), value.to_string());
        }
    }
}

/// What makes an activity critical: the engine's default (total float) until the user chooses.
pub fn critical_basis_of(doc: &Document) -> CriticalBasis {
    match setting(doc, KEY_CRITICAL).as_deref() {
        Some("longest_path") => CriticalBasis::LongestPath,
        _ => CriticalBasis::TotalFloat,
    }
}

pub fn critical_key(b: CriticalBasis) -> &'static str {
    match b {
        CriticalBasis::TotalFloat => "total_float",
        CriticalBasis::LongestPath => "longest_path",
    }
}

pub fn rollup_basis_of(doc: &Document) -> RollupBasis {
    match setting(doc, KEY_ROLLUP).as_deref() {
        Some("weight") => RollupBasis::Weight,
        _ => RollupBasis::Duration,
    }
}

pub fn rollup_key(b: RollupBasis) -> &'static str {
    match b {
        RollupBasis::Duration => "duration",
        RollupBasis::Weight => "weight",
    }
}

/// The engine's own definitions of the two bases of criticality, as written in its documentation.
pub const CRITICAL_TOTAL_FLOAT_TEXT: &str = "Total float of zero or less and not complete. Total float is the working days in [early finish, late finish): the number of working days the finish can move without moving the finish limit. It is negative when the late finish is before the early finish. With a required finish later than the computed finish nothing is critical by this test.";
pub const CRITICAL_LONGEST_PATH_TEXT: &str = "On the longest path: the chain of driving relationships that ends at the project finish. Start from every unfinished activity whose early finish is the project finish. An unfinished predecessor joins the path when its relationship is driving, that is when the date that relationship alone requires of the successor equals the successor's scheduled (remaining) start. Repeat from each activity that joined. A constraint, the data date or the programme start that holds an activity later than its logic ends the chain there. Relationships of work already started that are spent (SS) never drive. This is the definition of the schedule crate; it has not been compared with what P6 or MS Project mark as the longest path.";

pub const ROLLUP_DURATION_TEXT: &str = "percent = sum(duration x percent) / sum(duration) over every activity below the summary task, at any depth. The weight is the entered duration in working days; a milestone weighs nothing. When the durations add up to zero (only milestones) the percent is the share of activities complete. Complete work counts as 100.";
pub const ROLLUP_WEIGHT_TEXT: &str = "The same formula with the weight entered on each activity as the weight. Every activity below a summary task must have a weight, or the roll-up says which do not.";

// ─── Primavera XER ──────────────────────────────────────────────────────────────────────

/// What the dialogs say about XER, in every report.
pub const XER_UNTESTED: &str = "Files written or read here have not been tested in Primavera P6. The writer and the reader follow the published XER layout and were checked against third-party readers, not against P6 itself. Open an exported file in P6 and check it before relying on it.";

/// Reads an XER file's bytes into a programme. The links of the programme being replaced are kept
/// for activities that survive (a file cannot carry links to model elements). The lines are what
/// the file could not carry, in the engine's words.
pub fn import_xer_file(bytes: &[u8], old: Option<&Programme>) -> Result<(Programme, Vec<String>), String> {
    let (mut p, report) = import_xer_bytes(bytes).map_err(|e| e.to_string())?;
    if let Some(old) = old {
        p.links = old.links.iter().filter(|l| p.activity(&l.activity).is_some()).cloned().collect();
    }
    Ok((p, report.warnings))
}

/// The programme as XER text, with the lines of what the file does not carry.
pub fn export_xer_text(p: &Programme, s: Option<&Schedule>) -> (String, Vec<String>) {
    let (text, report) = export_xer_with_report(p, s);
    (text, report.warnings)
}

// ─── the work breakdown structure ───────────────────────────────────────────────────────

/// Every summary task below `id`, at any depth.
pub fn descendants_of(p: &Programme, id: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let mut frontier = vec![id.to_string()];
    while let Some(next) = frontier.pop() {
        for s in p.summaries.iter().filter(|s| s.parent.as_deref() == Some(next.as_str())) {
            if out.insert(s.id.clone()) {
                frontier.push(s.id.clone());
            }
        }
    }
    out
}

pub fn add_summary(p: &mut Programme, id: &str, wbs: &str, name: &str, parent: Option<&str>) -> Result<(), String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("Give the summary task an id.".into());
    }
    if p.summary(id).is_some() {
        return Err(format!("There is already a summary task '{id}'."));
    }
    if p.activity(id).is_some() {
        return Err(format!("'{id}' is an activity id; use a different id for the summary task."));
    }
    if let Some(q) = parent {
        if p.summary(q).is_none() {
            return Err(format!("There is no summary task '{q}' to put it under."));
        }
    }
    p.summaries.push(SummaryTask::new(id, wbs.trim(), name.trim(), parent));
    Ok(())
}

/// Removes a summary task. What was under it moves up to its parent.
pub fn remove_summary(p: &mut Programme, id: &str) {
    let parent = p.summary(id).and_then(|s| s.parent.clone());
    for s in &mut p.summaries {
        if s.parent.as_deref() == Some(id) {
            s.parent = parent.clone();
        }
    }
    for a in &mut p.activities {
        if a.parent.as_deref() == Some(id) {
            a.parent = parent.clone();
        }
    }
    p.summaries.retain(|s| s.id != id);
}

/// Indent: the summary task becomes a child of the summary task just above it at its own level.
pub fn indent_summary(p: &mut Programme, id: &str) -> Result<(), String> {
    let me = p.summary(id).ok_or("No such summary task.")?.clone();
    let before = p.summaries.iter().take_while(|s| s.id != id).filter(|s| s.parent == me.parent).last().map(|s| s.id.clone());
    match before {
        Some(b) => {
            if let Some(s) = p.summaries.iter_mut().find(|s| s.id == id) {
                s.parent = Some(b);
            }
            Ok(())
        }
        None => Err("There is no summary task above it at this level to indent under.".into()),
    }
}

/// Outdent: the summary task moves up one level, to its parent's parent.
pub fn outdent_summary(p: &mut Programme, id: &str) -> Result<(), String> {
    let parent = p.summary(id).ok_or("No such summary task.")?.parent.clone().ok_or("It is already at the top.")?;
    let grand = p.summary(&parent).and_then(|s| s.parent.clone());
    if let Some(s) = p.summaries.iter_mut().find(|s| s.id == id) {
        s.parent = grand;
    }
    Ok(())
}

/// The activities and summary tasks hidden when the `collapsed` summary tasks are folded: everything
/// below a collapsed summary task, at any depth (the collapsed task itself stays).
pub fn hidden_by_collapse(p: &Programme, collapsed: &HashSet<String>) -> (HashSet<String>, HashSet<String>) {
    let mut hidden_summaries: HashSet<String> = HashSet::new();
    for c in collapsed {
        hidden_summaries.extend(descendants_of(p, c));
    }
    let hidden_activities: HashSet<String> = p
        .activities
        .iter()
        .filter(|a| a.parent.as_ref().is_some_and(|q| collapsed.contains(q) || hidden_summaries.contains(q)))
        .map(|a| a.id.clone())
        .collect();
    (hidden_activities, hidden_summaries)
}

// ─── location ranges ────────────────────────────────────────────────────────────────────

/// The location range a row of the time-location table stores: the two stations (internal, as the
/// station fields convert them from the stations shown on the drawings), the direction the user
/// chose and the dwell as typed (blank is none). Nothing is assumed: a direction must be chosen.
pub fn range_from_entries(alignment: &str, from_m: f64, to_m: f64, direction: Option<WorkDirection>, dwell_text: &str) -> Result<LocationRange, String> {
    let direction = direction.ok_or("Choose the direction of working.")?;
    let dwell = dwell_text.trim();
    let dwell_days = if dwell.is_empty() { None } else { Some(dwell.parse::<u32>().map_err(|_| "The dwell is a whole number of days, or blank for none.".to_string())?) };
    let mut r = LocationRange::new(alignment, from_m, to_m, direction);
    r.dwell_days = dwell_days;
    Ok(r)
}

// ─── playback ───────────────────────────────────────────────────────────────────────────

/// The states for a new date, or `None` when nothing changes between the date of the states held
/// and the new one (moving forward), so a player redraws only on the dates in
/// `Playback::change_dates`. Moving backwards, or with no states yet, always gives new states.
pub fn states_update(held: Option<&ElementStates>, pb: &Playback, date: Date) -> Option<ElementStates> {
    if let Some(h) = held.filter(|h| h.date <= date) {
        let from = pb.change_dates.partition_point(|d| *d <= h.date);
        let to = pb.change_dates.partition_point(|d| *d <= date);
        if from == to {
            return None;
        }
    }
    Some(states_at(pb, date))
}

/// The states of every linked element on a date, read from a playback without touching the
/// schedule. The same type `element_states` returns, so the 3D view uses it unchanged; the per-link
/// detail is not carried (the view does not use it).
pub fn states_at(pb: &Playback, date: Date) -> ElementStates {
    let states = pb
        .elements
        .iter()
        .filter_map(|t| t.at(date).map(|c| ElementState { element: t.element, phase: c.phase, present: c.present, links: Vec::new() }))
        .collect();
    ElementStates { date, states, unlinked: pb.unlinked.clone() }
}

/// A schedule date as a cost date.
pub fn cost_date(d: Date) -> afrobim_cost::Date {
    let (y, m, day) = d.ymd();
    afrobim_cost::Date::new(y, m as u8, day as u8).expect("a schedule date is a real date")
}

#[cfg(test)]
pub fn from_cost_date(d: afrobim_cost::Date) -> Date {
    Date::from_ymd(d.year(), d.month() as u32, d.day() as u32).expect("a cost date is a real date")
}

pub fn weekday_label(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;
    use afrobim_schedule::{schedule, Activity, ElementRule, ElementSelector, LinkAction, LinkType};

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    fn sample() -> Programme {
        let mut p = new_programme("Test programme", d(2026, 3, 2), "Five day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]).unwrap();
        p.calendars[0].holidays.insert(d(2026, 3, 9));
        p.add_activity(Activity::new("A", "1.1", "First", 3));
        p.add_activity(Activity::new("B", "1.2", "Second", 4));
        p.link("A", "B", LinkType::FS, 1);
        p.link_elements("A", ElementSelector::Id(ElementId(7)), LinkAction::Construct);
        p.link_elements("B", ElementSelector::Rule(ElementRule { category: Some("drain".into()), layer: None, mark_pattern: Some("SD-*".into()) }), LinkAction::Temporary);
        let s = schedule(&p).unwrap();
        p.set_baseline("Fixture baseline", &s);
        p.data_date = Some(d(2026, 3, 4));
        p
    }

    #[test]
    fn the_programme_survives_save_and_open() {
        let mut doc = Document::default();
        doc.add("sign", "S1", Pt::new(0.0, 0.0), 10.0, 10.0, "0", serde_json::Value::Null);
        let p = sample();
        let id = store(&mut doc, None, &p).unwrap();
        // Storing again updates the element; it does not add a second programme.
        let mut p2 = p.clone();
        p2.add_activity(Activity::new("C", "1.3", "Third", 2));
        assert_eq!(store(&mut doc, Some(id), &p2).unwrap(), id);
        assert_eq!(doc.elements().iter().filter(|e| e.category == SCHEDULE_CATEGORY).count(), 1);
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        let (rid, back) = load(&reopened).unwrap();
        assert_eq!(rid, id);
        assert_eq!(back, p2);
        // The reloaded programme schedules to the same dates, holiday and baseline included.
        assert_eq!(schedule(&back).unwrap().rows, schedule(&p2).unwrap().rows);
        assert!(back.baseline.is_some() && back.links.len() == 2 && back.calendars[0].holidays.contains(&d(2026, 3, 9)));
    }

    #[test]
    fn a_programme_needs_a_name_a_calendar_and_working_days() {
        assert!(new_programme("", d(2026, 1, 5), "Cal", &[Weekday::Mon]).is_err());
        assert!(new_programme("P", d(2026, 1, 5), "Cal", &[]).unwrap_err().contains("assumes none"));
    }

    fn wbs_sample() -> Programme {
        // Arbitrary test programme: two summary tasks, the second with a sub-summary.
        let mut p = sample();
        add_summary(&mut p, "S1", "1", "Earthworks", None).unwrap();
        add_summary(&mut p, "S2", "2", "Surfacing", None).unwrap();
        add_summary(&mut p, "S2A", "2.1", "Binder", Some("S2")).unwrap();
        p.activity_mut("A").unwrap().parent = Some("S1".into());
        p.activity_mut("B").unwrap().parent = Some("S2A".into());
        p.activity_mut("B").unwrap().extra_constraints = vec![afrobim_schedule::Constraint::FinishNoLaterThan(d(2026, 6, 30))];
        p
    }

    #[test]
    fn xer_export_then_import_through_the_app_glue_keeps_the_wbs_and_the_links() {
        let p = wbs_sample();
        let s = afrobim_schedule::schedule(&p).unwrap();
        let (text, warnings) = export_xer_text(&p, Some(&s));
        assert!(text.starts_with("ERMHDR"), "{}", &text[..40.min(text.len())]);
        // The second constraint of B is one the file cannot carry in this place or it is carried as the second: either way nothing is lost silently.
        let _ = warnings;
        let (back, _) = import_xer_file(text.as_bytes(), Some(&p)).unwrap();
        let name_of = |p: &Programme, id: &Option<String>| id.as_ref().and_then(|i| p.summary(i)).map(|x| x.name.clone());
        assert_eq!(back.summaries.len(), 3);
        let parent_name = |p: &Programme, act: &str| p.activity(act).and_then(|a| name_of(p, &a.parent));
        assert_eq!(parent_name(&back, "A").as_deref(), Some("Earthworks"));
        assert_eq!(parent_name(&back, "B").as_deref(), Some("Binder"));
        let binder = back.summaries.iter().find(|x| x.name == "Binder").unwrap();
        assert_eq!(name_of(&back, &binder.parent).as_deref(), Some("Surfacing"));
        // The element links of the programme being replaced stay on the activities that survive.
        assert_eq!(back.links.len(), p.links.len());
        // Both constraints of B came through.
        assert_eq!(back.activity("B").unwrap().constraints().count(), p.activity("B").unwrap().constraints().count());
    }

    #[test]
    fn summary_tasks_indent_outdent_remove_and_collapse() {
        let mut p = wbs_sample();
        assert!(add_summary(&mut p, "S1", "9", "dup", None).is_err());
        assert!(add_summary(&mut p, "A", "9", "clashes with an activity id", None).is_err());
        assert!(add_summary(&mut p, "S9", "9", "x", Some("nope")).is_err());
        // Indent S2 under S1 (the summary above it at its level), then outdent it again.
        indent_summary(&mut p, "S2").unwrap();
        assert_eq!(p.summary("S2").unwrap().parent.as_deref(), Some("S1"));
        assert!(indent_summary(&mut p, "S1").is_err());
        outdent_summary(&mut p, "S2").unwrap();
        assert_eq!(p.summary("S2").unwrap().parent, None);
        assert!(outdent_summary(&mut p, "S2").is_err());
        // Folding S2 hides what is under it at any depth, not S2 itself.
        let collapsed: HashSet<String> = ["S2".to_string()].into_iter().collect();
        let (acts, sums) = hidden_by_collapse(&p, &collapsed);
        assert!(acts.contains("B") && !acts.contains("A"));
        assert!(sums.contains("S2A") && !sums.contains("S2"));
        // Removing S2A moves B up to S2.
        remove_summary(&mut p, "S2A");
        assert_eq!(p.activity("B").unwrap().parent.as_deref(), Some("S2"));
        // The roll-up and the folded Gantt chart draw fewer rows with the activities hidden.
        let s = afrobim_schedule::schedule(&p).unwrap();
        let (open, e1) = crate::schedule_ext::build_gantt(&p, &s, RollupBasis::Duration, &HashSet::new());
        let (folded, e2) = crate::schedule_ext::build_gantt(&p, &s, RollupBasis::Duration, &collapsed);
        assert!(e1.is_none() && e2.is_none());
        let names = |g: &afrobim_schedule::GanttDrawing| g.labels.iter().filter(|l| l.kind == afrobim_schedule::GanttLabelKind::Name).count();
        assert!(names(&folded) < names(&open), "{} {}", names(&folded), names(&open));
        // On the weight basis with no weights entered the roll-up says so and the plain chart is drawn.
        let (_, err) = crate::schedule_ext::build_gantt(&p, &s, RollupBasis::Weight, &HashSet::new());
        assert!(err.is_some());
    }

    #[test]
    fn the_choices_of_method_are_kept_on_the_programme_element() {
        let mut doc = Document::default();
        let p = sample();
        store(&mut doc, None, &p).unwrap();
        assert_eq!(critical_basis_of(&doc), CriticalBasis::TotalFloat);
        assert_eq!(rollup_basis_of(&doc), RollupBasis::Duration);
        set_setting(&mut doc, KEY_CRITICAL, critical_key(CriticalBasis::LongestPath));
        set_setting(&mut doc, KEY_ROLLUP, rollup_key(RollupBasis::Weight));
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(critical_basis_of(&reopened), CriticalBasis::LongestPath);
        assert_eq!(rollup_basis_of(&reopened), RollupBasis::Weight);
        // The programme itself is untouched by the choices.
        assert_eq!(load(&reopened).unwrap().1, p);
    }

    #[test]
    fn location_ranges_convert_displayed_stations_to_internal_ones_and_need_a_direction() {
        // MC10 of the corridor test file has station equations: at internal 1300 the displayed
        // station jumps from 1300 to 1350, and at internal 1600 it drops from 1650 to 1500.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../afrobim-alignment/tests/landxml/corridor.xml");
        let import = afrobim_alignment::read_landxml(std::path::Path::new(path)).unwrap();
        let a = import.alignments.iter().find(|a| a.name == "MC10").unwrap();
        use crate::station_input::displayed_to_internal;
        assert!((displayed_to_internal(a, 1400.0, 2).unwrap() - 1350.0).abs() < 1e-6);
        // 1550 is shown twice: it needs the region.
        assert!(displayed_to_internal(a, 1550.0, 0).is_err());
        assert!((displayed_to_internal(a, 1550.0, 2).unwrap() - 1500.0).abs() < 1e-6);
        assert!((displayed_to_internal(a, 1550.0, 3).unwrap() - 1650.0).abs() < 1e-6);
        // The range the table stores holds the internal stations and what the user chose.
        let from = displayed_to_internal(a, 1400.0, 2).unwrap();
        let to = displayed_to_internal(a, 1550.0, 3).unwrap();
        let r = range_from_entries("MC10", from, to, Some(WorkDirection::Increasing), " 3 ").unwrap();
        assert_eq!((r.from_m, r.to_m, r.dwell_days, r.alignment.as_str()), (1350.0, 1650.0, Some(3), "MC10"));
        assert_eq!(range_from_entries("MC10", from, to, Some(WorkDirection::Fixed), "").unwrap().dwell_days, None);
        assert!(range_from_entries("MC10", from, to, None, "").unwrap_err().contains("direction"));
        assert!(range_from_entries("MC10", from, to, Some(WorkDirection::Fixed), "two").is_err());
    }

    #[test]
    fn the_playback_timeline_equals_the_single_date_states() {
        use afrobim_schedule::{element_states, element_timelines};
        let mut doc = Document::default();
        for (cat, name) in [("drain", "SD-1"), ("drain", "SD-2"), ("culvert", "CV-1"), ("sign", "S-1")] {
            doc.add(cat, name, Pt::new(0.0, 0.0), 10.0, 10.0, "0", serde_json::Value::Null);
        }
        let p = sample();
        let s = afrobim_schedule::schedule(&p).unwrap();
        let (from, to) = (s.project_start.add_days(-3), s.project_finish.add_days(3));
        let pb = element_timelines(&p, &s, &doc, from, to);
        let mut checked = 0;
        let mut day = from;
        while day <= to {
            let single = element_states(&p, &s, &doc, day);
            let played = states_at(&pb, day);
            assert_eq!(single.states.len(), played.states.len(), "{day}");
            for e in &single.states {
                let q = played.get(e.element).expect("the same elements are linked");
                assert_eq!((q.phase, q.present), (e.phase, e.present), "{day} {:?}", e.element);
                checked += 1;
            }
            assert_eq!(single.unlinked, played.unlinked);
            day = day.add_days(1);
        }
        assert!(checked > 20, "{checked}");
    }

    #[test]
    fn playback_redraws_only_on_the_dates_something_changes() {
        use afrobim_schedule::element_timelines;
        let mut doc = Document::default();
        for (cat, name) in [("drain", "SD-1"), ("culvert", "CV-1")] {
            doc.add(cat, name, Pt::new(0.0, 0.0), 10.0, 10.0, "0", serde_json::Value::Null);
        }
        let p = sample();
        let s = afrobim_schedule::schedule(&p).unwrap();
        let (from, to) = (s.project_start.add_days(-2), s.project_finish.add_days(2));
        let pb = element_timelines(&p, &s, &doc, from, to);
        let mut held = states_update(None, &pb, from).expect("the first date always draws");
        let mut redraws = 0;
        let mut day = from.add_days(1);
        while day <= to {
            if let Some(next) = states_update(Some(&held), &pb, day) {
                // A redraw only on a date in the list, and it shows the state of that date.
                assert!(pb.change_dates.contains(&day) || pb.change_dates.iter().any(|d| *d > held.date && *d <= day), "{day}");
                held = next;
                redraws += 1;
            } else {
                // Nothing changed: the held state is the state of this day too.
                let truth = states_at(&pb, day);
                assert!(held.states.iter().all(|e| truth.get(e.element).is_some_and(|q| (q.phase, q.present) == (e.phase, e.present))), "{day}");
            }
            day = day.add_days(1);
        }
        assert!(redraws > 0 && redraws <= pb.change_dates.len(), "{redraws} redraws, {} change dates", pb.change_dates.len());
        // Going back in time always draws.
        assert!(states_update(Some(&held), &pb, from).is_some());
    }

    #[test]
    fn dates_convert_to_the_cost_crate_and_back() {
        let x = d(2026, 12, 31);
        assert_eq!(cost_date(x).to_string(), "2026-12-31");
        assert_eq!(from_cost_date(cost_date(x)), x);
    }
}
