//! 4D: the state of each linked model element on a given date.

use afrobim_kernel::{Document, Element, ElementId};
use serde::{Deserialize, Serialize};

use crate::cpm::Schedule;
use crate::date::Date;
use crate::model::{ElementRule, ElementSelector, LinkAction, Programme};

/// Where an element stands. `InProgress` carries the fraction (0 to 1, exclusive) of the linked
/// activity's working days elapsed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    NotStarted,
    InProgress { fraction: f64 },
    Complete,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkState {
    pub activity: String,
    pub action: LinkAction,
    pub phase: Phase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementState {
    pub element: ElementId,
    /// The state the element shows: that of the latest-starting link that has started, else of the
    /// earliest link.
    pub phase: Phase,
    /// Whether the element physically exists on this date (fully or in part): the app hides it
    /// when false.
    pub present: bool,
    pub links: Vec<LinkState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementStates {
    pub date: Date,
    pub states: Vec<ElementState>,
    /// Elements no link reaches (the schedule element itself excluded).
    pub unlinked: Vec<ElementId>,
}

impl ElementStates {
    pub fn get(&self, id: ElementId) -> Option<&ElementState> {
        self.states.iter().find(|s| s.element == id)
    }
}

/// A `*` / `?` pattern match, case-sensitive.
fn glob(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|k| glob(rest, &text[k..])),
        Some(('?', rest)) => !text.is_empty() && glob(rest, &text[1..]),
        Some((c, rest)) => text.first() == Some(c) && glob(rest, &text[1..]),
    }
}

impl ElementRule {
    pub fn matches(&self, e: &Element) -> bool {
        if self.category.is_none() && self.layer.is_none() && self.mark_pattern.is_none() {
            return false;
        }
        if self.category.as_deref().is_some_and(|c| c != e.category) {
            return false;
        }
        if self.layer.as_deref().is_some_and(|l| l != e.layer) {
            return false;
        }
        if let Some(p) = &self.mark_pattern {
            let (p, t): (Vec<char>, Vec<char>) = (p.chars().collect(), e.name.chars().collect());
            if !glob(&p, &t) {
                return false;
            }
        }
        true
    }
}

fn selects(sel: &ElementSelector, e: &Element) -> bool {
    match sel {
        ElementSelector::Id(id) => *id == e.id,
        ElementSelector::Rule(r) => r.matches(e),
    }
}

enum Raw {
    Before,
    During(f64),
    After,
}

fn raw_state(p: &Programme, s: &Schedule, activity: &str, date: Date) -> Option<(Raw, Date)> {
    let row = s.row(activity)?;
    let (es, ef) = (row.early_start, row.early_finish);
    let raw = if date < es {
        Raw::Before
    } else if date >= ef {
        Raw::After
    } else {
        let cal = p.calendar_named(&row.calendar)?;
        let total = cal.wd_between(es, ef.add_days(1)).max(1) as f64;
        let done = cal.wd_between(es, date.add_days(1)) as f64;
        Raw::During((done / total).clamp(0.0, 1.0))
    };
    Some((raw, es))
}

fn phase_of(action: LinkAction, raw: &Raw) -> (Phase, bool) {
    match (action, raw) {
        (LinkAction::Construct, Raw::Before) => (Phase::NotStarted, false),
        (LinkAction::Construct, Raw::During(f)) => (Phase::InProgress { fraction: *f }, true),
        (LinkAction::Construct, Raw::After) => (Phase::Complete, true),
        (LinkAction::Demolish, Raw::Before) => (Phase::NotStarted, true),
        (LinkAction::Demolish, Raw::During(f)) => (Phase::InProgress { fraction: *f }, true),
        (LinkAction::Demolish, Raw::After) => (Phase::Removed, false),
        (LinkAction::Temporary, Raw::Before) => (Phase::NotStarted, false),
        (LinkAction::Temporary, Raw::During(f)) => (Phase::InProgress { fraction: *f }, true),
        (LinkAction::Temporary, Raw::After) => (Phase::Removed, false),
    }
}

