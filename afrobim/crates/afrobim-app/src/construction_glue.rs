//! The construction stage in the app: where site progress is kept, how records are entered for a
//! set of elements, the roll-up to the programme, earned value and the report tables.
//!
//! The progress book is one document element of category `progress` (the progress crate's
//! `add_progress`, `store_progress` and `load_progress`), so it is saved, opened and undone with the
//! document. What the book does not hold (the data date as typed, where the budget comes from, the
//! budget entered per activity, the EAC formula, the look-ahead length) is kept as one property of
//! the same element, `construction.settings`, as JSON of [`Settings`].
//!
//! Nothing here holds a rate, a percentage, a weight, a threshold or a formula of its own. Actual
//! cost is never derived; the EAC formula starts unchosen; every field starts empty.

use std::collections::BTreeMap;

use afrobim_cost::{Decimal, EstimateResult, Rounding, RoundingRules};
use afrobim_kernel::{Document, ElementId};
use afrobim_progress::{
    add_progress, apply_updates, delay_list, load_progress, look_ahead, refresh_progress, store_progress, ApplyReport, Budget, DelayRow, EacFormula, EvmEngine, EvmResult, EvmSetup, LookAheadRow,
    PaymentQuantities, PeriodTable, ProgressBook, ProgressLog, RefreshParams, Record, Rollup, RollupResult, SCurveData, StoredReport, WeightSources, PROGRESS_CATEGORY,
};
use afrobim_schedule::{Date, ElementRule, Programme, Schedule};
use serde::{Deserialize, Serialize};

use crate::schedule_glue;

/// Property of the progress element that holds [`Settings`].
pub const PROP_SETTINGS: &str = "construction.settings";
/// The name the progress element is given when the first record is entered.
pub const ELEMENT_NAME: &str = "Site progress";

/// The EAC formulae the window offers. There is no default: `Settings::eac` starts as `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EacKind {
    AcPlusRemainingAtBudget,
    BacOverCpi,
    CpiAndSpi,
    CpiSpiWeighted,
    BottomUp,
}

impl EacKind {
    pub const ALL: [EacKind; 5] = [EacKind::AcPlusRemainingAtBudget, EacKind::BacOverCpi, EacKind::CpiAndSpi, EacKind::CpiSpiWeighted, EacKind::BottomUp];

    /// The formula's name and its definition, as the crate words them (with the weights as typed).
    pub fn label(self) -> String {
        let f = match self {
            EacKind::AcPlusRemainingAtBudget => EacFormula::AcPlusRemainingAtBudget,
            EacKind::BacOverCpi => EacFormula::BacOverCpi,
            EacKind::CpiAndSpi => EacFormula::CpiAndSpi,
            EacKind::CpiSpiWeighted => EacFormula::CpiSpiWeighted { cpi_weight: Decimal::ZERO, spi_weight: Decimal::ZERO },
            EacKind::BottomUp => EacFormula::BottomUp { etc: Decimal::ZERO },
        };
        match self {
            EacKind::CpiSpiWeighted => format!("{}: EAC = AC + (BAC - EV) / (wc * CPI + ws * SPI)", f.name()),
            EacKind::BottomUp => format!("{}: EAC = AC + ETC", f.name()),
            _ => format!("{}: {}", f.name(), f.definition()),
        }
    }
}

/// What the construction windows keep besides the book. Text fields hold what was typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The data date, YYYY-MM-DD, as typed.
    pub data_date: String,
    /// The budget comes from the priced estimate (true) or is entered per activity (false).
    pub budget_from_estimate: bool,
    /// The budget at completion entered per activity, as typed (an empty entry is no budget).
    pub budget_text: BTreeMap<String, String>,
    /// The EAC formula the user chose; None until one is chosen.
    pub eac: Option<EacKind>,
    pub cpi_weight: String,
    pub spi_weight: String,
    pub etc: String,
    /// The look-ahead length in days, as typed.
    pub look_ahead_days: String,
}

impl Settings {
    /// The data date typed, or why it is not usable.
    pub fn date(&self) -> Result<Date, String> {
        let t = self.data_date.trim();
        if t.is_empty() {
            return Err("enter the data date (YYYY-MM-DD)".into());
        }
        if t.len() == 10 {
            if let Some(d) = Date::parse(t) {
                return Ok(d);
            }
        }
        Err(format!("'{t}' is not a date (YYYY-MM-DD)"))
    }

