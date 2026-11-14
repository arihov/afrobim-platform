//! Rates and currencies: the rate library (CSV in and out), optional rate build-ups, currencies
//! and the exchange rates the user entered with the date they were taken.
//!
//! Nothing here is a price. A library starts empty; every rate, build-up component and exchange
//! rate is typed in by the user or loaded from the user's file.

use serde::{Deserialize, Serialize};

use crate::{csv, CostError, Date, Decimal, Result, Rounding};

/// A currency in use: its code (`UGX`, `USD`) and the decimals its amounts are rounded to. The
/// decimals are the user's choice (whole shillings, cents).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Currency {
    pub code: String,
    pub decimals: u32,
}

impl Currency {
    pub fn new(code: &str, decimals: u32) -> Currency {
        Currency { code: code.trim().to_uppercase(), decimals: decimals.min(crate::decimal::DECIMALS) }
    }
}

/// An amount in a currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: String,
}

impl Money {
    pub fn new(amount: Decimal, currency: &str) -> Money {
        Money { amount, currency: currency.trim().to_uppercase() }
    }
}

/// An exchange rate the user entered: 1 unit of `from` is `rate` units of `to`, taken on `date`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub from: String,
    pub to: String,
    pub rate: Decimal,
    pub date: Date,
    #[serde(default)]
    pub source_note: String,
}

/// The currencies of an estimate and the exchange rates between them.
///
/// A rate entered one way converts the other way by its reciprocal: with 1 USD = 3700 UGX,
/// UGX to USD divides by 3700. Only direct and reciprocal rates are used, never a chain through
/// a third currency.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CurrencyTable {
    pub currencies: Vec<Currency>,
    pub rates: Vec<ExchangeRate>,
}

impl CurrencyTable {
    /// Adds or redefines a currency.
    pub fn add_currency(&mut self, currency: Currency) {
        match self.currencies.iter_mut().find(|c| c.code == currency.code) {
            Some(c) => *c = currency,
            None => self.currencies.push(currency),
        }
    }

    pub fn currency(&self, code: &str) -> Result<&Currency> {
        let code = code.trim().to_uppercase();
        self.currencies.iter().find(|c| c.code == code).ok_or_else(|| CostError::Invalid(format!("currency '{code}' has not been defined")))
    }

    /// Enters an exchange rate. Both currencies must be defined and the rate positive.
    pub fn add_rate(&mut self, from: &str, to: &str, rate: Decimal, date: Date, source_note: &str) -> Result<()> {
        let (from, to) = (self.currency(from)?.code.clone(), self.currency(to)?.code.clone());
        if from == to {
            return Err(CostError::Invalid("an exchange rate needs two different currencies".into()));
        }
        if rate <= Decimal::ZERO {
            return Err(CostError::Invalid(format!("the exchange rate {from} to {to} must be greater than zero")));
        }
        self.rates.push(ExchangeRate { from, to, rate, date, source_note: source_note.into() });
        Ok(())
    }

    /// The rate in force on `as_of` (the latest entered on or before it; the latest of all when
    /// `as_of` is None), and whether it must be inverted to go `from` to `to`.
    pub fn rate_between(&self, from: &str, to: &str, as_of: Option<Date>) -> Result<(&ExchangeRate, bool)> {
        let (from, to) = (from.trim().to_uppercase(), to.trim().to_uppercase());
        let mut best: Option<(&ExchangeRate, bool)> = None;
        for r in &self.rates {
            let inverted = if r.from == from && r.to == to {
                false
            } else if r.from == to && r.to == from {
                true
            } else {
                continue;
            };
            if as_of.is_some_and(|d| r.date > d) {
                continue;
            }
            if best.is_none_or(|(b, _)| r.date >= b.date) {
                best = Some((r, inverted));
            }
        }
        best.ok_or_else(|| CostError::NoExchangeRate {
            from,
            to,
            when: as_of.map(|d| format!(" on or before {d}")).unwrap_or_default(),
        })
    }

