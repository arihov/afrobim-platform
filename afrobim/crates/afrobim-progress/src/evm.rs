//! Earned value at a data date. The definitions are in the README; each formula is also in the
//! doc comment of the item that computes it.
//!
//! Money is [`Decimal`]. Actual cost is an input and is never derived: when none is entered,
//! everything that needs it is reported as [`Figure::InputNeeded`], not as zero.

use std::collections::BTreeMap;

use afrobim_cost::{Decimal, EstimateResult, Rounding};
use afrobim_kernel::Document;
use afrobim_schedule::{Baseline, Calendar, Date, Programme};
use serde::{Deserialize, Serialize};

use crate::error::{invalid, Result};
use crate::record::ProgressLog;
use crate::rollup::{Rollup, WeightSources};

/// A reported quantity that may be missing for a stated reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Figure<T> {
    Value(T),
    /// An input the user has not entered, and which one.
    InputNeeded(String),
    /// The quantity has no meaning with these figures (a zero or non-positive denominator, or a
    /// date past the data date).
    Undefined(String),
}

impl<T> Figure<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Figure::Value(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_input_needed(&self) -> bool {
        matches!(self, Figure::InputNeeded(_))
    }

    /// The value as text, or `input needed` / `undefined (reason)`.
    pub fn text(&self, f: impl Fn(&T) -> String) -> String {
        match self {
            Figure::Value(v) => f(v),
            Figure::InputNeeded(_) => "input needed".to_string(),
            Figure::Undefined(why) => format!("undefined ({why})"),
        }
    }
}

/// The budget at completion of each activity, in the reporting currency. User input, or taken from
/// a priced estimate with [`Budget::from_estimate`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Budget {
    pub by_activity: BTreeMap<String, Decimal>,
    /// Where the budget came from, in words.
    #[serde(default)]
    pub source: String,
    /// For a budget taken from an estimate: the part of the bill subtotal no activity carries.
    /// It is outside BAC. Additions to the bill (the estimate's own, if any) are outside too.
    #[serde(default)]
    pub unallocated: Option<Decimal>,
}

impl Budget {
    /// A budget entered by the user.
    pub fn from_activity_values(values: BTreeMap<String, Decimal>, source: &str) -> Result<Budget> {
        if let Some((a, v)) = values.iter().find(|(_, v)| **v < Decimal::ZERO) {
            return invalid(format!("budget of activity {a} is {v}; it cannot be negative"));
        }
        Ok(Budget { by_activity: values, source: source.to_string(), unallocated: None })
    }

    /// Budget of each linked activity = sum over its elements of the element's priced value
    /// ([`EstimateResult::element_cost`], rounded to the reporting currency). Adjustments and
    /// manual items belong to no element and are therefore unallocated. An element that several
    /// activities reach would be counted in each, so the call is refused: give those budgets
    /// explicitly with [`Budget::from_activity_values`].
    pub fn from_estimate(rollup: &Rollup, result: &EstimateResult) -> Result<Budget> {
        if !rollup.shared_elements.is_empty() {
            let ids: Vec<String> = rollup.shared_elements.iter().map(|e| e.0.to_string()).collect();
            return invalid(format!("element(s) {} are linked to more than one activity, so their cost cannot be split by the crate; enter the activity budgets", ids.join(", ")));
        }
        let r = Rounding::half_up(result.reporting.decimals);
        let mut by_activity = BTreeMap::new();
        let mut total = Decimal::ZERO;
        for a in &rollup.activities {
            let v: Decimal = a.elements.iter().map(|(e, _)| result.element_cost(*e, r)).sum();
            total += v;
            by_activity.insert(a.activity.clone(), v);
        }
        Ok(Budget {
            by_activity,
            source: format!("estimate '{}' priced element values", result.name),
            unallocated: Some(result.subtotal - total),
        })
    }

    /// BAC = sum of the activity budgets.
    pub fn bac(&self) -> Decimal {
        self.by_activity.values().sum()
    }
}

/// One actual cost entered by the user: an amount incurred on a date, for one activity or (with
/// `activity` None) for the project in that period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActualCost {
    pub date: Date,
    pub activity: Option<String>,
    pub amount: Decimal,
    #[serde(default)]
    pub note: String,
}

/// The actual costs entered so far. Empty means "not entered".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ActualCosts {
    pub entries: Vec<ActualCost>,
}

