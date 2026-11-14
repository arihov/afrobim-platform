//! Link to interim payment: progress at a date as the `quantities_to_date` input of
//! [`afrobim_cost::certify`].

use std::collections::{BTreeMap, BTreeSet};

use afrobim_cost::{quantities_from_progress, Decimal, EstimateResult, RoundingRules};
use afrobim_kernel::ElementId;
use afrobim_schedule::Date;

use crate::error::Result;
use crate::record::ProgressLog;

/// The quantities to date of each bill item, ready for `certify`, and what could not be used.
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentQuantities {
    /// Item code to cumulative quantity: the `quantities_to_date` argument of `certify`.
    pub quantities: BTreeMap<String, Decimal>,
    /// Elements with progress above 0% on the date that feed no bill item, so they are not paid.
    pub elements_without_item: Vec<ElementId>,
}

/// Quantities to date from the progress log, through the cost crate's
/// [`quantities_from_progress`]: each item's quantity is, for each element it measures, the
/// element's quantity times its percent complete on `date`, plus the item's adjustments and manual
/// quantity times the percent the caller gives for that item (`item_percent`; absent means 0%).
/// Elements with no record count at 0%.
pub fn qty_to_date(
    result: &EstimateResult,
    log: &ProgressLog,
    date: Date,
    item_percent: &BTreeMap<String, Decimal>,
    rules: &RoundingRules,
) -> Result<PaymentQuantities> {
    let element_percent = log.percents_at(date)?;
    let quantities = quantities_from_progress(result, &element_percent, item_percent, rules)?;
    let fed: BTreeSet<ElementId> = result.measured.items.iter().flat_map(|m| m.elements()).collect();
    let elements_without_item = element_percent.iter().filter(|(e, p)| **p > Decimal::ZERO && !fed.contains(e)).map(|(e, _)| *e).collect();
    Ok(PaymentQuantities { quantities, elements_without_item })
}
