// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The estimate as a kernel element: category `"estimate"`, `data` = the serialised
//! [`EstimateElement`] (the estimate the user entered and, after a refresh, a summary of its last
//! computation), and the helpers that put one in a document.
//!
//! `generate` draws a simple summary: one bar per section of the bill, as long as the section is
//! costly, and writes the totals as derived values. The quantity lines are not stored in the
//! element; they are collected from the document again at each refresh.

use std::collections::BTreeMap;

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_fabrication::SignQuantity;
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Severity};
use serde::{Deserialize, Serialize};

use crate::collect::{collect_all, CollectOptions};
use crate::{CostError, Estimate, EstimateResult, Result};

/// The element category this crate owns.
pub const ESTIMATE_CATEGORY: &str = "estimate";

/// The layer estimates are put on.
pub const ESTIMATE_LAYER: &str = "Estimates";

/// Length of the longest bar of the summary drawing, mm.
pub const SUMMARY_BAR_MAX_MM: f64 = 2000.0;
const BAR_HEIGHT_MM: f64 = 200.0;
const BAR_GAP_MM: f64 = 100.0;

/// One section in the stored summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionSummary {
    pub code: String,
    pub title: String,
    /// In the reporting currency, as text with the currency's decimals.
    pub amount: String,
    /// The amount in the secondary currency, when there is one.
    pub amount_secondary: Option<String>,
    /// Bar length weight: the amount as a plain number (display only).
    pub weight: f64,
}

/// What the last computation said; stored so the element can draw and report without the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimateSummary {
    pub currency: String,
    pub secondary_currency: Option<String>,
    pub sections: Vec<SectionSummary>,
    pub subtotal: String,
    pub grand_total: String,
    pub grand_total_secondary: Option<String>,
    pub items: usize,
    pub unpriced_items: usize,
    pub lines_offered: usize,
    pub unmapped_lines: usize,
    pub skipped_sources: usize,
}

impl EstimateSummary {
    pub fn of(r: &EstimateResult, skipped_sources: usize) -> EstimateSummary {
        let rd = r.reporting.decimals;
        let sd = r.secondary.as_ref().map(|c| c.decimals);
        EstimateSummary {
            currency: r.reporting.code.clone(),
            secondary_currency: r.secondary.as_ref().map(|c| c.code.clone()),
            sections: r
                .sections
                .iter()
                .map(|s| SectionSummary {
                    code: s.code.clone(),
                    title: s.title.clone(),
                    amount: s.amount_reporting.to_fixed(rd),
                    amount_secondary: s.amount_secondary.zip(sd).map(|(a, d)| a.to_fixed(d)),
                    weight: s.amount_reporting.to_f64(),
                })
                .collect(),
            subtotal: r.subtotal.to_fixed(rd),
            grand_total: r.grand_total.to_fixed(rd),
            grand_total_secondary: r.grand_total_secondary.zip(sd).map(|(a, d)| a.to_fixed(d)),
            items: r.items.len(),
            unpriced_items: r.unpriced.len(),
            lines_offered: r.measured.lines_offered,
            unmapped_lines: r.measured.unmapped.len(),
            skipped_sources,
        }
    }
}

/// The data of an estimate element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimateElement {
    pub estimate: Estimate,
    #[serde(default)]
    pub computed: Option<EstimateSummary>,
}

/// The kernel module for estimates.
///
/// Derived values: `currency`, `items`, `unpriced_items`, `lines_offered`, `unmapped_lines`,
/// `skipped_sources`, `bill_total`, `grand_total`, `grand_total_<secondary currency>` when there is
/// one, and `section_<code>` per section; all amounts as text with the currency's decimals. Without
/// a computation only the estimate's name and an Info note are given.
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimateModule;

impl Module for EstimateModule {
    fn category(&self) -> &str {
        ESTIMATE_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Estimates"
    }

    fn generate(&self, element: &Element) -> std::result::Result<Generated, String> {
        let data: EstimateElement = serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}': {e}", element.name))?;
        let mut derived = BTreeMap::new();
        derived.insert("estimate".to_string(), data.estimate.name.clone());
        derived.insert("revision".to_string(), data.estimate.revision.clone());
        let Some(s) = &data.computed else {
            let note = Note { severity: Severity::Info, code: "estimate.not_computed".into(), message: "The estimate has not been computed from the model yet.".into() };
            return Ok(Generated { notes: vec![note], derived, ..Generated::default() });
        };
        let mut notes = Vec::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("currency", s.currency.clone());
        put("items", s.items.to_string());
        put("unpriced_items", s.unpriced_items.to_string());
        put("lines_offered", s.lines_offered.to_string());
        put("unmapped_lines", s.unmapped_lines.to_string());
        put("skipped_sources", s.skipped_sources.to_string());
        put("bill_total", s.subtotal.clone());
        put("grand_total", s.grand_total.clone());
        if let (Some(c), Some(v)) = (&s.secondary_currency, &s.grand_total_secondary) {
            put(&format!("grand_total_{c}"), v.clone());
        }
        for sec in &s.sections {
            put(&format!("section_{}", sec.code), sec.amount.clone());
        }
        if s.unmapped_lines > 0 {
            notes.push(Note {
                severity: Severity::Warning,
                code: "estimate.unmapped_lines".into(),
                message: format!("{} of {} quantity lines are not mapped to any item and are not priced.", s.unmapped_lines, s.lines_offered),
            });
        }
        if s.unpriced_items > 0 {
            notes.push(Note {
                severity: Severity::Warning,
                code: "estimate.unpriced_items".into(),
                message: format!("{} items have no usable rate and are priced at zero.", s.unpriced_items),
            });
        }
        if s.skipped_sources > 0 {
            notes.push(Note {
                severity: Severity::Warning,
                code: "estimate.skipped_sources".into(),
                message: format!("{} elements could not be measured and are not in the quantities.", s.skipped_sources),
            });
        }
        Ok(Generated { drawing: summary_drawing(s), notes, derived, solids: Vec::new() })
    }
}

