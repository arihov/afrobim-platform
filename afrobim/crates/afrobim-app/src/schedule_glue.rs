//! The construction programme in the app: where it is kept, how model elements are offered to
//! links, and the conversion between the date types of the schedule and cost crates.
//!
//! The programme is one document element of category `schedule` (the schedule crate's
//! `add_programme`, `store_programme` and `load_programme`), so it is saved, opened and undone
//! with the document.

use afrobim_kernel::{Document, Element, ElementId};
use afrobim_schedule::{add_programme, load_programme, store_programme, Calendar, Date, Programme, Weekday, SCHEDULE_CATEGORY};

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

    #[test]
    fn dates_convert_to_the_cost_crate_and_back() {
        let x = d(2026, 12, 31);
        assert_eq!(cost_date(x).to_string(), "2026-12-31");
        assert_eq!(from_cost_date(cost_date(x)), x);
    }
}
