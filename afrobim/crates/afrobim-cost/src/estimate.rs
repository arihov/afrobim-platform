//! The estimate: a bill priced from the rate library, section and grand totals, the user's
//! additions in the order the user defined, and comparison of two estimates.

use std::collections::{BTreeMap, BTreeSet};

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::lines::same_unit;
use crate::{
    AmountRounding, Bill, CostError, Currency, CurrencyTable, Date, Decimal, MeasuredBill, Money, Phasing, QuantitySet, RateLibrary, Result,
    Rounding,
};

/// What an addition's percentage is taken of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdditionBase {
    /// The bill total before any addition.
    BillTotal,
    /// The bill total plus every addition above this one in the list.
    RunningTotal,
    /// The total of one section, by section code.
    Section(String),
    /// The sum of the named additions above this one, by id.
    Additions(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdditionKind {
    /// A percentage of a stated base. Negative for a discount.
    Percentage { percent: Decimal, base: AdditionBase },
    /// A lump sum, in any defined currency.
    LumpSum(Money),
}

/// An addition to the bill total (preliminaries, contingency, a provisional sum, dayworks, VAT:
/// whatever the user names). They are applied in the order of the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Addition {
    /// A short unique key other additions refer to.
    pub id: String,
    /// The name shown.
    pub name: String,
    pub kind: AdditionKind,
}

/// An estimate: everything entered by the user. The quantities come from the model at compute time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    pub name: String,
    /// The user's revision label, e.g. `Rev B`.
    #[serde(default)]
    pub revision: String,
    pub bill: Bill,
    pub rates: RateLibrary,
    pub currencies: CurrencyTable,
    /// The currency of totals.
    pub reporting_currency: String,
    /// A second currency shown beside it (UGX with USD); needs an exchange rate.
    #[serde(default)]
    pub secondary_currency: Option<String>,
    /// Exchange rates in force on this date (the latest entered on or before it); the latest of all when None.
    #[serde(default)]
    pub as_of: Option<Date>,
    #[serde(default)]
    pub additions: Vec<Addition>,
    /// When the work is done, for the cash flow.
    #[serde(default)]
    pub phasing: Vec<Phasing>,
}

impl Estimate {
    /// An empty estimate reporting in `reporting`.
    pub fn new(name: &str, reporting: Currency) -> Estimate {
        let code = reporting.code.clone();
        let mut currencies = CurrencyTable::default();
        currencies.add_currency(reporting);
        Estimate {
            name: name.into(),
            revision: String::new(),
            bill: Bill::default(),
            rates: RateLibrary::default(),
            currencies,
            reporting_currency: code,
            secondary_currency: None,
            as_of: None,
            additions: Vec::new(),
            phasing: Vec::new(),
        }
    }
}

/// An item priced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricedItem {
    pub code: String,
    pub section: String,
    pub description: String,
    pub unit: String,
    pub quantity: Decimal,
    /// The rate used and its currency; None when the item is not priced.
    pub rate: Option<Decimal>,
    pub rate_currency: Option<String>,
    /// `quantity * rate`, in the rate's currency (zero when not priced).
    pub amount: Decimal,
    /// The decimals of the rate's currency, for printing `amount`.
    pub native_decimals: u32,
    /// The amount in the reporting currency.
    pub amount_reporting: Decimal,
    /// The amount in the secondary currency, when there is one.
    pub amount_secondary: Option<Decimal>,
    /// The rate in the reporting currency at 6 decimals, for payments and element costs.
    pub rate_reporting: Option<Decimal>,
}

/// An item left unpriced and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unpriced {
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionTotal {
    pub code: String,
    pub title: String,
    pub items: usize,
    pub amount_reporting: Decimal,
    pub amount_secondary: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionResult {
    pub id: String,
    pub name: String,
    /// The base amount for a percentage; None for a lump sum.
    pub base: Option<Decimal>,
    pub percent: Option<Decimal>,
    pub amount: Decimal,
    pub amount_secondary: Option<Decimal>,
    /// Bill total plus additions up to and including this one.
    pub running_total: Decimal,
}

/// A priced estimate with its audit trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimateResult {
    pub name: String,
    pub revision: String,
    pub reporting: Currency,
    pub secondary: Option<Currency>,
    /// The bill measured against the model: every item with its lines and elements, and the lines that went nowhere.
    pub measured: MeasuredBill,
    pub items: Vec<PricedItem>,
    pub unpriced: Vec<Unpriced>,
    pub sections: Vec<SectionTotal>,
    /// Native subtotal of the items by currency of their rate.
    pub by_currency: BTreeMap<String, Decimal>,
    /// The bill total before additions, in the reporting currency.
    pub subtotal: Decimal,
    pub subtotal_secondary: Option<Decimal>,
    pub additions: Vec<AdditionResult>,
    pub grand_total: Decimal,
    pub grand_total_secondary: Option<Decimal>,
}

