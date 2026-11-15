//! Applying the valid rows of a plan to a document.
//!
//! Everything the sign engine is needed for goes through [`SignBuilder`], which the app
//! implements: the plan itself never needs the engine.

use afrobim_kernel::{Document, ElementId};
use afrobim_signs::SignDefinition;

use super::plan::{ImportPlan, PlannedSign, RowOutcome};
use super::{PROP_DESCRIPTION, PROP_MOUNTING_HEIGHT_MM, PROP_NOTES, PROP_SIZE_MM, SIGN_CATEGORY, SIGN_LAYER};

/// What the app supplies to apply an import.
pub trait SignBuilder {
    /// The overall size of the sign's face in millimetres, `(width, height)`, for the
    /// definition: lay it out with the sign engine and take the extents scaled by the stroke
    /// width (`layout.drawing.scaled_to_origin(layout.sw_mm).extents`). An `Err` fails the row
    /// (nothing is added for it) and its text is reported.
    fn measure(&mut self, definition: &SignDefinition) -> Result<(f64, f64), String>;

    /// Called after a sign element has been created or updated and placed, so the app can do
    /// what it does for any placed sign (for example `afrobim_supports::sync_support_sites`).
    fn placed(&mut self, _doc: &mut Document, _id: ElementId) {}
}

/// What to do with a valid row whose mark is already in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DuplicatePolicy {
    /// Leave the existing sign alone and report the row as skipped.
    #[default]
    Skip,
    /// Replace the existing sign's definition, size, site placement and the schedule's
    /// properties with the row's. Its position on the design sheet, layer, host and any other
    /// properties are kept.
    Update,
}

/// What [`apply_plan`] did, row by row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ApplyReport {
    /// (row, id) of each sign added.
    pub created: Vec<(usize, ElementId)>,
    /// (row, id) of each existing sign updated.
    pub updated: Vec<(usize, ElementId)>,
    /// (row, mark, why) of each row not applied: a duplicate that was skipped, or an invalid row.
    pub skipped: Vec<(usize, String, String)>,
    /// (row, mark, why) of each valid row whose application failed (the builder refused it).
    pub failed: Vec<(usize, String, String)>,
}

/// Adds the plan's valid rows to the document and updates or skips duplicates as `policy` says.
/// Invalid rows are listed in the report as skipped. The document is changed only for rows that
/// succeed; there is no undo here, so a caller that wants one takes its snapshot first.
pub fn apply_plan(doc: &mut Document, plan: &ImportPlan, builder: &mut dyn SignBuilder, policy: DuplicatePolicy) -> ApplyReport {
    let mut report = ApplyReport::default();
    for row in &plan.rows {
        let planned = match &row.outcome {
            RowOutcome::Valid(p) => p,
            RowOutcome::Invalid(issues) => {
                let why = issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join("; ");
                report.skipped.push((row.row, row.mark.clone(), format!("not valid: {why}")));
                continue;
            }
        };
        match (planned.existing, policy) {
            (Some(_), DuplicatePolicy::Skip) => {
                report.skipped.push((row.row, planned.mark.clone(), "the mark is already in the document and duplicates are skipped".into()));
            }
            (Some(id), DuplicatePolicy::Update) => match update(doc, id, planned, builder) {
                Ok(()) => report.updated.push((row.row, id)),
                Err(m) => report.failed.push((row.row, planned.mark.clone(), m)),
            },
            (None, _) => match create(doc, planned, builder) {
                Ok(id) => report.created.push((row.row, id)),
                Err(m) => report.failed.push((row.row, planned.mark.clone(), m)),
            },
        }
    }
    report
}

fn data_of(planned: &PlannedSign) -> Result<serde_json::Value, String> {
    serde_json::to_value(&planned.definition).map_err(|e| format!("the sign definition cannot be stored: {e}"))
}

fn set_properties(e: &mut afrobim_kernel::Element, planned: &PlannedSign) {
    for key in [PROP_DESCRIPTION, PROP_MOUNTING_HEIGHT_MM, PROP_SIZE_MM, PROP_NOTES] {
        match planned.properties.get(key) {
            Some(v) => {
                e.properties.insert(key.to_string(), v.clone());
            }
            None => {
                e.properties.remove(key);
            }
        }
    }
}

fn create(doc: &mut Document, planned: &PlannedSign, builder: &mut dyn SignBuilder) -> Result<ElementId, String> {
    let (w, h) = builder.measure(&planned.definition)?;
    let data = data_of(planned)?;
    let origin = doc.next_free_origin(w, h);
    let id = doc.add(SIGN_CATEGORY, &planned.mark, origin, w, h, SIGN_LAYER, data);
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(planned.site.clone());
        set_properties(e, planned);
    }
    builder.placed(doc, id);
    Ok(id)
}

fn update(doc: &mut Document, id: ElementId, planned: &PlannedSign, builder: &mut dyn SignBuilder) -> Result<(), String> {
    let (w, h) = builder.measure(&planned.definition)?;
    let data = data_of(planned)?;
    let e = doc.find_mut(id).ok_or("the existing sign no longer exists")?;
    e.data = data;
    e.width = w;
    e.height = h;
    e.site = Some(planned.site.clone());
    set_properties(e, planned);
    builder.placed(doc, id);
    Ok(())
}