impl ActualCosts {
    pub fn new() -> ActualCosts {
        ActualCosts::default()
    }

    pub fn add(&mut self, date: Date, activity: Option<&str>, amount: Decimal, note: &str) {
        self.entries.push(ActualCost { date, activity: activity.map(str::to_string), amount, note: note.to_string() });
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// AC to `date`: the sum of the entries dated on or before it; None when nothing is entered.
    pub fn total_to(&self, date: Date) -> Option<Decimal> {
        (!self.entries.is_empty()).then(|| self.entries.iter().filter(|e| e.date <= date).map(|e| e.amount).sum())
    }

    /// The same for one activity's own entries; None when it has none.
    pub fn activity_total_to(&self, activity: &str, date: Date) -> Option<Decimal> {
        let mine: Vec<&ActualCost> = self.entries.iter().filter(|e| e.activity.as_deref() == Some(activity)).collect();
        (!mine.is_empty()).then(|| mine.iter().filter(|e| e.date <= date).map(|e| e.amount).sum())
    }
}

/// The estimate-at-completion formula. There is no default: the caller chooses and the result
/// names it. Below, `CPI = EV / AC` and `SPI = EV / PV`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EacFormula {
    /// `EAC = AC + (BAC - EV)`: the work left is done at the budgeted cost.
    AcPlusRemainingAtBudget,
    /// `EAC = BAC / CPI`, computed as `BAC * AC / EV`: the whole job ends at the cost performance
    /// so far.
    BacOverCpi,
    /// `EAC = AC + (BAC - EV) / (CPI * SPI)`: the work left costs more or less by both indices.
    CpiAndSpi,
    /// `EAC = AC + (BAC - EV) / (wc * CPI + ws * SPI)` with the user's weights `wc`, `ws` (each at
    /// least 0, adding to 1). Indices are taken rounded to 6 decimals.
    CpiSpiWeighted { cpi_weight: Decimal, spi_weight: Decimal },
    /// `EAC = AC + ETC`, with `ETC` a bottom-up estimate of the remaining work entered by the user.
    BottomUp { etc: Decimal },
}

impl EacFormula {
    pub fn name(&self) -> &'static str {
        match self {
            EacFormula::AcPlusRemainingAtBudget => "AC plus remaining work at budget",
            EacFormula::BacOverCpi => "BAC divided by CPI",
            EacFormula::CpiAndSpi => "AC plus remaining work divided by CPI times SPI",
            EacFormula::CpiSpiWeighted { .. } => "AC plus remaining work divided by weighted CPI and SPI",
            EacFormula::BottomUp { .. } => "AC plus bottom-up ETC (user estimate)",
        }
    }

    pub fn definition(&self) -> String {
        match self {
            EacFormula::AcPlusRemainingAtBudget => "EAC = AC + (BAC - EV)".into(),
            EacFormula::BacOverCpi => "EAC = BAC / CPI = BAC * AC / EV".into(),
            EacFormula::CpiAndSpi => "EAC = AC + (BAC - EV) / (CPI * SPI)".into(),
            EacFormula::CpiSpiWeighted { cpi_weight, spi_weight } => {
                format!("EAC = AC + (BAC - EV) / ({cpi_weight} * CPI + {spi_weight} * SPI)")
            }
            EacFormula::BottomUp { etc } => format!("EAC = AC + ETC, ETC entered as {etc}"),
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            EacFormula::CpiSpiWeighted { cpi_weight, spi_weight }
                if *cpi_weight < Decimal::ZERO || *spi_weight < Decimal::ZERO || *cpi_weight + *spi_weight != Decimal::ONE =>
            {
                return invalid("the EAC weights must each be at least 0 and add to 1");
            }
            EacFormula::BottomUp { etc } if *etc < Decimal::ZERO => return invalid("the bottom-up ETC cannot be negative"),
            _ => {}
        }
        Ok(())
    }
}

/// The EAC of a result, with the formula named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EacOutcome {
    pub formula: String,
    pub definition: String,
    pub value: Figure<Decimal>,
}

/// One activity's earned value figures at the date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEvm {
    pub id: String,
    pub name: String,
    pub bac: Decimal,
    pub pv: Decimal,
    pub percent_complete: Decimal,
    pub ev: Decimal,
    /// The activity's own entered actual costs; input needed when it has none.
    pub ac: Figure<Decimal>,
    /// False when no element is linked to the activity, so there is no progress source (EV zero).
    pub measured: bool,
}