impl EstimateResult {
    pub fn item(&self, code: &str) -> Option<&PricedItem> {
        self.items.iter().find(|i| i.code == code)
    }

    /// The cost of one element in the reporting currency: for each item, the element's share of
    /// the quantity times the item's reporting rate, rounded once per item. Adjustments and manual
    /// quantities belong to no element and are not included.
    pub fn element_cost(&self, element: ElementId, rounding: Rounding) -> Decimal {
        self.measured
            .items
            .iter()
            .filter_map(|m| {
                let priced = self.item(&m.item.code)?;
                let rate = priced.rate_reporting?;
                let q = m.quantity_of(element);
                (!q.is_zero()).then(|| q.mul_rounded(rate, rounding))
            })
            .sum()
    }
}

impl Estimate {
    fn check(&self) -> Result<()> {
        self.currencies.currency(&self.reporting_currency)?;
        if let Some(s) = &self.secondary_currency {
            self.currencies.currency(s)?;
        }
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for a in &self.additions {
            if a.id.trim().is_empty() {
                return Err(CostError::Invalid("an addition has no id".into()));
            }
            match &a.kind {
                AdditionKind::Percentage { base, .. } => match base {
                    AdditionBase::Section(c) if self.bill.section(c).is_none() => {
                        return Err(CostError::Invalid(format!("addition '{}' is a percentage of section '{c}', which does not exist", a.id)));
                    }
                    AdditionBase::Additions(list) => {
                        for r in list {
                            if !ids.contains(r.as_str()) {
                                return Err(CostError::Invalid(format!(
                                    "addition '{}' is a percentage of addition '{r}', which is not defined above it",
                                    a.id
                                )));
                            }
                        }
                    }
                    _ => {}
                },
                AdditionKind::LumpSum(m) => {
                    self.currencies.currency(&m.currency)?;
                }
            }
            if !ids.insert(a.id.as_str()) {
                return Err(CostError::Invalid(format!("addition id '{}' is used twice", a.id)));
            }
        }
        Ok(())
    }

