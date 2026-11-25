// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! 4D playback: how every linked element's state changes over a range of dates, worked out in one
//! pass so an animation does not reschedule or re-resolve the links for each frame.

use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

use crate::cpm::Schedule;
use crate::date::Date;
use crate::links::{candidate_dates, element_state_at};
use crate::model::Programme;
use crate::Phase;

/// What an element looks like from `date` (the end of that day) until the next change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StateChange {
    pub date: Date,
    pub phase: Phase,
    /// Whether the element physically exists (fully or in part): the viewer hides it when false.
    pub present: bool,
}

/// One element's states over the range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementTimeline {
    pub element: ElementId,
    /// The state at the end of the first date of the range.
    pub initial: StateChange,
    /// Later changes only, in date order. Each entry holds from its date until the next one. An
    /// element in progress changes on every working day of its activity (the fraction moves).
    pub changes: Vec<StateChange>,
}

impl ElementTimeline {
    /// The state at the end of `date`: the last entry on or before it. `None` before the range.
    pub fn at(&self, date: Date) -> Option<&StateChange> {
        if date < self.initial.date {
            return None;
        }
        match self.changes.partition_point(|c| c.date <= date) {
            0 => Some(&self.initial),
            k => Some(&self.changes[k - 1]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playback {
    pub from: Date,
    pub to: Date,
    /// Linked elements, in document order.
    pub elements: Vec<ElementTimeline>,
    /// Every date in `(from, to]` on which at least one element changes, ascending. A player only
    /// needs to redraw on these dates (and at `from`).
    pub change_dates: Vec<Date>,
    /// Elements no link reaches (the schedule element excluded).
    pub unlinked: Vec<ElementId>,
}

impl Playback {
    pub fn timeline(&self, id: ElementId) -> Option<&ElementTimeline> {
        self.elements.iter().find(|t| t.element == id)
    }
}

/// The state changes of every linked element from `from` to `to`, both inclusive (`to` before
/// `from` gives an empty range). The result agrees, date by date, with calling
/// [`crate::element_states`] on each date of the range: [`ElementTimeline::at`] returns exactly the
/// phase and presence it reports.
///
/// Cost: one state evaluation per element per working day of its activities inside the range, not
/// per calendar day of the range.
pub fn element_timelines(p: &Programme, s: &Schedule, doc: &Document, from: Date, to: Date) -> Playback {
    let mut elements = Vec::new();
    let mut unlinked = Vec::new();
    let mut change_dates: Vec<Date> = Vec::new();
    if to < from {
        return Playback { from, to, elements, change_dates, unlinked };
    }
    for e in doc.elements() {
        if e.category == crate::SCHEDULE_CATEGORY {
            continue;
        }
        let Some(first) = element_state_at(p, s, e, from) else {
            unlinked.push(e.id);
            continue;
        };
        let initial = StateChange { date: from, phase: first.phase, present: first.present };
        let mut last = initial;
        let mut changes = Vec::new();
        for d in candidate_dates(p, s, e) {
            if d <= from || d > to {
                continue;
            }
            if let Some(st) = element_state_at(p, s, e, d) {
                if st.phase != last.phase || st.present != last.present {
                    last = StateChange { date: d, phase: st.phase, present: st.present };
                    changes.push(last);
                    change_dates.push(d);
                }
            }
        }
        elements.push(ElementTimeline { element: e.id, initial, changes });
    }
    change_dates.sort();
    change_dates.dedup();
    Playback { from, to, elements, change_dates, unlinked }
}