/// Earned value at a data date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvmResult {
    pub data_date: Date,
    /// Decimals of money in this result (the rounding the caller gave).
    pub decimals: u32,
    pub bac: Decimal,
    pub pv: Decimal,
    pub ev: Decimal,
    pub ac: Figure<Decimal>,
    /// `SV = EV - PV`.
    pub sv: Decimal,
    /// `CV = EV - AC`.
    pub cv: Figure<Decimal>,
    /// `SPI = EV / PV`.
    pub spi: Figure<Decimal>,
    /// `CPI = EV / AC`.
    pub cpi: Figure<Decimal>,
    pub eac: EacOutcome,
    /// `ETC = EAC - AC`.
    pub etc: Figure<Decimal>,
    /// `VAC = BAC - EAC`.
    pub vac: Figure<Decimal>,
    /// `TCPI = (BAC - EV) / (BAC - AC)`: the cost performance needed on the rest to finish on BAC.
    pub tcpi_bac: Figure<Decimal>,
    /// `TCPI = (BAC - EV) / (EAC - AC)`: the same to finish on the chosen EAC.
    pub tcpi_eac: Figure<Decimal>,
    pub activities: Vec<ActivityEvm>,
    pub notes: Vec<String>,
}

/// Everything earned value needs besides the progress log.
#[derive(Debug, Clone)]
pub struct EvmSetup<'a> {
    /// Must carry a baseline (PV comes from it).
    pub programme: &'a Programme,
    pub document: &'a Document,
    pub weights: WeightSources<'a>,
    pub budget: &'a Budget,
    pub costs: &'a ActualCosts,
    /// Rounding of money: decimals and mode, the user's choice (usually the reporting currency's).
    pub rounding: Rounding,
    pub eac: EacFormula,
}

/// Totals at one date.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Totals {
    pub pv: Decimal,
    pub ev: Decimal,
    pub ac: Option<Decimal>,
    pub rows: Vec<ActivityEvm>,
}

/// Earned value over a programme, a roll-up and a budget. Build it once, ask at many dates.
pub struct EvmEngine<'a> {
    pub(crate) setup: EvmSetup<'a>,
    pub(crate) rollup: Rollup,
    pub(crate) calendar: &'a Calendar,
    pub(crate) baseline: &'a Baseline,
}

/// `a * b / c` exactly in units, rounded once half away from zero; None when `c` is not above zero
/// or the product overflows.
pub(crate) fn muldiv(a: Decimal, b: Decimal, c: Decimal) -> Option<Decimal> {
    if c <= Decimal::ZERO {
        return None;
    }
    let n = a.units().checked_mul(b.units())?;
    let d = c.units();
    let q = n / d;
    let r = n % d;
    Some(Decimal::from_units(if r.abs() * 2 >= d { q + n.signum() } else { q }))
}

fn ratio(n: Decimal, d: Decimal, what: &str) -> Figure<Decimal> {
    if d <= Decimal::ZERO {
        return Figure::Undefined(format!("{what} is not above zero"));
    }
    Figure::Value(n.div_rounded(d, Rounding::half_up(6)).expect("denominator is above zero"))
}

impl<'a> EvmEngine<'a> {
    pub fn new(setup: EvmSetup<'a>) -> Result<EvmEngine<'a>> {
        setup.eac.validate()?;
        let Some(baseline) = setup.programme.baseline.as_ref() else {
            return invalid("the programme has no baseline: planned value comes from it");
        };
        let Some(calendar) = setup.programme.calendar_named(&setup.programme.calendar) else {
            return invalid("the programme calendar is not defined");
        };
        for a in setup.budget.by_activity.keys() {
            if setup.programme.activity(a).is_none() {
                return invalid(format!("the budget names activity {a}, which the programme does not have"));
            }
            if !baseline.entries.iter().any(|e| &e.id == a) {
                return invalid(format!("activity {a} has a budget but no baseline dates, so its planned value cannot be computed"));
            }
        }
        for c in &setup.costs.entries {
            if let Some(a) = &c.activity {
                if setup.programme.activity(a).is_none() {
                    return invalid(format!("an actual cost names activity {a}, which the programme does not have"));
                }
            }
        }
        let rollup = Rollup::new(setup.programme, setup.document, &setup.weights)?;
        Ok(EvmEngine { setup, rollup, calendar, baseline })
    }

    pub fn rollup(&self) -> &Rollup {
        &self.rollup
    }

    pub fn setup(&self) -> &EvmSetup<'a> {
        &self.setup
    }