    /// Measures the bill against `lines` and prices it.
    ///
    /// Fails when the bill structure is unsound, a currency is undefined, an addition refers to
    /// something that does not exist, or an exchange rate needed has not been entered. An item
    /// with no rate is not an error: it is priced at zero and listed in `unpriced`.
    pub fn compute(&self, lines: &QuantitySet) -> Result<EstimateResult> {
        self.check()?;
        let measured = self.bill.measure(lines)?;
        let rep = self.currencies.currency(&self.reporting_currency)?.clone();
        let sec = self.secondary_currency.as_deref().map(|c| self.currencies.currency(c)).transpose()?.cloned();
        let rules = &self.bill.rounding;
        let per_item = rules.amount_rounding == AmountRounding::PerItem;
        let work = |decimals: u32| if per_item { rules.amount_for(decimals) } else { Rounding::new(crate::decimal::DECIMALS, rules.amount_mode) };
        let rate_rounding = Rounding::new(crate::decimal::DECIMALS, rules.amount_mode);

        let mut items = Vec::new();
        let mut unpriced = Vec::new();
        let mut by_currency: BTreeMap<String, Decimal> = BTreeMap::new();
        for m in &measured.items {
            let it = &m.item;
            let zero = |reason: Option<String>, unpriced: &mut Vec<Unpriced>| {
                if let Some(reason) = reason {
                    unpriced.push(Unpriced { code: it.code.clone(), reason });
                }
                PricedItem {
                    code: it.code.clone(),
                    section: it.section.clone(),
                    description: it.description.clone(),
                    unit: it.unit.clone(),
                    quantity: m.quantity,
                    rate: None,
                    rate_currency: None,
                    amount: Decimal::ZERO,
                    native_decimals: rep.decimals,
                    amount_reporting: Decimal::ZERO,
                    amount_secondary: sec.as_ref().map(|_| Decimal::ZERO),
                    rate_reporting: None,
                }
            };
            let Some(rate) = self.rates.find(it.rate_code(), it.rate_currency.as_deref(), &rep.code) else {
                items.push(zero(Some(format!("no rate for '{}' in the rate library", it.rate_code())), &mut unpriced));
                continue;
            };
            if !same_unit(&rate.unit, &it.unit) {
                items.push(zero(Some(format!("the rate is per '{}' but the item is in '{}'", rate.unit, it.unit)), &mut unpriced));
                continue;
            }
            let native = self.currencies.currency(&rate.currency)?;
            let amount = m.quantity.mul_rounded(rate.rate, work(native.decimals));
            let amount_reporting = self.currencies.convert(amount, &native.code, &rep.code, self.as_of, work(rep.decimals))?;
            let amount_secondary = match &sec {
                Some(s) => Some(self.currencies.convert(amount, &native.code, &s.code, self.as_of, work(s.decimals))?),
                None => None,
            };
            let rate_reporting = self.currencies.convert(rate.rate, &native.code, &rep.code, self.as_of, rate_rounding)?;
            *by_currency.entry(native.code.clone()).or_default() += amount;
            items.push(PricedItem {
                code: it.code.clone(),
                section: it.section.clone(),
                description: it.description.clone(),
                unit: it.unit.clone(),
                quantity: m.quantity,
                rate: Some(rate.rate),
                rate_currency: Some(native.code.clone()),
                amount,
                native_decimals: native.decimals,
                amount_reporting,
                amount_secondary,
                rate_reporting: Some(rate_reporting),
            });
        }

        let final_round = |v: Decimal, c: &Currency| v.round(rules.amount_for(c.decimals));
        let mut sections = Vec::new();
        for s in &self.bill.sections {
            let its: Vec<&PricedItem> = items.iter().filter(|i| i.section == s.code).collect();
            sections.push(SectionTotal {
                code: s.code.clone(),
                title: s.title.clone(),
                items: its.len(),
                amount_reporting: final_round(its.iter().map(|i| i.amount_reporting).sum(), &rep),
                amount_secondary: sec.as_ref().map(|c| final_round(its.iter().filter_map(|i| i.amount_secondary).sum(), c)),
            });
        }
        let subtotal: Decimal = sections.iter().map(|s| s.amount_reporting).sum();
        let subtotal_secondary = sec.as_ref().map(|_| sections.iter().filter_map(|s| s.amount_secondary).sum::<Decimal>());

        let mut additions: Vec<AdditionResult> = Vec::new();
        let mut running = subtotal;
        let mut running_secondary = subtotal_secondary;
        for a in &self.additions {
            let amt_round = rules.amount_for(rep.decimals);
            let (base, percent, amount, secondary) = match &a.kind {
                AdditionKind::Percentage { percent, base } => {
                    let b = match base {
                        AdditionBase::BillTotal => subtotal,
                        AdditionBase::RunningTotal => running,
                        AdditionBase::Section(c) => sections.iter().find(|s| &s.code == c).map(|s| s.amount_reporting).unwrap_or_default(),
                        AdditionBase::Additions(list) => additions.iter().filter(|r| list.contains(&r.id)).map(|r| r.amount).sum(),
                    };
                    let amount = b.percent_of(*percent, amt_round);
                    let secondary = match &sec {
                        Some(s) => Some(self.currencies.convert(amount, &rep.code, &s.code, self.as_of, rules.amount_for(s.decimals))?),
                        None => None,
                    };
                    (Some(b), Some(*percent), amount, secondary)
                }
                AdditionKind::LumpSum(m) => {
                    let amount = self.currencies.convert(m.amount, &m.currency, &rep.code, self.as_of, amt_round)?;
                    let secondary = match &sec {
                        Some(s) => Some(self.currencies.convert(m.amount, &m.currency, &s.code, self.as_of, rules.amount_for(s.decimals))?),
                        None => None,
                    };
                    (None, None, amount, secondary)
                }
            };
            running += amount;
            running_secondary = running_secondary.zip(secondary).map(|(r, s)| r + s);
            additions.push(AdditionResult {
                id: a.id.clone(),
                name: a.name.clone(),
                base,
                percent,
                amount,
                amount_secondary: secondary,
                running_total: running,
            });
        }

        Ok(EstimateResult {
            name: self.name.clone(),
            revision: self.revision.clone(),
            reporting: rep,
            secondary: sec,
            measured,
            items,
            unpriced,
            sections,
            by_currency,
            subtotal,
            subtotal_secondary,
            additions,
            grand_total: running,
            grand_total_secondary: running_secondary,
        })
    }

    /// Prices the same bill against two revisions of the model and compares them.
    pub fn compare_revisions(&self, old: &QuantitySet, new: &QuantitySet) -> Result<Comparison> {
        Ok(compare(&self.compute(old)?, &self.compute(new)?))
    }
}

