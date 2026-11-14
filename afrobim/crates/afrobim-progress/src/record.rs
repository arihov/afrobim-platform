//! Progress records: dated site measurements per model element.
//!
//! A record says "on this date, this element was this far along", either as a percent complete or
//! as a measured quantity with a unit. Records are append-only. The state of an element on a date
//! is its latest record on or before that date; records on the same date count in the order they
//! were added.

use std::collections::{BTreeMap, BTreeSet};

use afrobim_cost::{Decimal, EstimateResult, Rounding};
use afrobim_kernel::ElementId;
use afrobim_schedule::Date;
use serde::{Deserialize, Serialize};

use crate::error::{invalid, Result};

const PCT: u32 = 6;

fn hundred() -> Decimal {
    Decimal::from_int(100)
}

/// What was measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Measurement {
    /// Percent complete, 0 to 100.
    Percent(Decimal),
    /// Quantity done to date, in a unit. It becomes a percent against the element's target
    /// quantity ([`ProgressLog::set_target`]), which must be in the same unit.
    Quantity { measured: Decimal, unit: String },
}

/// One site measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub element: ElementId,
    pub date: Date,
    pub measurement: Measurement,
    /// Who measured it. Required.
    pub recorded_by: String,
    #[serde(default)]
    pub note: String,
    /// Paths of photos or files (paths only; the crate never opens them).
    #[serde(default)]
    pub files: Vec<String>,
    /// `Some(reason)` marks the record as a correction, the only way the percent may fall below
    /// the previous record's. The reason must not be empty.
    #[serde(default)]
    pub correction: Option<String>,
}

impl Record {
    pub fn new(element: ElementId, date: Date, measurement: Measurement, recorded_by: &str) -> Record {
        Record { element, date, measurement, recorded_by: recorded_by.to_string(), note: String::new(), files: Vec::new(), correction: None }
    }

    pub fn percent(element: ElementId, date: Date, percent: Decimal, recorded_by: &str) -> Record {
        Record::new(element, date, Measurement::Percent(percent), recorded_by)
    }

    pub fn quantity(element: ElementId, date: Date, measured: Decimal, unit: &str, recorded_by: &str) -> Record {
        Record::new(element, date, Measurement::Quantity { measured, unit: unit.to_string() }, recorded_by)
    }

    pub fn with_note(mut self, note: &str) -> Record {
        self.note = note.to_string();
        self
    }

    pub fn with_file(mut self, path: &str) -> Record {
        self.files.push(path.to_string());
        self
    }

    /// Marks the record as a correction, with the reason.
    pub fn as_correction(mut self, reason: &str) -> Record {
        self.correction = Some(reason.to_string());
        self
    }
}

/// The full quantity of an element, against which quantity records become percentages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementTarget {
    pub element: ElementId,
    pub quantity: Decimal,
    pub unit: String,
}

/// An element's state on a date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementProgress {
    pub element: ElementId,
    pub percent: Decimal,
    /// Date of the record that gives this state.
    pub record_date: Date,
    /// Index of that record in [`ProgressLog::records`].
    pub record_index: usize,
}

/// All records and quantity targets of a project.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProgressLog {
    records: Vec<Record>,
    targets: Vec<ElementTarget>,
}