    /// `amount` of `from` in `to`, rounded once by `rounding`.
    pub fn convert(&self, amount: Decimal, from: &str, to: &str, as_of: Option<Date>, rounding: Rounding) -> Result<Decimal> {
        if from.trim().eq_ignore_ascii_case(to.trim()) {
            return Ok(amount.round(rounding));
        }
        let (r, inverted) = self.rate_between(from, to, as_of)?;
        Ok(if inverted {
            amount.div_rounded(r.rate, rounding).expect("rates are positive")
        } else {
            amount.mul_rounded(r.rate, rounding)
        })
    }
}

/// One rate of the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub item_code: String,
    pub description: String,
    pub unit: String,
    pub rate: Decimal,
    pub currency: String,
    /// Where the rate came from: a quotation, a schedule of rates, a past contract. The user's words.
    #[serde(default)]
    pub source_note: String,
    /// The date the rate was taken.
    #[serde(default)]
    pub date: Option<Date>,
}

/// The header of the rate library CSV.
pub const RATES_CSV_HEADER: [&str; 7] = ["item_code", "description", "unit", "rate", "currency", "source_note", "date"];

/// The user's rates. Empty until the user enters or loads some.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RateLibrary {
    pub rates: Vec<Rate>,
}

impl RateLibrary {
    /// Adds a rate, replacing one with the same item code and currency.
    pub fn set(&mut self, mut rate: Rate) {
        rate.currency = rate.currency.trim().to_uppercase();
        match self.rates.iter_mut().find(|r| r.item_code == rate.item_code && r.currency.eq_ignore_ascii_case(&rate.currency)) {
            Some(r) => *r = rate,
            None => self.rates.push(rate),
        }
    }

    /// The rate for a code: in `currency` when given; otherwise in `preferred` if there is one,
    /// else the first in the library.
    pub fn find(&self, code: &str, currency: Option<&str>, preferred: &str) -> Option<&Rate> {
        let mut it = self.rates.iter().filter(|r| r.item_code == code);
        match currency {
            Some(c) => it.find(|r| r.currency.eq_ignore_ascii_case(c)),
            None => {
                let all: Vec<&Rate> = it.collect();
                all.iter().find(|r| r.currency.eq_ignore_ascii_case(preferred)).or(all.first()).copied()
            }
        }
    }

    pub fn to_csv(&self) -> String {
        let mut out = csv::row(&RATES_CSV_HEADER);
        for r in &self.rates {
            out.push_str(&csv::row(&[
                r.item_code.as_str(),
                &r.description,
                &r.unit,
                &r.rate.to_string(),
                &r.currency,
                &r.source_note,
                &r.date.map(|d| d.to_string()).unwrap_or_default(),
            ]));
        }
        out
    }

    /// Loads a library from CSV. A rate that is not a number, or a date that is not
    /// `YYYY-MM-DD`, is an error naming the row; nothing is guessed.
    pub fn from_csv(text: &str) -> Result<RateLibrary> {
        let rows = csv::parse_with_header(text, &RATES_CSV_HEADER)?;
        let mut lib = RateLibrary::default();
        for (n, r) in rows.iter().enumerate() {
            let at = |e: CostError| CostError::Csv(format!("row {}: {e}", n + 2));
            if r[0].trim().is_empty() {
                return Err(CostError::Csv(format!("row {}: the item code is empty", n + 2)));
            }
            let date = if r[6].trim().is_empty() { None } else { Some(Date::parse(&r[6]).map_err(at)?) };
            lib.set(Rate {
                item_code: r[0].trim().into(),
                description: r[1].trim().into(),
                unit: r[2].trim().into(),
                rate: Decimal::parse(&r[3]).map_err(at)?,
                currency: r[4].trim().to_uppercase(),
                source_note: r[5].trim().into(),
                date,
            });
        }
        Ok(lib)
    }