    /// Planned value of one activity at the end of `date`.
    ///
    /// ```text
    /// n = working days of the programme calendar from the baseline start to min(date, baseline finish), both ends counted
    /// N = working days of the programme calendar from the baseline start to the baseline finish, both ends counted
    /// PV(activity) = BAC(activity) * n / N              n = 0 before the baseline start
    /// ```
    /// An activity with N = 0 (a milestone, or no working day in its span) has PV equal to its
    /// whole BAC from its baseline finish date and zero before it. The result is rounded once.
    pub fn planned_value(&self, activity: &str, date: Date) -> Decimal {
        let bac = self.setup.budget.by_activity.get(activity).copied().unwrap_or_default();
        let Some(e) = self.baseline.entries.iter().find(|e| e.id == activity) else { return Decimal::ZERO };
        let cal = self.calendar;
        let big_n = cal.wd_between(e.start, e.finish.add_days(1));
        if big_n <= 0 {
            return if date >= e.finish { bac } else { Decimal::ZERO };
        }
        if date < e.start {
            return Decimal::ZERO;
        }
        let upto = if date < e.finish { date } else { e.finish };
        let n = cal.wd_between(e.start, upto.add_days(1));
        bac.mul_ratio(n as i128, big_n as i128, self.setup.rounding)
    }

    /// PV, EV and AC and the activity rows at the end of `date`.
    ///
    /// `EV(activity) = BAC(activity) * percent complete(activity) / 100`, rounded once, with the
    /// percent from the roll-up of element records on `date`. An activity with budget and no linked
    /// element has EV zero (it is flagged `measured = false`).
    pub(crate) fn totals_at(&self, log: &ProgressLog, date: Date) -> Result<Totals> {
        let progress = self.rollup.at(log, date)?;
        let mut rows = Vec::new();
        let (mut pv, mut ev) = (Decimal::ZERO, Decimal::ZERO);
        for (id, bac) in &self.setup.budget.by_activity {
            let name = self.setup.programme.activity(id).map(|a| a.name.clone()).unwrap_or_default();
            let p = progress.get(id);
            let percent = p.map(|p| p.percent).unwrap_or_default();
            let a_pv = self.planned_value(id, date);
            let a_ev = bac.percent_of(percent, self.setup.rounding);
            pv += a_pv;
            ev += a_ev;
            let ac = match self.setup.costs.activity_total_to(id, date) {
                Some(v) => Figure::Value(v),
                None => Figure::InputNeeded(format!("no actual cost entered for activity {id}")),
            };
            rows.push(ActivityEvm { id: id.clone(), name, bac: *bac, pv: a_pv, percent_complete: percent, ev: a_ev, ac, measured: p.is_some() });
        }
        Ok(Totals { pv, ev, ac: self.setup.costs.total_to(date), rows })
    }

