// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The programme as a model element: category `schedule`, drawn as a Gantt chart.

use std::collections::BTreeMap;

use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt};

use crate::cpm::{schedule, ScheduleError};
use crate::model::Programme;
use crate::output::{gantt_drawing, GanttOptions};

/// The element category that holds a programme in a document.
pub const SCHEDULE_CATEGORY: &str = "schedule";

/// Draws the Gantt chart of the programme in `element.data` and reports its findings as notes.
/// The chart's text labels are not part of [`Generated`]; call [`gantt_drawing`] for them.
#[derive(Debug, Clone, Default)]
pub struct ScheduleModule {
    pub options: GanttOptions,
}

impl Module for ScheduleModule {
    fn category(&self) -> &str {
        SCHEDULE_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Construction programme"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let p = Programme::from_value(&element.data)?;
        let s = match schedule(&p) {
            Ok(s) => s,
            Err(ScheduleError::Invalid(issues)) => {
                let notes = issues.into_iter().map(|i| Note { severity: i.severity, code: i.code, message: i.message }).collect();
                return Ok(Generated { notes, ..Generated::default() });
            }
            Err(e) => return Err(e.to_string()),
        };
        let g = gantt_drawing(&p, &s, &self.options);
        let notes = s.issues.iter().map(|i| Note { severity: i.severity, code: i.code.clone(), message: i.message.clone() }).collect();
        let mut derived = BTreeMap::new();
        derived.insert("Start".to_string(), s.project_start.to_string());
        derived.insert("Finish".to_string(), s.project_finish.to_string());
        derived.insert("Activities".to_string(), s.rows.len().to_string());
        derived.insert("Critical activities".to_string(), s.critical().count().to_string());
        if let Some(d) = s.data_date {
            derived.insert("Data date".to_string(), d.to_string());
        }
        Ok(Generated { drawing: g.drawing, notes, derived, solids: Vec::new() })
    }
}

/// Adds the programme to the document as a `schedule` element and returns its id.
pub fn add_programme(doc: &mut Document, name: &str, p: &Programme) -> ElementId {
    let size = size_of(p);
    doc.ensure_layer("Schedule");
    doc.add(SCHEDULE_CATEGORY, name, Pt::new(0.0, 0.0), size.0, size.1, "Schedule", p.to_value())
}

/// Replaces the programme stored in an existing `schedule` element.
pub fn store_programme(doc: &mut Document, id: ElementId, p: &Programme) -> Result<(), String> {
    let size = size_of(p);
    let e = doc.find_mut(id).ok_or_else(|| format!("no element {}", id.0))?;
    if e.category != SCHEDULE_CATEGORY {
        return Err(format!("element {} is a '{}', not a schedule", id.0, e.category));
    }
    e.data = p.to_value();
    e.width = size.0;
    e.height = size.1;
    Ok(())
}

/// Reads the programme of a `schedule` element.
pub fn load_programme(doc: &Document, id: ElementId) -> Result<Programme, String> {
    let e = doc.find(id).ok_or_else(|| format!("no element {}", id.0))?;
    if e.category != SCHEDULE_CATEGORY {
        return Err(format!("element {} is a '{}', not a schedule", id.0, e.category));
    }
    Programme::from_value(&e.data)
}

/// Stored size of the element: the Gantt extents when the programme schedules, else a placeholder.
fn size_of(p: &Programme) -> (f64, f64) {
    match schedule(p) {
        Ok(s) => {
            let g = gantt_drawing(p, &s, &GanttOptions::default());
            (g.drawing.extents.width(), g.drawing.extents.height())
        }
        Err(_) => (100.0, 20.0),
    }
}
