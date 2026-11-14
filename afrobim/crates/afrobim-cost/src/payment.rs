//! Interim payment support, kept generic.
//!
//! For a period: the quantity done to date per item (entered, or worked out from the percent
//! complete of each element), the quantity at the previous certificate, the quantity this period
//! and the amount of each, then the rules the user entered (retention, recovery of an advance
//! payment, other percentages and lump sums) and the net amount for the period.
//!
//! This module encodes no contract terms. It has no clause numbers and no percentages: every
//! percentage, cap, threshold and amount is entered by the user in [`PaymentTerms`], from the
//! terms of their own contract. It computes arithmetic on those entries and nothing else; it does
//! not decide what a contract entitles anyone to.

use std::collections::BTreeMap;

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::{CostError, Decimal, EstimateResult, Result, Rounding, RoundingRules};

/// Whether a rule adds to or deducts from the amount payable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Add,
    Deduct,
}

/// What a percentage rule is a percentage of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrossBasis {
    /// The gross value of work to date; the rule's amount to date is that percentage and the
    /// amount for the period is the change.
    ToDate,
    /// The gross value of this period; amounts to date accumulate.
    ThisPeriod,
}

/// A rule the user entered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaymentRuleKind {
    /// Retention: a percentage of the gross value to date, held back, up to `cap` in total when one is entered.
    Retention { percent: Decimal, cap: Option<Decimal> },
    /// Recovery of an advance payment of `advance`: `percent` of the gross value to date above
    /// `start_threshold`, up to the advance in total. Deducts.
    AdvanceRecovery { advance: Decimal, percent: Decimal, start_threshold: Decimal },
    /// Any other percentage of the gross value.
    PercentOfGross { percent: Decimal, basis: GrossBasis, effect: Effect },
    /// A fixed amount in this period only.
    Lump { amount: Decimal, effect: Effect },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentRule {
    /// Unique within the terms; the state carried to the next certificate is kept by it.
    pub name: String,
    pub kind: PaymentRuleKind,
}

impl PaymentRule {
    fn effect(&self) -> Effect {
        match &self.kind {
            PaymentRuleKind::Retention { .. } | PaymentRuleKind::AdvanceRecovery { .. } => Effect::Deduct,
            PaymentRuleKind::PercentOfGross { effect, .. } | PaymentRuleKind::Lump { effect, .. } => *effect,
        }
    }
}

/// The user's payment terms: rules in the order they are shown.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PaymentTerms {
    pub rules: Vec<PaymentRule>,
}

/// What the next certificate needs from this one.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PaymentState {
    pub label: String,
    pub gross_to_date: Decimal,
    /// Quantity to date by item code.
    pub quantities: BTreeMap<String, Decimal>,
    /// Amount to date by item code.
    pub amounts: BTreeMap<String, Decimal>,
    /// Cumulative amount of each rule, by rule name.
    pub rule_cumulative: BTreeMap<String, Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateLine {
    pub code: String,
    pub description: String,
    pub unit: String,
    /// The rate in the reporting currency.
    pub rate: Decimal,
    pub bill_quantity: Decimal,
    pub quantity_to_date: Decimal,
    pub quantity_previous: Decimal,
    pub quantity_this_period: Decimal,
    pub amount_to_date: Decimal,
    pub amount_previous: Decimal,
    pub amount_this_period: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleOutcome {
    pub name: String,
    pub effect: Effect,
    pub to_date: Decimal,
    pub previous: Decimal,
    pub this_period: Decimal,
}

/// One interim certificate worked out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Certificate {
    pub label: String,
    pub currency: String,
    pub decimals: u32,
    pub lines: Vec<CertificateLine>,
    pub gross_to_date: Decimal,
    pub gross_previous: Decimal,
    pub gross_this_period: Decimal,
    pub rules: Vec<RuleOutcome>,
    /// Gross for the period plus additions minus deductions for the period.
    pub net_this_period: Decimal,
    /// The same to date.
    pub net_to_date: Decimal,
    pub warnings: Vec<String>,
    /// Pass this as `previous` to the next period.
    pub state: PaymentState,
}