    /// The earned value figures at `data_date`.
    ///
    /// ```text
    /// BAC = sum of the activity budgets          PV = sum of activity PV          EV = sum of activity EV
    /// AC  = sum of the entered actual costs dated on or before the data date (input)
    /// SV  = EV - PV        CV = EV - AC
    /// SPI = EV / PV        CPI = EV / AC       (shown only where the denominator is above zero)
    /// ETC = EAC - AC       VAC = BAC - EAC
    /// TCPI(BAC) = (BAC - EV) / (BAC - AC)      TCPI(EAC) = (BAC - EV) / (EAC - AC)
    /// ```
    /// EAC is the formula the caller chose in [`EvmSetup::eac`]; see [`EacFormula`].
    pub fn evm(&self, log: &ProgressLog, data_date: Date) -> Result<EvmResult> {
        let t = self.totals_at(log, data_date)?;
        let bac = self.setup.budget.bac();
        let (pv, ev) = (t.pv, t.ev);
        let mut notes = Vec::new();
        for r in &t.rows {
            if !r.measured {
                notes.push(format!("activity {} has a budget but no linked element with progress data, so its earned value is zero", r.id));
            }
        }
        for a in &self.setup.programme.activities {
            if !self.setup.budget.by_activity.contains_key(&a.id) {
                notes.push(format!("activity {} has no budget: it is outside BAC, PV and EV", a.id));
            }
        }
        if let Some(u) = self.setup.budget.unallocated {
            if !u.is_zero() {
                notes.push(format!("{u} of the estimate's bill subtotal is carried by no activity and is outside BAC"));
            }
        }
        let eac_def = (self.setup.eac.name().to_string(), self.setup.eac.definition());
        let need_ac = || Figure::InputNeeded("actual cost (AC) not entered".to_string());
        let result = match t.ac {
            None => EvmResult {
                data_date,
                decimals: self.setup.rounding.decimals,
                bac,
                pv,
                ev,
                ac: need_ac(),
                sv: ev - pv,
                cv: need_ac(),
                spi: ratio(ev, pv, "PV"),
                cpi: need_ac(),
                eac: EacOutcome { formula: eac_def.0, definition: eac_def.1, value: need_ac() },
                etc: need_ac(),
                vac: need_ac(),
                tcpi_bac: need_ac(),
                tcpi_eac: need_ac(),
                activities: t.rows,
                notes,
            },
            Some(ac) => {
                let eac = self.eac_value(bac, pv, ev, ac);
                let (etc, vac, tcpi_eac) = match &eac {
                    Figure::Value(e) => (Figure::Value(*e - ac), Figure::Value(bac - *e), ratio(bac - ev, *e - ac, "EAC - AC")),
                    Figure::Undefined(w) => (Figure::Undefined(w.clone()), Figure::Undefined(w.clone()), Figure::Undefined(w.clone())),
                    Figure::InputNeeded(w) => (Figure::InputNeeded(w.clone()), Figure::InputNeeded(w.clone()), Figure::InputNeeded(w.clone())),
                };
                EvmResult {
                    data_date,
                    decimals: self.setup.rounding.decimals,
                    bac,
                    pv,
                    ev,
                    ac: Figure::Value(ac),
                    sv: ev - pv,
                    cv: Figure::Value(ev - ac),
                    spi: ratio(ev, pv, "PV"),
                    cpi: ratio(ev, ac, "AC"),
                    eac: EacOutcome { formula: eac_def.0, definition: eac_def.1, value: eac },
                    etc,
                    vac,
                    tcpi_bac: ratio(bac - ev, bac - ac, "BAC - AC"),
                    tcpi_eac,
                    activities: t.rows,
                    notes,
                }
            }
        };
        Ok(result)
    }

    fn eac_value(&self, bac: Decimal, pv: Decimal, ev: Decimal, ac: Decimal) -> Figure<Decimal> {
        let r = self.setup.rounding;
        let remaining = bac - ev;
        let undefined = |why: &str| Figure::Undefined(why.to_string());
        let v = match &self.setup.eac {
            EacFormula::AcPlusRemainingAtBudget => ac + remaining,
            EacFormula::BottomUp { etc } => ac + *etc,
            EacFormula::BacOverCpi => {
                if ev <= Decimal::ZERO || ac <= Decimal::ZERO {
                    return undefined("CPI is undefined or zero (EV or AC is not above zero)");
                }
                match muldiv(bac, ac, ev) {
                    Some(v) => v,
                    None => return undefined("amounts too large"),
                }
            }
            EacFormula::CpiAndSpi => {
                if ev <= Decimal::ZERO || ac <= Decimal::ZERO || pv <= Decimal::ZERO {
                    return undefined("an index is undefined or zero (EV, AC or PV is not above zero)");
                }
                // (BAC - EV) / (CPI * SPI) = (BAC - EV) * AC * PV / EV^2, in two exact steps.
                match muldiv(remaining, ac, ev).and_then(|x| muldiv(x, pv, ev)) {
                    Some(y) => ac + y,
                    None => return undefined("amounts too large"),
                }
            }
            EacFormula::CpiSpiWeighted { cpi_weight, spi_weight } => {
                if ac <= Decimal::ZERO || pv <= Decimal::ZERO {
                    return undefined("an index is undefined (AC or PV is not above zero)");
                }
                let r6 = Rounding::half_up(6);
                let (Some(cpi), Some(spi)) = (ev.div_rounded(ac, r6), ev.div_rounded(pv, r6)) else { return undefined("an index is undefined") };
                let den = cpi_weight.mul_rounded(cpi, r6) + spi_weight.mul_rounded(spi, r6);
                match remaining.div_rounded(den, r6) {
                    Some(x) if den > Decimal::ZERO => ac + x,
                    _ => return undefined("the weighted index is not above zero"),
                }
            }
        };
        Figure::Value(v.round(r))
    }
}