/// The state of every linked element at the end of `date`.
///
/// An activity has not started when `date` is before its early start, is complete when `date` is on
/// or after its early finish, and is in progress between, with fraction
/// `working days from start to date inclusive / working days of the activity`. A one-day
/// activity therefore jumps from not started to complete. The schedule's early dates are the
/// actual dates for work already done.
pub fn element_states(p: &Programme, s: &Schedule, doc: &Document, date: Date) -> ElementStates {
    let mut states = Vec::new();
    let mut unlinked = Vec::new();
    for e in doc.elements() {
        if e.category == crate::SCHEDULE_CATEGORY {
            continue;
        }
        match element_state_at(p, s, e, date) {
            Some(st) => states.push(st),
            None => unlinked.push(e.id),
        }
    }
    ElementStates { date, states, unlinked }
}

/// The state of one element at the end of `date`, or `None` when no link reaches it (or the
/// links name activities the schedule does not have). [`element_states`] runs this per element,
/// and so does the 4D playback ([`crate::element_timelines`]).
pub fn element_state_at(p: &Programme, s: &Schedule, e: &Element, date: Date) -> Option<ElementState> {
    // (activity start, link state, present, started)
    let mut hits: Vec<(Date, LinkState, bool, bool)> = Vec::new();
    for l in p.links.iter().filter(|l| selects(&l.selector, e)) {
        if let Some((raw, start)) = raw_state(p, s, &l.activity, date) {
            let (phase, present) = phase_of(l.action, &raw);
            let started = !matches!(raw, Raw::Before);
            hits.push((start, LinkState { activity: l.activity.clone(), action: l.action, phase }, present, started));
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by_key(|h| h.0);
    let chosen = hits.iter().rev().find(|h| h.3).unwrap_or(&hits[0]);
    Some(ElementState { element: e.id, phase: chosen.1.phase, present: chosen.2, links: hits.iter().map(|h| h.1.clone()).collect() })
}

/// The dates at which an element's state could change: for each linked activity, its early start,
/// its early finish and every working day between. The state can only differ from the day before
/// on one of these.
pub(crate) fn candidate_dates(p: &Programme, s: &Schedule, e: &Element) -> Vec<Date> {
    let mut out = Vec::new();
    for l in p.links.iter().filter(|l| selects(&l.selector, e)) {
        let Some(row) = s.row(&l.activity) else { continue };
        out.push(row.early_start);
        out.push(row.early_finish);
        if let Some(cal) = p.calendar_named(&row.calendar) {
            out.extend(cal.working_days_inclusive(row.early_start, row.early_finish));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The elements (not the schedule element) that at least one link selects, by id.
pub(crate) fn selects_element(p: &Programme, activity: &str, e: &Element) -> bool {
    p.links.iter().any(|l| l.activity == activity && selects(&l.selector, e))
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LinkReport {
    /// Elements no link reaches.
    pub unlinked_elements: Vec<ElementId>,
    /// Activities no link points to.
    pub activities_without_links: Vec<String>,
    /// Links by element id to an element the document does not have: (activity, id).
    pub missing_elements: Vec<(String, ElementId)>,
    /// Indices into `Programme::links` of rules that match no element.
    pub empty_rules: Vec<usize>,
}

/// What the links do and do not cover.
pub fn link_report(p: &Programme, doc: &Document) -> LinkReport {
    let mut r = LinkReport::default();
    for e in doc.elements() {
        if e.category != crate::SCHEDULE_CATEGORY && !p.links.iter().any(|l| selects(&l.selector, e)) {
            r.unlinked_elements.push(e.id);
        }
    }
    for a in &p.activities {
        if !p.links.iter().any(|l| l.activity == a.id) {
            r.activities_without_links.push(a.id.clone());
        }
    }
    for (k, l) in p.links.iter().enumerate() {
        match &l.selector {
            ElementSelector::Id(id) => {
                if doc.find(*id).is_none() {
                    r.missing_elements.push((l.activity.clone(), *id));
                }
            }
            ElementSelector::Rule(_) => {
                if !doc.elements().iter().any(|e| e.category != crate::SCHEDULE_CATEGORY && selects(&l.selector, e)) {
                    r.empty_rules.push(k);
                }
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_patterns() {
        let g = |p: &str, t: &str| glob(&p.chars().collect::<Vec<_>>(), &t.chars().collect::<Vec<_>>());
        assert!(g("DR-*", "DR-01"));
        assert!(g("DR-0?", "DR-01"));
        assert!(!g("DR-0?", "DR-011"));
        assert!(g("*", ""));
        assert!(!g("dr-*", "DR-01"));
        assert!(g("*-01", "DR-01"));
    }
}