fn summary_size(n: usize) -> (f64, f64) {
    let h = if n == 0 { BAR_HEIGHT_MM } else { n as f64 * BAR_HEIGHT_MM + (n - 1) as f64 * BAR_GAP_MM };
    (SUMMARY_BAR_MAX_MM, h)
}

/// One bar per section, top to bottom in bill order, the longest `SUMMARY_BAR_MAX_MM` long.
fn summary_drawing(s: &EstimateSummary) -> Drawing {
    let n = s.sections.len();
    let (w, h) = summary_size(n);
    let max = s.sections.iter().map(|x| x.weight).fold(0.0_f64, f64::max);
    let mut prims = vec![Primitive::Line { from: Pt2::new(0.0, 0.0), to: Pt2::new(0.0, h), width: 10.0, colour: None }];
    for (i, sec) in s.sections.iter().enumerate() {
        let len = if max > 0.0 { (sec.weight.max(0.0) / max * w).max(1.0) } else { 1.0 };
        let y = h - (i as f64 + 1.0) * BAR_HEIGHT_MM - i as f64 * BAR_GAP_MM;
        prims.push(Primitive::Fill {
            outline: vec![Pt2::new(0.0, y), Pt2::new(len, y), Pt2::new(len, y + BAR_HEIGHT_MM), Pt2::new(0.0, y + BAR_HEIGHT_MM)],
            colour: Colour::Blue,
        });
    }
    Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(w, h) } }
}

/// Adds an estimate to the document and returns its id. It goes on [`ESTIMATE_LAYER`] at the next
/// free place on the sheet.
pub fn add_estimate(doc: &mut Document, name: &str, estimate: &Estimate) -> Result<ElementId> {
    let data = EstimateElement { estimate: estimate.clone(), computed: None };
    let value = serde_json::to_value(&data).map_err(|e| CostError::Invalid(e.to_string()))?;
    let name = doc.unique_name(if name.trim().is_empty() { "Estimate" } else { name.trim() });
    let (w, h) = summary_size(estimate.bill.sections.len());
    let origin = doc.next_free_origin(w, h);
    Ok(doc.add(ESTIMATE_CATEGORY, &name, origin, w, h, ESTIMATE_LAYER, value))
}

/// The stored data of an estimate element.
pub fn estimate_of(doc: &Document, id: ElementId) -> Option<EstimateElement> {
    let e = doc.find(id).filter(|e| e.category == ESTIMATE_CATEGORY)?;
    serde_json::from_value(e.data.clone()).ok()
}

/// The estimates in the document.
pub fn estimates(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == ESTIMATE_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

/// Replaces the estimate an element holds (its stored summary is cleared: it is no longer current).
pub fn set_estimate(doc: &mut Document, id: ElementId, estimate: &Estimate) -> Result<()> {
    let Some(e) = doc.find_mut(id).filter(|e| e.category == ESTIMATE_CATEGORY) else {
        return Err(CostError::Invalid(format!("element {} is not an estimate", id.0)));
    };
    let data = EstimateElement { estimate: estimate.clone(), computed: None };
    e.data = serde_json::to_value(&data).map_err(|err| CostError::Invalid(err.to_string()))?;
    let (w, h) = summary_size(estimate.bill.sections.len());
    e.width = w;
    e.height = h;
    Ok(())
}

/// Collects the document's quantities, computes the estimate and stores the summary in the
/// element. `signs` are the sign take-off rows for the document's signs (see
/// [`crate::collect_all`]). Returns the full result for display, cash flow and payments.
pub fn refresh_estimate(doc: &mut Document, id: ElementId, signs: &[SignQuantity], options: &CollectOptions) -> Result<EstimateResult> {
    let data = estimate_of(doc, id).ok_or_else(|| CostError::Invalid(format!("element {} is not an estimate", id.0)))?;
    let set = collect_all(doc, signs, options);
    let result = data.estimate.compute(&set)?;
    let summary = EstimateSummary::of(&result, set.skipped.len());
    let stored = EstimateElement { estimate: data.estimate, computed: Some(summary) };
    let value = serde_json::to_value(&stored).map_err(|e| CostError::Invalid(e.to_string()))?;
    if let Some(e) = doc.find_mut(id) {
        e.data = value;
    }
    Ok(result)
}
