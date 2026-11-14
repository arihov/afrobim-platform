//! Time-phased cost: when the money is spent.
//!
//! The input is deliberately small and owned by this crate: for an item, an element or an
//! addition, a start date, a finish date and a distribution. The application joins it with
//! whatever schedule it holds (the schedule crate is not a dependency of this one): one
//! [`Phasing`] per scheduled task.
//!
//! The only distribution is linear by calendar day: the amount is spread evenly over every day
//! from start to finish, both included, and each month receives the share of its days in the
//! window. Each month's amount is rounded to the currency by the bill's rule, and the last month
//! takes the remainder so the months always add back to the amount exactly.

use std::collections::{BTreeMap, BTreeSet};

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::date::days_in_month;
use crate::{CostError, Date, Decimal, Estimate, EstimateResult, QuantitySet, Result, Rounding};

/// How an amount is spread between start and finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Distribution {
    /// Evenly by calendar day, start and finish included.
    #[default]
    Linear,
}

/// What a window applies to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PhasingTarget {
    /// The whole amount of a bill item (including its adjustments), by item code.
    Item(String),
    /// What one element contributes to measured items: its quantities times the items' rates. For
    /// items that are not themselves phased. What an element does not account for stays unphased.
    Element(ElementId),
    /// An addition's amount, by addition id.
    Addition(String),
}

/// One task's window: `item or element -> (start, finish, distribution)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phasing {
    pub target: PhasingTarget,
    pub start: Date,
    pub finish: Date,
    #[serde(default)]
    pub distribution: Distribution,
}

/// One month of the cash flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthRow {
    pub year: i32,
    pub month: u8,
    pub amount: Decimal,
    pub cumulative: Decimal,
    /// Cumulative as a percentage of the phased total, 2 decimals.
    pub cumulative_percent: Decimal,
}

impl MonthRow {
    pub fn month_end(&self) -> Date {
        Date::new(self.year, self.month, days_in_month(self.year, self.month)).expect("a month row holds a real month")
    }
}

/// A point of the S-curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SCurvePoint {
    pub date: Date,
    pub cumulative: Decimal,
    pub percent: Decimal,
}

/// The cash flow by month.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CashFlow {
    pub currency: String,
    /// Consecutive months from the first to the last with any phasing; months with nothing are zero.
    pub months: Vec<MonthRow>,
    /// Sum of the months.
    pub phased_total: Decimal,
    /// Grand total minus the phased total: whatever has no window.
    pub unphased_total: Decimal,
    pub grand_total: Decimal,
    pub warnings: Vec<String>,
}

impl CashFlow {
    /// Cumulative cost against time: a zero point on the day before the first month, then the
    /// end of each month.
    pub fn s_curve(&self) -> Vec<SCurvePoint> {
        let Some(first) = self.months.first() else { return Vec::new() };
        let start = Date::new(first.year, first.month, 1).expect("a month row holds a real month").add_days(-1);
        let mut out = vec![SCurvePoint { date: start, cumulative: Decimal::ZERO, percent: Decimal::ZERO }];
        out.extend(self.months.iter().map(|m| SCurvePoint { date: m.month_end(), cumulative: m.cumulative, percent: m.cumulative_percent }));
        out
    }
}

/// Spreads `amount` over `start..=finish`: per calendar month, rounded by `rounding`, the last
/// month taking the remainder. Months are in order and only those the window touches.
pub fn spread_linear(amount: Decimal, start: Date, finish: Date, rounding: Rounding) -> Result<Vec<((i32, u8), Decimal)>> {
    if finish < start {
        return Err(CostError::Invalid(format!("the window finishes ({finish}) before it starts ({start})")));
    }
    let total_days = start.days_until(finish) + 1;
    let mut parts: Vec<((i32, u8), i64)> = Vec::new();
    let mut day = start;
    while day <= finish {
        let month_last = day.end_of_month().min(finish);
        let n = day.days_until(month_last) + 1;
        parts.push(((day.year(), day.month()), n));
        day = month_last.add_days(1);
    }
    let mut out = Vec::new();
    let mut given = Decimal::ZERO;
    let last = parts.len() - 1;
    for (i, (key, n)) in parts.into_iter().enumerate() {
        let a = if i == last { amount - given } else { amount.mul_ratio(i128::from(n), i128::from(total_days), rounding) };
        given += a;
        out.push((key, a));
    }
    Ok(out)
}