fn same_unit(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl ProgressLog {
    pub fn new() -> ProgressLog {
        ProgressLog::default()
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn targets(&self) -> &[ElementTarget] {
        &self.targets
    }

    pub fn target_of(&self, element: ElementId) -> Option<&ElementTarget> {
        self.targets.iter().find(|t| t.element == element)
    }

    /// Sets the full quantity of an element (replacing an earlier one). Refused when it would make
    /// an existing quantity record invalid.
    pub fn set_target(&mut self, element: ElementId, quantity: Decimal, unit: &str) -> Result<()> {
        if quantity <= Decimal::ZERO {
            return invalid(format!("target quantity of element {} is {quantity}; it must be above zero", element.0));
        }
        if unit.trim().is_empty() {
            return invalid("a target quantity needs a unit");
        }
        let old = self.targets.clone();
        match self.targets.iter_mut().find(|t| t.element == element) {
            Some(t) => {
                t.quantity = quantity;
                t.unit = unit.to_string();
            }
            None => self.targets.push(ElementTarget { element, quantity, unit: unit.to_string() }),
        }
        if let Err(e) = self.validate_all() {
            self.targets = old;
            return Err(e);
        }
        Ok(())
    }

    /// Takes the target from a priced estimate: the quantity the element contributes to the bill
    /// item `item_code`, in that item's unit.
    pub fn set_target_from_estimate(&mut self, result: &EstimateResult, element: ElementId, item_code: &str) -> Result<()> {
        let Some(m) = result.measured.items.iter().find(|m| m.item.code == item_code) else {
            return invalid(format!("the estimate has no item '{item_code}'"));
        };
        let q = m.quantity_of(element);
        if q <= Decimal::ZERO {
            return invalid(format!("element {} contributes no quantity to item '{item_code}'", element.0));
        }
        self.set_target(element, q.round(Rounding::half_up(PCT)), &m.item.unit)
    }

    /// The percent a record stands for. A quantity record needs a target in the same unit.
    pub fn percent_of(&self, r: &Record) -> Result<Decimal> {
        match &r.measurement {
            Measurement::Percent(p) => Ok(*p),
            Measurement::Quantity { measured, unit } => {
                let Some(t) = self.target_of(r.element) else {
                    return invalid(format!("input needed: element {} has a quantity record but no target quantity", r.element.0));
                };
                if !same_unit(unit, &t.unit) {
                    return invalid(format!("element {}: record unit '{unit}' differs from the target unit '{}'", r.element.0, t.unit));
                }
                let r6 = Rounding::half_up(PCT);
                Ok(measured.mul_ratio(100, 1, r6).div_rounded(t.quantity, r6).expect("target is above zero"))
            }
        }
    }

    fn check_one(&self, r: &Record) -> Result<()> {
        if r.recorded_by.trim().is_empty() {
            return invalid(format!("element {} on {}: the record needs the name of who recorded it", r.element.0, r.date));
        }
        if let Some(reason) = &r.correction {
            if reason.trim().is_empty() {
                return invalid(format!("element {} on {}: a correction needs a reason", r.element.0, r.date));
            }
        }
        match &r.measurement {
            Measurement::Percent(p) => {
                if *p < Decimal::ZERO || *p > hundred() {
                    return invalid(format!("element {} on {}: percent complete {p} is outside 0 to 100", r.element.0, r.date));
                }
            }
            Measurement::Quantity { measured, unit } => {
                if *measured < Decimal::ZERO {
                    return invalid(format!("element {} on {}: measured quantity {measured} is negative", r.element.0, r.date));
                }
                if let Some(t) = self.target_of(r.element) {
                    if same_unit(unit, &t.unit) && *measured > t.quantity {
                        return invalid(format!(
                            "element {} on {}: measured {measured} {unit} is above the target {} {}",
                            r.element.0, r.date, t.quantity, t.unit
                        ));
                    }
                }
            }
        }
        self.percent_of(r).map(|_| ())
    }

    /// Records of one element in time order (same date: order added).
    fn ordered(&self, element: ElementId) -> Vec<(usize, &Record)> {
        let mut v: Vec<(usize, &Record)> = self.records.iter().enumerate().filter(|(_, r)| r.element == element).collect();
        v.sort_by_key(|(_, r)| r.date);
        v
    }

    fn validate_sequence(&self, element: ElementId) -> Result<()> {
        let mut prev: Option<(Decimal, Date)> = None;
        for (_, r) in self.ordered(element) {
            self.check_one(r)?;
            let p = self.percent_of(r)?;
            if let Some((pp, pd)) = prev {
                if p < pp && r.correction.is_none() {
                    return invalid(format!(
                        "element {}: the record of {} ({p}%) is below the one of {pd} ({pp}%); a decrease needs a correction with a reason",
                        element.0, r.date
                    ));
                }
            }
            prev = Some((p, r.date));
        }
        Ok(())
    }

    fn validate_all(&self) -> Result<()> {
        let elements: BTreeSet<ElementId> = self.records.iter().map(|r| r.element).collect();
        elements.into_iter().try_for_each(|e| self.validate_sequence(e))
    }

    /// Appends a record after validating it and the whole sequence of its element: percent within
    /// 0 to 100, quantity not above the target, who recorded it, and no decrease against the
    /// previous record (or, for a record dated before others, against the next) unless the lower
    /// record is marked as a correction with a reason. A refused record is not stored.
    pub fn add(&mut self, record: Record) -> Result<()> {
        let element = record.element;
        self.records.push(record);
        if let Err(e) = self.validate_sequence(element) {
            self.records.pop();
            return Err(e);
        }
        Ok(())
    }

    /// The element's state at the end of `date`: its latest record on or before it.
    pub fn state_at(&self, element: ElementId, date: Date) -> Result<Option<ElementProgress>> {
        let last = self.ordered(element).into_iter().rfind(|(_, r)| r.date <= date);
        match last {
            None => Ok(None),
            Some((i, r)) => Ok(Some(ElementProgress { element, percent: self.percent_of(r)?, record_date: r.date, record_index: i })),
        }
    }

    /// Percent of every element that has a record on or before `date`.
    pub fn percents_at(&self, date: Date) -> Result<BTreeMap<ElementId, Decimal>> {
        let elements: BTreeSet<ElementId> = self.records.iter().map(|r| r.element).collect();
        let mut out = BTreeMap::new();
        for e in elements {
            if let Some(s) = self.state_at(e, date)? {
                out.insert(e, s.percent);
            }
        }
        Ok(out)
    }

    /// Date of the first record above 0% of the element, as of `date`; None when the element
    /// stands at 0% (or has no record) at `date`.
    pub fn start_date(&self, element: ElementId, date: Date) -> Result<Option<Date>> {
        let recs: Vec<&Record> = self.ordered(element).into_iter().map(|(_, r)| r).filter(|r| r.date <= date).collect();
        let Some(last) = recs.last() else { return Ok(None) };
        if self.percent_of(last)? <= Decimal::ZERO {
            return Ok(None);
        }
        for r in &recs {
            if self.percent_of(r)? > Decimal::ZERO {
                return Ok(Some(r.date));
            }
        }
        Ok(None)
    }

    /// When the element, standing at 100% on `date`, first reached 100% in its latest unbroken run
    /// at 100% (a correction down and back up starts a new run). None when it is below 100%.
    pub fn completion_date(&self, element: ElementId, date: Date) -> Result<Option<Date>> {
        let recs: Vec<&Record> = self.ordered(element).into_iter().map(|(_, r)| r).filter(|r| r.date <= date).collect();
        let mut at: Option<Date> = None;
        for r in recs.iter().rev() {
            if self.percent_of(r)? >= hundred() {
                at = Some(r.date);
            } else {
                break;
            }
        }
        Ok(at)
    }

    pub fn latest_record_date(&self) -> Option<Date> {
        self.records.iter().map(|r| r.date).max()
    }
}