/// What happened to an item between two estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeStatus {
    Added,
    Removed,
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemChange {
    pub code: String,
    pub description: String,
    pub unit: String,
    pub quantity_old: Decimal,
    pub quantity_new: Decimal,
    pub quantity_change: Decimal,
    pub amount_old: Decimal,
    pub amount_new: Decimal,
    pub amount_change: Decimal,
    /// `amount_change / amount_old * 100` at 2 decimals; None when the old amount is zero.
    pub percent_change: Option<Decimal>,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionChange {
    pub id: String,
    pub name: String,
    pub amount_old: Decimal,
    pub amount_new: Decimal,
    pub amount_change: Decimal,
}

/// Two estimates side by side: change in quantity and amount per item, and in the totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub old_label: String,
    pub new_label: String,
    pub currency: String,
    /// New items first in the new estimate's order, then the removed ones.
    pub items: Vec<ItemChange>,
    pub subtotal_old: Decimal,
    pub subtotal_new: Decimal,
    pub subtotal_change: Decimal,
    pub additions: Vec<AdditionChange>,
    pub grand_old: Decimal,
    pub grand_new: Decimal,
    pub grand_change: Decimal,
    pub warnings: Vec<String>,
}

fn label(r: &EstimateResult) -> String {
    if r.revision.is_empty() {
        r.name.clone()
    } else {
        format!("{} {}", r.name, r.revision)
    }
}

/// Compares two estimates item by item (by item code) in the reporting currency.
pub fn compare(old: &EstimateResult, new: &EstimateResult) -> Comparison {
    let mut warnings = Vec::new();
    if old.reporting.code != new.reporting.code {
        warnings.push(format!(
            "the estimates report in different currencies ({} and {}); amounts are not comparable",
            old.reporting.code, new.reporting.code
        ));
    }
    let pct = |change: Decimal, old: Decimal| change.mul_rounded(Decimal::from_int(100), Rounding::half_up(6)).div_rounded(old, Rounding::half_up(2));
    let row = |o: Option<&PricedItem>, n: Option<&PricedItem>| {
        let base = n.or(o).expect("one side exists");
        let (qo, qn) = (o.map(|i| i.quantity).unwrap_or_default(), n.map(|i| i.quantity).unwrap_or_default());
        let (ao, an) = (o.map(|i| i.amount_reporting).unwrap_or_default(), n.map(|i| i.amount_reporting).unwrap_or_default());
        let status = match (o, n) {
            (None, Some(_)) => ChangeStatus::Added,
            (Some(_), None) => ChangeStatus::Removed,
            _ if qo == qn && ao == an => ChangeStatus::Unchanged,
            _ => ChangeStatus::Changed,
        };
        ItemChange {
            code: base.code.clone(),
            description: base.description.clone(),
            unit: base.unit.clone(),
            quantity_old: qo,
            quantity_new: qn,
            quantity_change: qn - qo,
            amount_old: ao,
            amount_new: an,
            amount_change: an - ao,
            percent_change: pct(an - ao, ao),
            status,
        }
    };
    let mut items = Vec::new();
    for n in &new.items {
        let o = old.item(&n.code);
        if let Some(o) = o {
            if o.unit != n.unit {
                warnings.push(format!("item {} changed unit from '{}' to '{}'", n.code, o.unit, n.unit));
            }
        }
        items.push(row(o, Some(n)));
    }
    for o in &old.items {
        if new.item(&o.code).is_none() {
            items.push(row(Some(o), None));
        }
    }
    let mut additions = Vec::new();
    for n in &new.additions {
        let o = old.additions.iter().find(|a| a.id == n.id).map(|a| a.amount).unwrap_or_default();
        additions.push(AdditionChange { id: n.id.clone(), name: n.name.clone(), amount_old: o, amount_new: n.amount, amount_change: n.amount - o });
    }
    for o in &old.additions {
        if !new.additions.iter().any(|a| a.id == o.id) {
            additions.push(AdditionChange { id: o.id.clone(), name: o.name.clone(), amount_old: o.amount, amount_new: Decimal::ZERO, amount_change: -o.amount });
        }
    }
    Comparison {
        old_label: label(old),
        new_label: label(new),
        currency: new.reporting.code.clone(),
        items,
        subtotal_old: old.subtotal,
        subtotal_new: new.subtotal,
        subtotal_change: new.subtotal - old.subtotal,
        additions,
        grand_old: old.grand_total,
        grand_new: new.grand_total,
        grand_change: new.grand_total - old.grand_total,
        warnings,
    }
}