    /// The chosen formula with its entries, `Ok(None)` when none is chosen.
    pub fn formula(&self) -> Result<Option<EacFormula>, String> {
        let dec = |label: &str, text: &str| -> Result<Decimal, String> {
            if text.trim().is_empty() {
                return Err(format!("enter {label}"));
            }
            Decimal::parse(text.trim()).map_err(|e| format!("{label}: {e}"))
        };
        Ok(match self.eac {
            None => None,
            Some(EacKind::AcPlusRemainingAtBudget) => Some(EacFormula::AcPlusRemainingAtBudget),
            Some(EacKind::BacOverCpi) => Some(EacFormula::BacOverCpi),
            Some(EacKind::CpiAndSpi) => Some(EacFormula::CpiAndSpi),
            Some(EacKind::CpiSpiWeighted) => Some(EacFormula::CpiSpiWeighted { cpi_weight: dec("the CPI weight", &self.cpi_weight)?, spi_weight: dec("the SPI weight", &self.spi_weight)? }),
            Some(EacKind::BottomUp) => Some(EacFormula::BottomUp { etc: dec("the bottom-up ETC", &self.etc)? }),
        })
    }
}

// ─── the book in the document ───────────────────────────────────────────────────────────

/// The progress element of the document, if there is one.
pub fn find(doc: &Document) -> Option<ElementId> {
    doc.elements().iter().find(|e| e.category == PROGRESS_CATEGORY).map(|e| e.id)
}

/// The book and the settings of the document's progress element.
pub fn load(doc: &Document) -> Option<(ElementId, ProgressBook, Settings)> {
    let id = find(doc)?;
    let book = load_progress(doc, id).ok()?;
    let settings = doc.find(id).and_then(|e| e.properties.get(PROP_SETTINGS)).and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
    Some((id, book, settings))
}

/// Writes the book and the settings, adding the element the first time.
pub fn store(doc: &mut Document, id: Option<ElementId>, book: &ProgressBook, settings: &Settings) -> Result<ElementId, String> {
    let id = match id.filter(|i| doc.find(*i).is_some()) {
        Some(id) => {
            store_progress(doc, id, book).map_err(|e| e.to_string())?;
            id
        }
        None => add_progress(doc, ELEMENT_NAME, book),
    };
    let text = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    if let Some(e) = doc.find_mut(id) {
        e.properties.insert(PROP_SETTINGS.to_string(), text);
    }
    Ok(id)
}

// ─── site records ───────────────────────────────────────────────────────────────────────

/// What a site record measures, as typed.
#[derive(Debug, Clone, PartialEq)]
pub enum Measure {
    Percent(String),
    Quantity { measured: String, unit: String },
}

/// One record entered for several elements: the same date, measurement, recorder and note.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordEntry {
    pub date: String,
    pub measure: Measure,
    pub recorded_by: String,
    pub note: String,
    /// Paths of photos or files, one per entry.
    pub files: Vec<String>,
    /// `Some(reason)` makes each record a correction (the reason may not be empty).
    pub correction: Option<String>,
}

/// The elements a record is for: the selection, or the elements a rule picks. Records of the
/// document itself and alignments are never offered.
pub fn target_elements(doc: &Document, selection: &[ElementId], rule: Option<&ElementRule>) -> Vec<ElementId> {
    let mut out: Vec<ElementId> = match rule {
        Some(rule) => doc.elements().iter().filter(|e| schedule_glue::linkable(e) && rule.matches(e)).map(|e| e.id).collect(),
        None => selection.iter().copied().filter(|id| doc.find(*id).is_some_and(schedule_glue::linkable)).collect(),
    };
    out.dedup();
    out
}