/// The cash flow of a priced estimate for the given windows. `rounding` is the rounding of an
/// amount in the reporting currency.
pub fn cash_flow_with(result: &EstimateResult, phasing: &[Phasing], rounding: Rounding) -> Result<CashFlow> {
    let mut warnings = Vec::new();
    let mut seen: BTreeSet<&PhasingTarget> = BTreeSet::new();
    for p in phasing {
        if !seen.insert(&p.target) {
            return Err(CostError::Invalid(format!("{:?} has more than one window", p.target)));
        }
    }
    let direct: BTreeSet<&str> = phasing.iter().filter_map(|p| if let PhasingTarget::Item(c) = &p.target { Some(c.as_str()) } else { None }).collect();

    let mut windows: Vec<(Decimal, &Phasing)> = Vec::new();
    for p in phasing {
        let amount = match &p.target {
            PhasingTarget::Item(code) => {
                result.item(code).ok_or_else(|| CostError::Invalid(format!("phasing refers to item '{code}', which is not in the bill")))?.amount_reporting
            }
            PhasingTarget::Addition(id) => {
                result.additions.iter().find(|a| &a.id == id).ok_or_else(|| CostError::Invalid(format!("phasing refers to addition '{id}', which does not exist")))?.amount
            }
            PhasingTarget::Element(id) => {
                let mut sum = Decimal::ZERO;
                for m in &result.measured.items {
                    let q = m.quantity_of(*id);
                    if q.is_zero() {
                        continue;
                    }
                    if direct.contains(m.item.code.as_str()) {
                        warnings.push(format!("element {} contributes to item {}, which is phased as a whole; its share is not phased separately", id.0, m.item.code));
                        continue;
                    }
                    if let Some(rate) = result.item(&m.item.code).and_then(|i| i.rate_reporting) {
                        sum += q.mul_rounded(rate, rounding);
                    }
                }
                sum
            }
        };
        windows.push((amount, p));
    }

    let mut by_month: BTreeMap<(i32, u8), Decimal> = BTreeMap::new();
    for (amount, p) in &windows {
        let Distribution::Linear = p.distribution;
        for (key, a) in spread_linear(*amount, p.start, p.finish, rounding)? {
            *by_month.entry(key).or_default() += a;
        }
    }
    let phased_total: Decimal = by_month.values().sum();
    let mut months = Vec::new();
    if let (Some(&first), Some(&last)) = (by_month.keys().next(), by_month.keys().next_back()) {
        let (mut y, mut m) = first;
        let mut cumulative = Decimal::ZERO;
        loop {
            let amount = by_month.get(&(y, m)).copied().unwrap_or_default();
            cumulative += amount;
            let percent = if phased_total.is_zero() {
                Decimal::ZERO
            } else {
                cumulative.mul_rounded(Decimal::from_int(100), Rounding::half_up(6)).div_rounded(phased_total, Rounding::half_up(2)).expect("non-zero")
            };
            months.push(MonthRow { year: y, month: m, amount, cumulative, cumulative_percent: percent });
            if (y, m) == last {
                break;
            }
            if m == 12 {
                y += 1;
                m = 1;
            } else {
                m += 1;
            }
        }
    }
    Ok(CashFlow {
        currency: result.reporting.code.clone(),
        months,
        phased_total,
        unphased_total: result.grand_total - phased_total,
        grand_total: result.grand_total,
        warnings,
    })
}

impl Estimate {
    /// The cash flow of the estimate's own phasing, rounded by the bill's amount rule.
    pub fn cash_flow(&self, result: &EstimateResult) -> Result<CashFlow> {
        let dec = self.currencies.currency(&self.reporting_currency)?.decimals;
        cash_flow_with(result, &self.phasing, self.bill.rounding.amount_for(dec))
    }

    /// Computes the estimate for `lines` and its cash flow.
    pub fn compute_with_cash_flow(&self, lines: &QuantitySet) -> Result<(EstimateResult, CashFlow)> {
        let r = self.compute(lines)?;
        let cf = self.cash_flow(&r)?;
        Ok((r, cf))
    }
}
