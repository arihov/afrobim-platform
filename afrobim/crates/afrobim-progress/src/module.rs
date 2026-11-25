// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The progress book as a model element: category `progress`, drawn as an S-curve.
//!
//! The element stores the records, the entered actual costs and, after [`refresh_progress`], the
//! last earned value figures and S-curve. `generate` sees only the element, so it draws and reports
//! what was stored by the last refresh.

use afrobim_cost::Rounding;
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt, Severity};
use afrobim_schedule::{Date, Programme};
use serde::{Deserialize, Serialize};

use crate::error::{invalid, ProgressError, Result};
use crate::evm::{ActualCosts, Budget, EacFormula, EvmEngine, EvmResult, EvmSetup, Figure};
use crate::record::ProgressLog;
use crate::report::{s_curve_drawing, SCurveData};
use crate::rollup::WeightSources;

/// The element category that holds a progress book in a document.
pub const PROGRESS_CATEGORY: &str = "progress";

/// The layer progress elements are put on.
pub const PROGRESS_LAYER: &str = "Progress";

/// Size of the drawn S-curve box, millimetres.
pub const CURVE_WIDTH_MM: f64 = 1000.0;
pub const CURVE_HEIGHT_MM: f64 = 500.0;

/// What the last refresh computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredReport {
    pub evm: EvmResult,
    pub curve: SCurveData,
}

/// Everything stored in a `progress` element.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProgressBook {
    pub log: ProgressLog,
    pub costs: ActualCosts,
    #[serde(default)]
    pub report: Option<StoredReport>,
}

impl ProgressBook {
    pub fn new() -> ProgressBook {
        ProgressBook::default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a progress book always serialises")
    }

    pub fn from_value(v: &serde_json::Value) -> std::result::Result<ProgressBook, String> {
        serde_json::from_value(v.clone()).map_err(|e| format!("not a valid progress book: {e}"))
    }
}

/// Draws the S-curve stored by the last refresh and reports the stored figures.
#[derive(Debug, Clone, Default)]
pub struct ProgressModule;

impl Module for ProgressModule {
    fn category(&self) -> &str {
        PROGRESS_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Site progress and earned value"
    }

    fn generate(&self, element: &Element) -> std::result::Result<Generated, String> {
        let book = ProgressBook::from_value(&element.data)?;
        let mut g = Generated::default();
        g.derived.insert("Progress records".into(), book.log.records().len().to_string());
        g.derived.insert("Actual cost entries".into(), book.costs.entries.len().to_string());
        let Some(rep) = &book.report else {
            g.notes.push(Note { severity: Severity::Info, code: "not_refreshed".into(), message: "no earned value report stored yet: run refresh_progress".into() });
            return Ok(g);
        };
        let (drawing, _labels) = s_curve_drawing(&rep.curve, CURVE_WIDTH_MM, CURVE_HEIGHT_MM);
        g.drawing = drawing;
        let e = &rep.evm;
        let d = e.decimals;
        let money = |f: &Figure<afrobim_cost::Decimal>| f.text(|v| v.to_fixed(d));
        let idx = |f: &Figure<afrobim_cost::Decimal>| f.text(|v| v.to_string());
        g.derived.insert("Data date".into(), e.data_date.to_string());
        g.derived.insert("BAC".into(), e.bac.to_fixed(d));
        g.derived.insert("PV".into(), e.pv.to_fixed(d));
        g.derived.insert("EV".into(), e.ev.to_fixed(d));
        g.derived.insert("AC".into(), money(&e.ac));
        g.derived.insert("SPI".into(), idx(&e.spi));
        g.derived.insert("CPI".into(), idx(&e.cpi));
        g.derived.insert(format!("EAC ({})", e.eac.formula), money(&e.eac.value));
        if e.ac.is_input_needed() {
            g.notes.push(Note { severity: Severity::Info, code: "ac_input_needed".into(), message: "actual cost has not been entered: cost indices and EAC need it".into() });
        }
        for n in &e.notes {
            g.notes.push(Note { severity: Severity::Warning, code: "evm_note".into(), message: n.clone() });
        }
        Ok(g)
    }
}

fn size_of(book: &ProgressBook) -> (f64, f64) {
    if book.report.is_some() {
        (CURVE_WIDTH_MM, CURVE_HEIGHT_MM)
    } else {
        (100.0, 20.0)
    }
}

/// Adds the progress book to the document as a `progress` element and returns its id.
pub fn add_progress(doc: &mut Document, name: &str, book: &ProgressBook) -> ElementId {
    let size = size_of(book);
    doc.ensure_layer(PROGRESS_LAYER);
    doc.add(PROGRESS_CATEGORY, name, Pt::new(0.0, 0.0), size.0, size.1, PROGRESS_LAYER, book.to_value())
}

/// Replaces the book stored in an existing `progress` element.
pub fn store_progress(doc: &mut Document, id: ElementId, book: &ProgressBook) -> Result<()> {
    let size = size_of(book);
    let e = doc.find_mut(id).ok_or_else(|| ProgressError::Invalid(format!("no element {}", id.0)))?;
    if e.category != PROGRESS_CATEGORY {
        return invalid(format!("element {} is a '{}', not a progress book", id.0, e.category));
    }
    e.data = book.to_value();
    e.width = size.0;
    e.height = size.1;
    Ok(())
}

/// Reads the book of a `progress` element.
pub fn load_progress(doc: &Document, id: ElementId) -> Result<ProgressBook> {
    let e = doc.find(id).ok_or_else(|| ProgressError::Invalid(format!("no element {}", id.0)))?;
    if e.category != PROGRESS_CATEGORY {
        return invalid(format!("element {} is a '{}', not a progress book", id.0, e.category));
    }
    ProgressBook::from_value(&e.data).map_err(ProgressError::Invalid)
}

/// What a refresh needs besides the document: the programme (with baseline), the weight sources,
/// the budget, the money rounding and the EAC formula.
#[derive(Debug, Clone)]
pub struct RefreshParams<'a> {
    pub programme: &'a Programme,
    pub weights: WeightSources<'a>,
    pub budget: &'a Budget,
    pub rounding: Rounding,
    pub eac: EacFormula,
}

/// Recomputes the earned value figures and S-curve at `data_date` from the book stored in the
/// element and the model in the document, stores them in the element, and returns them.
pub fn refresh_progress(doc: &mut Document, id: ElementId, p: &RefreshParams, data_date: Date) -> Result<StoredReport> {
    let mut book = load_progress(doc, id)?;
    let report = {
        let setup = EvmSetup { programme: p.programme, document: doc, weights: p.weights, budget: p.budget, costs: &book.costs, rounding: p.rounding, eac: p.eac.clone() };
        let engine = EvmEngine::new(setup)?;
        StoredReport { evm: engine.evm(&book.log, data_date)?, curve: engine.s_curve(&book.log, data_date)? }
    };
    book.report = Some(report.clone());
    store_progress(doc, id, &book)?;
    Ok(report)
}