/// Adds the record to every element, all or none: when the crate refuses one (a decrease without a
/// reason, a percent outside 0 to 100, a quantity above its target, no target...) nothing is
/// stored and its message is returned as the crate words it. Returns how many records were added.
pub fn add_records(log: &mut ProgressLog, elements: &[ElementId], entry: &RecordEntry) -> Result<usize, String> {
    if elements.is_empty() {
        return Err("no element is selected, and the rule picks none".into());
    }
    let date = {
        let t = entry.date.trim();
        if t.is_empty() {
            return Err("enter the date of the record (YYYY-MM-DD)".into());
        }
        if t.len() == 10 { Date::parse(t) } else { None }.ok_or_else(|| format!("'{t}' is not a date (YYYY-MM-DD)"))?
    };
    let dec = |label: &str, text: &str| -> Result<Decimal, String> {
        if text.trim().is_empty() {
            return Err(format!("enter {label}"));
        }
        Decimal::parse(text.trim()).map_err(|e| format!("{label}: {e}"))
    };
    let mut next = log.clone();
    for e in elements {
        let mut r = match &entry.measure {
            Measure::Percent(p) => Record::percent(*e, date, dec("the percent complete", p)?, entry.recorded_by.trim()),
            Measure::Quantity { measured, unit } => Record::quantity(*e, date, dec("the measured quantity", measured)?, unit.trim(), entry.recorded_by.trim()),
        };
        r.note = entry.note.trim().to_string();
        r.files = entry.files.iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
        if let Some(reason) = &entry.correction {
            r = r.as_correction(reason);
        }
        next.add(r).map_err(|e| e.to_string())?;
    }
    *log = next;
    Ok(elements.len())
}

/// The records of the log in date order, filtered to one element and a range of dates (each end
/// optional). Returns (index in the log, record, percent the record stands for).
pub fn filtered_records<'a>(log: &'a ProgressLog, element: Option<ElementId>, from: Option<Date>, to: Option<Date>) -> Vec<(usize, &'a Record, Option<Decimal>)> {
    let mut rows: Vec<(usize, &Record, Option<Decimal>)> = log
        .records()
        .iter()
        .enumerate()
        .filter(|(_, r)| element.is_none_or(|e| r.element == e) && from.is_none_or(|d| r.date >= d) && to.is_none_or(|d| r.date <= d))
        .map(|(i, r)| (i, r, log.percent_of(r).ok()))
        .collect();
    rows.sort_by_key(|(i, r, _)| (r.date, *i));
    rows
}

// ─── roll-up and the programme ──────────────────────────────────────────────────────────

/// The element to activity roll-up for the programme and the weights that are available: the
/// priced estimate when there is one (else equal weights). The crate says which basis each activity used.
pub fn rollup(doc: &Document, prog: &Programme, estimate: Option<&EstimateResult>) -> Result<Rollup, String> {
    Rollup::new(prog, doc, &WeightSources { user: None, estimate }).map_err(|e| e.to_string())
}

/// The roll-up at a date.
pub fn rollup_at(doc: &Document, prog: &Programme, estimate: Option<&EstimateResult>, log: &ProgressLog, date: Date) -> Result<RollupResult, String> {
    rollup(doc, prog, estimate)?.at(log, date).map_err(|e| e.to_string())
}

/// Applies a roll-up to a copy of the programme (data date, percent, inferred actual dates) and
/// reschedules it. Returns the programme, what was written, and the schedule (or why it could not
/// be made).
pub fn apply(prog: &Programme, result: &RollupResult) -> Result<(Programme, ApplyReport, Result<Schedule, String>), String> {
    let mut next = prog.clone();
    let report = apply_updates(&mut next, result.date, &result.updates()).map_err(|e| e.to_string())?;
    let schedule = afrobim_schedule::schedule(&next).map_err(|e| e.to_string());
    Ok((next, report, schedule))
}

// ─── earned value ───────────────────────────────────────────────────────────────────────

/// What the earned value window shows.
#[derive(Debug, Clone, PartialEq)]
pub struct EvmOutput {
    pub evm: EvmResult,
    pub periods: PeriodTable,
    pub curve: SCurveData,
    /// The user has not chosen an EAC formula: EAC, ETC, VAC and TCPI to EAC are not shown. The
    /// other figures do not depend on it.
    pub eac_unchosen: bool,
    pub budget: Budget,
}

