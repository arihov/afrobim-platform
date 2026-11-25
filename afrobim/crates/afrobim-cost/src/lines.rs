// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Quantity lines: the neutral record every collector produces and the bill reads.

use std::collections::BTreeMap;

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::{csv, Decimal};

/// Decimals a collected `f64` quantity keeps when it becomes a [`Decimal`] (millionths of a
/// unit: below any measurement).
pub const COLLECTED_DECIMALS: u32 = 6;

/// One measured quantity of one element: "marking MK-3 has 412.500 m of painted length".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityLine {
    /// Position in its [`QuantitySet`]; the audit trail refers to lines by it.
    pub id: usize,
    /// The element it was measured from; None when the quantity has no element in the document
    /// (a sign measured from a layout whose element cannot be found).
    pub element: Option<ElementId>,
    /// The element's mark or name, e.g. `MK-3`.
    pub mark: String,
    /// What was measured, as `family.quantity`, e.g. `marking.painted_area`. The collectors'
    /// categories are listed in the README.
    pub category: String,
    pub description: String,
    pub quantity: Decimal,
    /// `m`, `m2`, `m3`, `kg`, `nr`.
    pub unit: String,
    /// What distinguishes this line for mapping: marking type and width, pipe diameter, post
    /// section and so on. Values are text; numbers are written plainly (`150`).
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

/// An element the collectors could not measure, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedSource {
    /// Which collector: `marking`, `post`...
    pub source: String,
    pub mark: String,
    pub reason: String,
}

/// The lines of one model revision.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QuantitySet {
    pub lines: Vec<QuantityLine>,
    pub skipped: Vec<SkippedSource>,
}

impl QuantitySet {
    pub fn new() -> QuantitySet {
        QuantitySet::default()
    }

    #[allow(clippy::too_many_arguments)]
    /// Adds a line and returns its id. A non-finite quantity is not added; it is recorded as skipped.
    pub fn push(
        &mut self,
        element: Option<ElementId>,
        mark: &str,
        category: &str,
        description: &str,
        quantity: f64,
        unit: &str,
        attributes: &[(&str, String)],
    ) -> Option<usize> {
        let q = match Decimal::from_f64(quantity, COLLECTED_DECIMALS) {
            Ok(q) => q,
            Err(e) => {
                self.skipped.push(SkippedSource { source: category.to_string(), mark: mark.to_string(), reason: e.to_string() });
                return None;
            }
        };
        Some(self.push_decimal(element, mark, category, description, q, unit, attributes))
    }

    #[allow(clippy::too_many_arguments)]
    /// Adds a line with an exact quantity (a count, for instance).
    pub fn push_decimal(
        &mut self,
        element: Option<ElementId>,
        mark: &str,
        category: &str,
        description: &str,
        quantity: Decimal,
        unit: &str,
        attributes: &[(&str, String)],
    ) -> usize {
        let id = self.lines.len();
        self.lines.push(QuantityLine {
            id,
            element,
            mark: mark.to_string(),
            category: category.to_string(),
            description: description.to_string(),
            quantity,
            unit: unit.to_string(),
            attributes: attributes.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        });
        id
    }

    /// Appends another set, renumbering its lines.
    pub fn extend(&mut self, other: QuantitySet) {
        let base = self.lines.len();
        for mut l in other.lines {
            l.id += base;
            self.lines.push(l);
        }
        self.skipped.extend(other.skipped);
    }

    /// Totals by category and unit, in category order.
    pub fn totals_by_category(&self) -> Vec<(String, String, Decimal)> {
        let mut map: BTreeMap<(String, String), Decimal> = BTreeMap::new();
        for l in &self.lines {
            *map.entry((l.category.clone(), l.unit.clone())).or_default() += l.quantity;
        }
        map.into_iter().map(|((c, u), q)| (c, u, q)).collect()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// The header of [`lines_to_csv`].
pub const LINES_CSV_HEADER: &str = "line,element_id,mark,category,description,quantity,unit,attributes";

/// Every line as CSV; attributes as `key=value;key=value`.
pub fn lines_to_csv(set: &QuantitySet) -> String {
    let mut out = String::from(LINES_CSV_HEADER);
    out.push('\n');
    for l in &set.lines {
        let attrs = l.attributes.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(";");
        out.push_str(&csv::row(&[
            l.id.to_string(),
            l.element.map(|e| e.0.to_string()).unwrap_or_default(),
            l.mark.clone(),
            l.category.clone(),
            l.description.clone(),
            l.quantity.to_string(),
            l.unit.clone(),
            attrs,
        ]));
    }
    out
}

/// Unit text compared as the bill compares it: trimmed, case-insensitive.
pub(crate) fn same_unit(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}