    /// Enters the rate summed from a build-up (see [`BuildUp::total`]).
    pub fn set_from_build_up(&mut self, build_up: &BuildUp, rounding: Rounding, source_note: &str, date: Option<Date>) -> Result<()> {
        let b = build_up.total(rounding)?;
        self.set(Rate {
            item_code: build_up.item_code.clone(),
            description: build_up.description.clone(),
            unit: build_up.unit.clone(),
            rate: b.rate,
            currency: build_up.currency.trim().to_uppercase(),
            source_note: source_note.into(),
            date,
        });
        Ok(())
    }
}

/// What a build-up component is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildUpKind {
    Labour,
    Plant,
    Materials,
    /// Any other direct cost (transport, subcontract).
    Other,
    Overheads,
    Profit,
}

impl BuildUpKind {
    fn is_direct(self) -> bool {
        !matches!(self, BuildUpKind::Overheads | BuildUpKind::Profit)
    }
}

/// What a percentage component is a percentage of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PercentBase {
    /// Labour, plant, materials and other direct costs.
    Direct,
    /// Direct costs plus the overheads components that come before it.
    DirectAndOverheads,
    /// Everything that comes before it in the build-up.
    Running,
}

/// A component's value as entered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// An amount per unit of the item, in the build-up's currency.
    Amount(Decimal),
    /// A percentage of a base; only overheads and profit may use it.
    Percent { percent: Decimal, of: PercentBase },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub kind: BuildUpKind,
    pub description: String,
    pub value: Entry,
}

/// A rate built from entered components per unit: direct costs (amounts), then overheads and
/// profit (amounts or percentages the user entered). The rate is their sum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildUp {
    pub item_code: String,
    pub description: String,
    pub unit: String,
    pub currency: String,
    pub components: Vec<Component>,
}

/// A build-up worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildUpResult {
    /// Each component with its amount (percentages rounded by the rule), in entered order.
    pub components: Vec<(Component, Decimal)>,
    /// Sum of the direct components.
    pub direct: Decimal,
    /// Sum of all components, rounded by the rule: the rate.
    pub rate: Decimal,
}

impl BuildUp {
    /// Sums the components. Direct components (labour, plant, materials, other) must be
    /// amounts. Overheads and profit are worked in the order entered; each percentage amount is
    /// rounded by `rounding` and the rate is the rounded sum.
    pub fn total(&self, rounding: Rounding) -> Result<BuildUpResult> {
        let mut direct = Decimal::ZERO;
        for c in self.components.iter().filter(|c| c.kind.is_direct()) {
            match &c.value {
                Entry::Amount(a) => direct += *a,
                Entry::Percent { .. } => {
                    return Err(CostError::Invalid(format!("'{}' of {}: a direct cost must be an amount, not a percentage", c.description, self.item_code)))
                }
            }
        }
        let mut overheads = Decimal::ZERO;
        let mut running = direct;
        let mut amounts = Vec::new();
        for c in &self.components {
            let amount = if c.kind.is_direct() {
                match &c.value {
                    Entry::Amount(a) => *a,
                    Entry::Percent { .. } => unreachable!("checked above"),
                }
            } else {
                match &c.value {
                    Entry::Amount(a) => *a,
                    Entry::Percent { percent, of } => {
                        let base = match of {
                            PercentBase::Direct => direct,
                            PercentBase::DirectAndOverheads => direct + overheads,
                            PercentBase::Running => running,
                        };
                        base.percent_of(*percent, rounding)
                    }
                }
            };
            if !c.kind.is_direct() {
                running += amount;
                if c.kind == BuildUpKind::Overheads {
                    overheads += amount;
                }
            }
            amounts.push((c.clone(), amount));
        }
        let total: Decimal = amounts.iter().map(|(_, a)| *a).sum();
        Ok(BuildUpResult { components: amounts, direct, rate: total.round(rounding) })
    }
}