/// The budget the settings call for: from the estimate, or entered per activity.
pub fn budget_of(doc: &Document, prog: &Programme, estimate: Option<&EstimateResult>, s: &Settings) -> Result<Budget, String> {
    if s.budget_from_estimate {
        let result = estimate.ok_or("the budget is to come from the estimate, which has no result yet: build the bill and rates and open ESTIMATE, or enter the budget per activity")?;
        let rollup = rollup(doc, prog, Some(result))?;
        return Budget::from_estimate(&rollup, result).map_err(|e| e.to_string());
    }
    let mut values = BTreeMap::new();
    for (activity, text) in &s.budget_text {
        if text.trim().is_empty() {
            continue;
        }
        let v = Decimal::parse(text.trim()).map_err(|e| format!("the budget of activity {activity}: {e}"))?;
        values.insert(activity.clone(), v);
    }
    if values.is_empty() {
        return Err("enter the budget at completion of at least one activity, or take the budget from the estimate".into());
    }
    Budget::from_activity_values(values, "entered per activity").map_err(|e| e.to_string())
}

/// Earned value at the data date. `decimals` is the money rounding of the report, the decimals of
/// the reporting currency.
pub fn evm(doc: &Document, prog: &Programme, estimate: Option<&EstimateResult>, book: &ProgressBook, s: &Settings, decimals: u32) -> Result<EvmOutput, String> {
    let date = s.date()?;
    if prog.baseline.is_none() {
        return Err("the programme has no baseline, and planned value comes from it: set the baseline (BASELINE)".into());
    }
    let budget = budget_of(doc, prog, estimate, s)?;
    let chosen = s.formula()?;
    let eac_unchosen = chosen.is_none();
    // The figures that do not depend on the formula are the same whichever one is passed; with none
    // chosen the formula-dependent figures are hidden by the caller.
    let formula = chosen.unwrap_or(EacFormula::AcPlusRemainingAtBudget);
    let engine = EvmEngine::new(EvmSetup { programme: prog, document: doc, weights: WeightSources { user: None, estimate }, budget: &budget, costs: &book.costs, rounding: Rounding::half_up(decimals), eac: formula })
        .map_err(|e| e.to_string())?;
    let (evm, periods, curve) = engine.report(&book.log, date).map_err(|e| e.to_string())?;
    Ok(EvmOutput { evm, periods, curve, eac_unchosen, budget })
}

/// Stores the report in the progress element (so the S-curve is drawn on the sheet). Needs the
/// chosen formula.
pub fn store_report(doc: &mut Document, id: ElementId, prog: &Programme, estimate: Option<&EstimateResult>, s: &Settings, decimals: u32) -> Result<StoredReport, String> {
    let formula = s.formula()?.ok_or("choose an EAC formula first")?;
    let budget = budget_of(doc, prog, estimate, s)?;
    let params = RefreshParams { programme: prog, weights: WeightSources { user: None, estimate }, budget: &budget, rounding: Rounding::half_up(decimals), eac: formula };
    refresh_progress(doc, id, &params, s.date()?).map_err(|e| e.to_string())
}

/// Adds an actual cost entry. The amount and date are the user's.
pub fn add_cost(book: &mut ProgressBook, date: &str, activity: Option<&str>, amount: &str, note: &str) -> Result<(), String> {
    let d = {
        let t = date.trim();
        if t.is_empty() {
            return Err("enter the date of the cost (YYYY-MM-DD)".into());
        }
        if t.len() == 10 { Date::parse(t) } else { None }.ok_or_else(|| format!("'{t}' is not a date (YYYY-MM-DD)"))?
    };
    if amount.trim().is_empty() {
        return Err("enter the amount".into());
    }
    let amount = Decimal::parse(amount.trim()).map_err(|e| format!("the amount: {e}"))?;
    book.costs.add(d, activity, amount, note.trim());
    Ok(())
}

// ─── look-ahead and delays ──────────────────────────────────────────────────────────────

/// The look-ahead and the delay list from a schedule computed after the progress was applied.
pub fn look_ahead_and_delays(prog: &Programme, schedule: &Schedule, from: Date, days: &str) -> Result<(Vec<LookAheadRow>, Vec<DelayRow>), String> {
    let n: u32 = days.trim().parse().map_err(|_| "enter the number of days to look ahead (a whole number)".to_string())?;
    if n == 0 {
        return Err("the look-ahead must be at least one day".into());
    }
    let delays = delay_list(prog, schedule).map_err(|e| e.to_string())?;
    Ok((look_ahead(schedule, from, n), delays))
}