/// Works out one interim certificate.
///
/// `quantities_to_date` maps item codes to the cumulative quantity done. An item not in the map
/// stays at its previous quantity (zero in the first period). The amount to date of an item is its
/// quantity to date times its reporting-currency rate, rounded by `rounding`; the amount for the
/// period is the difference of amounts to date, so the periods always add up to the amount to date.
/// Items that could not be priced (no rate) are not certifiable and are refused when given a quantity.
pub fn certify(
    result: &EstimateResult,
    terms: &PaymentTerms,
    label: &str,
    quantities_to_date: &BTreeMap<String, Decimal>,
    previous: Option<&PaymentState>,
    rounding: Rounding,
) -> Result<Certificate> {
    let mut names = std::collections::BTreeSet::new();
    for r in &terms.rules {
        if r.name.trim().is_empty() || !names.insert(r.name.as_str()) {
            return Err(CostError::Invalid(format!("payment rule name '{}' is empty or used twice", r.name)));
        }
    }
    for code in quantities_to_date.keys() {
        let Some(item) = result.item(code) else {
            return Err(CostError::Invalid(format!("quantity given for item '{code}', which is not in the bill")));
        };
        if item.rate_reporting.is_none() {
            return Err(CostError::Invalid(format!("item '{code}' has no rate, so no amount can be certified for it")));
        }
    }
    let empty = PaymentState::default();
    let prev = previous.unwrap_or(&empty);
    let mut warnings = Vec::new();
    let mut lines = Vec::new();
    let mut state = PaymentState { label: label.to_string(), ..PaymentState::default() };

    for item in &result.items {
        let Some(rate) = item.rate_reporting else { continue };
        let q_prev = prev.quantities.get(&item.code).copied().unwrap_or_default();
        let q_to_date = quantities_to_date.get(&item.code).copied().unwrap_or(q_prev);
        let a_prev = prev.amounts.get(&item.code).copied().unwrap_or_default();
        let a_to_date = q_to_date.mul_rounded(rate, rounding);
        if q_to_date > item.quantity {
            warnings.push(format!("item {}: quantity to date {} exceeds the bill quantity {}", item.code, q_to_date, item.quantity));
        }
        if q_to_date < q_prev {
            warnings.push(format!("item {}: quantity to date {} is less than the previous {}", item.code, q_to_date, q_prev));
        }
        if !q_to_date.is_zero() || !q_prev.is_zero() {
            state.quantities.insert(item.code.clone(), q_to_date);
            state.amounts.insert(item.code.clone(), a_to_date);
        }
        lines.push(CertificateLine {
            code: item.code.clone(),
            description: item.description.clone(),
            unit: item.unit.clone(),
            rate,
            bill_quantity: item.quantity,
            quantity_to_date: q_to_date,
            quantity_previous: q_prev,
            quantity_this_period: q_to_date - q_prev,
            amount_to_date: a_to_date,
            amount_previous: a_prev,
            amount_this_period: a_to_date - a_prev,
        });
    }
    let gross_to_date: Decimal = lines.iter().map(|l| l.amount_to_date).sum();
    let gross_previous: Decimal = lines.iter().map(|l| l.amount_previous).sum();
    let gross_this_period = gross_to_date - gross_previous;
    state.gross_to_date = gross_to_date;

    let mut outcomes = Vec::new();
    for r in &terms.rules {
        let before = prev.rule_cumulative.get(&r.name).copied().unwrap_or_default();
        let to_date = match &r.kind {
            PaymentRuleKind::Retention { percent, cap } => {
                let held = gross_to_date.percent_of(*percent, rounding);
                cap.map_or(held, |c| held.min(c))
            }
            PaymentRuleKind::AdvanceRecovery { advance, percent, start_threshold } => {
                let over = (gross_to_date - *start_threshold).max(Decimal::ZERO);
                over.percent_of(*percent, rounding).min(*advance)
            }
            PaymentRuleKind::PercentOfGross { percent, basis: GrossBasis::ToDate, .. } => gross_to_date.percent_of(*percent, rounding),
            PaymentRuleKind::PercentOfGross { percent, basis: GrossBasis::ThisPeriod, .. } => before + gross_this_period.percent_of(*percent, rounding),
            PaymentRuleKind::Lump { amount, .. } => before + *amount,
        };
        state.rule_cumulative.insert(r.name.clone(), to_date);
        outcomes.push(RuleOutcome { name: r.name.clone(), effect: r.effect(), to_date, previous: before, this_period: to_date - before });
    }
    let signed = |o: &RuleOutcome, v: Decimal| if o.effect == Effect::Add { v } else { -v };
    let net_this_period = gross_this_period + outcomes.iter().map(|o| signed(o, o.this_period)).sum::<Decimal>();
    let net_to_date = gross_to_date + outcomes.iter().map(|o| signed(o, o.to_date)).sum::<Decimal>();

    Ok(Certificate {
        label: label.to_string(),
        currency: result.reporting.code.clone(),
        decimals: result.reporting.decimals,
        lines,
        gross_to_date,
        gross_previous,
        gross_this_period,
        rules: outcomes,
        net_this_period,
        net_to_date,
        warnings,
        state,
    })
}

/// Quantities to date from percent complete: for each item, the quantity of each of its elements
/// times that element's percent complete, plus the item's adjustments and manual quantity (which
/// belong to no element) times the item's own percent. An element or item absent from its map is
/// 0% complete. The result is rounded by the bill's quantity rule for the unit, ready for
/// [`certify`].
pub fn quantities_from_progress(
    result: &EstimateResult,
    element_percent: &BTreeMap<ElementId, Decimal>,
    item_percent: &BTreeMap<String, Decimal>,
    rules: &RoundingRules,
) -> Result<BTreeMap<String, Decimal>> {
    let hundred = Decimal::from_int(100);
    let check = |p: Decimal, what: String| {
        if p < Decimal::ZERO || p > hundred {
            Err(CostError::Invalid(format!("percent complete of {what} is {p}; it must be between 0 and 100")))
        } else {
            Ok(())
        }
    };
    for (e, p) in element_percent {
        check(*p, format!("element {}", e.0))?;
    }
    for (c, p) in item_percent {
        check(*p, format!("item {c}"))?;
    }
    let exact = Rounding::half_up(crate::decimal::DECIMALS);
    let mut out = BTreeMap::new();
    for m in &result.measured.items {
        let own = item_percent.get(&m.item.code).copied().unwrap_or_default();
        let mut q = Decimal::ZERO;
        for c in &m.contributions {
            let pct = match c.element {
                Some(e) => element_percent.get(&e).copied().unwrap_or_default(),
                None => own,
            };
            q += c.quantity.percent_of(pct, exact);
        }
        let outside = m.unrounded - m.from_lines;
        q += outside.percent_of(own, exact);
        out.insert(m.item.code.clone(), q.round(rules.quantity_for(&m.item.unit)));
    }
    Ok(out)
}