/// Writes every table the progress crate offers into a folder: the figures, the activity table, the
/// periods, the S-curve, the look-ahead (when its length was entered) and the delay list. Returns
/// the files written.
pub fn write_csvs(dir: &std::path::Path, out: &EvmOutput, look: Option<&[LookAheadRow]>, delays: Option<&[DelayRow]>) -> Result<Vec<String>, String> {
    use afrobim_progress::{delay_csv, evm_activity_csv, evm_csv, look_ahead_csv, period_csv, s_curve_csv};
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let mut files: Vec<(&str, String)> = vec![
        ("evm.csv", evm_csv(&out.evm)),
        ("evm-activities.csv", evm_activity_csv(&out.evm)),
        ("evm-periods.csv", period_csv(&out.periods)),
        ("evm-s-curve.csv", s_curve_csv(&out.curve)),
    ];
    if let Some(rows) = look {
        files.push(("look-ahead.csv", look_ahead_csv(rows)));
    }
    if let Some(rows) = delays {
        files.push(("delays.csv", delay_csv(rows)));
    }
    let mut written = Vec::new();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).map_err(|e| format!("could not write {name}: {e}"))?;
        written.push(name.to_string());
    }
    Ok(written)
}

/// The site records as CSV: date, element mark, measurement, percent, recorded by, note, files, correction.
pub fn records_csv(doc: &Document, log: &ProgressLog) -> String {
    fn field(t: &str) -> String {
        if t.contains([',', '"', '\n']) {
            format!("\"{}\"", t.replace('"', "\"\""))
        } else {
            t.to_string()
        }
    }
    let mut out = String::from("Date,Element,Measurement,Percent complete,Recorded by,Note,Files,Correction\n");
    for (_, r, percent) in filtered_records(log, None, None, None) {
        let mark = doc.find(r.element).map(|e| e.name.clone()).unwrap_or_else(|| format!("element {}", r.element.0));
        let measure = match &r.measurement {
            afrobim_progress::Measurement::Percent(p) => format!("{p} %"),
            afrobim_progress::Measurement::Quantity { measured, unit } => format!("{measured} {unit}"),
        };
        let percent = percent.map(|p| p.to_string()).unwrap_or_else(|| "input needed".into());
        let row = [r.date.to_string(), mark, measure, percent, r.recorded_by.clone(), r.note.clone(), r.files.join("; "), r.correction.clone().unwrap_or_default()];
        out.push_str(&row.iter().map(|f| field(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

// ─── payment and the 3D view ────────────────────────────────────────────────────────────

/// The quantities to date from the site records, for the interim certificate. `item_percent` gives
/// the percent for adjustments and manual items, which belong to no element.
pub fn site_quantities(result: &EstimateResult, log: &ProgressLog, date: Date, item_percent: &BTreeMap<String, Decimal>, rules: &RoundingRules) -> Result<PaymentQuantities, String> {
    afrobim_progress::qty_to_date(result, log, date, item_percent, rules).map_err(|e| e.to_string())
}

/// The recorded percent of every element at a date, as numbers for colouring the 3D view.
pub fn percents_at(log: &ProgressLog, date: Date) -> BTreeMap<ElementId, f64> {
    log.percents_at(date).map(|m| m.into_iter().map(|(e, p)| (e, p.to_f64())).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;
    use afrobim_schedule::{Activity, ElementSelector, LinkAction, Weekday};

    // The numbers in these tests are fixtures chosen for easy hand arithmetic, not project data.

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    /// Two activities, each building one element; the wall is twice the pipe in value when priced.
    fn fixture() -> (Document, Programme, ElementId, ElementId) {
        let mut doc = Document::default();
        let wall = doc.add("wall", "W1", Pt::new(0.0, 0.0), 10.0, 10.0, "L", serde_json::Value::Null);
        let pipe = doc.add("pipe", "P1", Pt::new(0.0, 0.0), 10.0, 10.0, "L", serde_json::Value::Null);
        let mut p = schedule_glue::new_programme("Fixture", d(2026, 3, 2), "Five day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]).unwrap();
        p.add_activity(Activity::new("A", "1", "Walls", 10));
        p.add_activity(Activity::new("B", "2", "Pipes", 10));
        p.link("A", "B", afrobim_schedule::LinkType::FS, 0);
        p.link_elements("A", ElementSelector::Id(wall), LinkAction::Construct);
        p.link_elements("B", ElementSelector::Id(pipe), LinkAction::Construct);
        let s = afrobim_schedule::schedule(&p).unwrap();
        p.set_baseline("Fixture baseline", &s);
        (doc, p, wall, pipe)
    }

    fn entry(date: &str, percent: &str) -> RecordEntry {
        RecordEntry { date: date.into(), measure: Measure::Percent(percent.into()), recorded_by: "J. Test".into(), note: String::new(), files: vec![], correction: None }
    }

    #[test]
    fn a_record_for_several_elements_is_all_or_none_and_a_decrease_needs_a_reason() {
        let (_, _, wall, pipe) = fixture();
        let mut log = ProgressLog::new();
        assert_eq!(add_records(&mut log, &[wall, pipe], &entry("2026-03-04", "50")).unwrap(), 2);
        assert_eq!(log.records().len(), 2);
        // A lower percent is refused with the crate's message, and nothing is stored.
        let err = add_records(&mut log, &[wall, pipe], &entry("2026-03-05", "40")).unwrap_err();
        assert!(err.contains("a decrease needs a correction with a reason"), "{err}");
        assert_eq!(log.records().len(), 2);
        // The same record with a reason is stored as a correction; an empty reason is refused.
        let mut e = entry("2026-03-05", "40");
        e.correction = Some("   ".into());
        assert!(add_records(&mut log, &[wall], &e).unwrap_err().contains("a correction needs a reason"));
        e.correction = Some("re-measured, the first figure was too high".into());
        add_records(&mut log, &[wall], &e).unwrap();
        assert_eq!(log.records().len(), 3);
        // Missing entries are named.
        assert!(add_records(&mut log, &[wall], &entry("", "10")).unwrap_err().contains("date"));
        assert!(add_records(&mut log, &[], &entry("2026-03-06", "10")).unwrap_err().contains("no element"));
        let mut no_who = entry("2026-03-06", "60");
        no_who.recorded_by.clear();
        assert!(add_records(&mut log, &[wall], &no_who).unwrap_err().contains("who recorded"));
    }

    #[test]
    fn a_quantity_record_needs_a_target_and_the_filter_finds_records_by_element_and_date() {
        let (doc, _, wall, pipe) = fixture();
        let mut log = ProgressLog::new();
        let q = RecordEntry { measure: Measure::Quantity { measured: "4".into(), unit: "m".into() }, ..entry("2026-03-04", "") };
        assert!(add_records(&mut log, &[pipe], &q).unwrap_err().contains("input needed"));
        log.set_target(pipe, Decimal::parse("10").unwrap(), "m").unwrap();
        add_records(&mut log, &[pipe], &q).unwrap();
        assert_eq!(log.percent_of(&log.records()[0]).unwrap().to_string(), "40");
        add_records(&mut log, &[wall], &entry("2026-03-10", "25")).unwrap();
        assert_eq!(filtered_records(&log, Some(pipe), None, None).len(), 1);
        assert_eq!(filtered_records(&log, None, Some(d(2026, 3, 5)), None).len(), 1);
        assert_eq!(filtered_records(&log, None, None, None).len(), 2);
        let csv = records_csv(&doc, &log);
        assert!(csv.contains("P1") && csv.contains("4 m") && csv.contains("W1") && csv.starts_with("Date,Element"));
    }

    #[test]
    fn a_rule_picks_elements_by_category_and_the_selection_is_used_otherwise() {
        let (doc, _, wall, pipe) = fixture();
        let rule = ElementRule { category: Some("pipe".into()), layer: None, mark_pattern: None };
        assert_eq!(target_elements(&doc, &[wall], Some(&rule)), vec![pipe]);
        assert_eq!(target_elements(&doc, &[wall, pipe], None), vec![wall, pipe]);
        let by_mark = ElementRule { category: None, layer: None, mark_pattern: Some("W*".into()) };
        assert_eq!(target_elements(&doc, &[], Some(&by_mark)), vec![wall]);
    }

    #[test]
    fn the_progress_book_and_settings_survive_save_and_open() {
        let (mut doc, _, wall, _) = fixture();
        let mut book = ProgressBook::new();
        add_records(&mut book.log, &[wall], &entry("2026-03-04", "50")).unwrap();
        add_cost(&mut book, "2026-03-04", Some("A"), "1200", "supplier invoice").unwrap();
        let settings = Settings { data_date: "2026-03-06".into(), eac: Some(EacKind::BacOverCpi), look_ahead_days: "14".into(), ..Default::default() };
        let id = store(&mut doc, None, &book, &settings).unwrap();
        assert_eq!(store(&mut doc, Some(id), &book, &settings).unwrap(), id, "the book is updated, not added twice");
        assert_eq!(doc.elements().iter().filter(|e| e.category == PROGRESS_CATEGORY).count(), 1);
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        let (rid, back, back_settings) = load(&reopened).unwrap();
        assert_eq!(rid, id);
        assert_eq!(back, book);
        assert_eq!(back_settings, settings);
    }

    #[test]
    fn the_roll_up_reports_its_basis_and_the_update_applies_inferred_dates_and_reschedules() {
        let (doc, prog, wall, pipe) = fixture();
        let mut log = ProgressLog::new();
        add_records(&mut log, &[wall], &entry("2026-03-04", "50")).unwrap();
        add_records(&mut log, &[wall], &entry("2026-03-06", "100")).unwrap();
        add_records(&mut log, &[pipe], &entry("2026-03-09", "20")).unwrap();
        let r = rollup_at(&doc, &prog, None, &log, d(2026, 3, 9)).unwrap();
        let a = r.get("A").unwrap();
        assert_eq!(a.percent.to_string(), "100");
        assert_eq!(a.basis.label(), afrobim_progress::WeightBasis::Equal.label());
        assert!(a.actual_start.as_ref().unwrap().basis.starts_with("inferred") && a.actual_finish.is_some());
        assert_eq!(a.actual_finish.as_ref().unwrap().date, d(2026, 3, 6));
        let (next, report, schedule) = apply(&prog, &r).unwrap();
        assert_eq!(report.applied.len(), 2);
        assert!(report.inferred_dates.iter().any(|(id, what, date)| id == "A" && what == "actual start" && *date == d(2026, 3, 4)));
        assert_eq!(next.data_date, Some(d(2026, 3, 9)));
        assert_eq!(next.activity("B").unwrap().progress.percent_complete, 20.0);
        // The programme given is untouched until the user accepts; the schedule is made from the new data date.
        assert_eq!(prog.data_date, None);
        assert!(schedule.is_ok());
        // A date after the data date is refused by the crate.
        let mut late = r.clone();
        late.date = d(2026, 3, 3);
        assert!(apply(&prog, &late).is_err());
    }

    /// A hand-worked case. Budgets 1000 for A and 3000 for B (fixture values), both baselined over
    /// ten working days from Monday 2 March 2026. At the end of Friday 6 March, five days in:
    /// PV(A) = 1000 * 5 / 10 = 500 (B has not started), A is 40 % so EV = 400, and B is 0 %.
    /// With 600 actual cost: CV = 400 - 600 = -200, SV = 400 - 500 = -100, CPI = 400 / 600 = 0.666667,
    /// SPI = 400 / 500 = 0.8. EAC by AC + (BAC - EV) = 600 + (4000 - 400) = 4200.
    #[test]
    fn earned_value_for_a_small_hand_worked_case_agrees_with_the_crate() {
        let (doc, prog, wall, _) = fixture();
        let mut book = ProgressBook::new();
        add_records(&mut book.log, &[wall], &entry("2026-03-06", "40")).unwrap();
        let mut s = Settings { data_date: "2026-03-06".into(), budget_from_estimate: false, ..Default::default() };
        s.budget_text.insert("A".into(), "1000".into());
        s.budget_text.insert("B".into(), "3000".into());
        // No actual cost yet: the cost figures read input needed, never zero, and no formula is chosen.
        let out = evm(&doc, &prog, None, &book, &s, 0).unwrap();
        assert!(out.eac_unchosen);
        assert_eq!(out.evm.bac.to_string(), "4000");
        assert_eq!(out.evm.pv.to_string(), "500");
        assert_eq!(out.evm.ev.to_string(), "400");
        assert!(out.evm.ac.is_input_needed() && out.evm.cpi.is_input_needed() && out.evm.cv.is_input_needed());
        assert_eq!(out.evm.sv.to_string(), "-100");
        assert_eq!(out.evm.spi.value().unwrap().to_string(), "0.8");
        // Enter the actual cost and choose a formula.
        add_cost(&mut book, "2026-03-06", None, "600", "demo of a hand-worked case").unwrap();
        s.eac = Some(EacKind::AcPlusRemainingAtBudget);
        let out = evm(&doc, &prog, None, &book, &s, 0).unwrap();
        assert!(!out.eac_unchosen);
        assert_eq!(out.evm.ac.value().unwrap().to_string(), "600");
        assert_eq!(out.evm.cv.value().unwrap().to_string(), "-200");
        assert_eq!(out.evm.cpi.value().unwrap().to_string(), "0.666667");
        assert_eq!(out.evm.eac.value.value().unwrap().to_string(), "4200");
        assert_eq!(out.evm.eac.formula, EacFormula::AcPlusRemainingAtBudget.name());
        // The monthly table and the S-curve are the crate's, and the CSVs write.
        assert!(!out.periods.rows.is_empty() && out.curve.points.len() >= 2);
        let dir = std::env::temp_dir().join(format!("afrobim-evm-test-{}", std::process::id()));
        let files = write_csvs(&dir, &out, None, None).unwrap();
        assert_eq!(files, ["evm.csv", "evm-activities.csv", "evm-periods.csv", "evm-s-curve.csv"]);
        assert!(std::fs::read_to_string(dir.join("evm.csv")).unwrap().contains("4200"));
        let _ = std::fs::remove_dir_all(&dir);
        // Without a baseline, a data date or a budget the reasons are plain.
        let mut bare = prog.clone();
        bare.baseline = None;
        assert!(evm(&doc, &bare, None, &book, &s, 0).unwrap_err().contains("baseline"));
        s.data_date.clear();
        assert!(evm(&doc, &prog, None, &book, &s, 0).unwrap_err().contains("data date"));
        s.data_date = "2026-03-06".into();
        s.budget_text.clear();
        assert!(evm(&doc, &prog, None, &book, &s, 0).unwrap_err().contains("budget"));
        s.budget_from_estimate = true;
        assert!(evm(&doc, &prog, None, &book, &s, 0).unwrap_err().contains("estimate"));
    }

    #[test]
    fn the_look_ahead_needs_its_length_and_the_formula_entries_are_checked() {
        let (doc, prog, wall, _) = fixture();
        let mut log = ProgressLog::new();
        add_records(&mut log, &[wall], &entry("2026-03-06", "40")).unwrap();
        let r = rollup_at(&doc, &prog, None, &log, d(2026, 3, 6)).unwrap();
        let (next, _, schedule) = apply(&prog, &r).unwrap();
        let schedule = schedule.unwrap();
        assert!(look_ahead_and_delays(&next, &schedule, d(2026, 3, 6), "").unwrap_err().contains("days"));
        let (rows, delays) = look_ahead_and_delays(&next, &schedule, d(2026, 3, 6), "14").unwrap();
        assert!(!rows.is_empty() || delays.is_empty());
        // A weighted formula needs its weights, which the crate checks add to one.
        let mut s = Settings { eac: Some(EacKind::CpiSpiWeighted), ..Default::default() };
        assert!(s.formula().unwrap_err().contains("CPI weight"));
        s.cpi_weight = "0.5".into();
        s.spi_weight = "0.5".into();
        assert!(matches!(s.formula().unwrap(), Some(EacFormula::CpiSpiWeighted { .. })));
        assert_eq!(Settings::default().formula().unwrap(), None);
    }
}
