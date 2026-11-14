//! Tests with hand-worked figures. The numbers are fixtures chosen for easy arithmetic: they are
//! not rates, prices, percentages or durations of any real project.

use std::collections::BTreeMap;

use afrobim_cost::{certify, Bill, Condition, Currency, Decimal, Estimate, EstimateResult, Item, MappingRule, PaymentTerms, QuantitySet, Rate, Rounding};
use afrobim_kernel::{Document, ElementId, Module, Pt};
use afrobim_schedule::{schedule, Activity, Calendar, Date, ElementSelector, LinkAction, Programme, Weekday};

use crate::*;

fn d(m: u32, day: u32) -> Date {
    Date::from_ymd(2026, m, day).unwrap()
}

fn dec(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}

/// Test data only: Monday to Friday, no holidays.
fn five_day() -> Calendar {
    Calendar::new("Five-day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri])
}

fn elements(n: usize) -> (Document, Vec<ElementId>) {
    let mut doc = Document::default();
    let ids = (0..n).map(|i| doc.add("pipe", &format!("P{}", i + 1), Pt::new(0.0, 0.0), 1.0, 1.0, "Pipes", serde_json::Value::Null)).collect();
    (doc, ids)
}

fn money2() -> Rounding {
    Rounding::half_up(2)
}

/// The March world. Programme start Monday 2 March 2026, Monday to Friday, no holidays.
///
/// A "Earthworks" 5 working days: Mon 2, Tue 3, Wed 4, Thu 5, Fri 6 March.
/// B "Pavement" 5 working days, finish-to-start after A: Mon 9 to Fri 13 March.
/// Baseline = this schedule. Elements 1 and 2 build A, element 3 builds B.
/// Budgets (user input fixtures): A 1000.00, B 2000.00, so BAC = 3000.00.
struct World {
    doc: Document,
    e: Vec<ElementId>,
    prog: Programme,
}

fn march_world() -> World {
    let (doc, e) = elements(3);
    let mut prog = Programme::new("March", d(3, 2), five_day());
    prog.add_activity(Activity::new("A", "1", "Earthworks", 5));
    prog.add_activity(Activity::new("B", "2", "Pavement", 5));
    prog.link("A", "B", afrobim_schedule::LinkType::FS, 0);
    prog.link_elements("A", ElementSelector::Id(e[0]), LinkAction::Construct);
    prog.link_elements("A", ElementSelector::Id(e[1]), LinkAction::Construct);
    prog.link_elements("B", ElementSelector::Id(e[2]), LinkAction::Construct);
    let s = schedule(&prog).unwrap();
    assert_eq!(s.row("A").unwrap().early_finish, d(3, 6));
    assert_eq!(s.row("B").unwrap().early_start, d(3, 9));
    prog.set_baseline("Baseline 1", &s);
    World { doc, e, prog }
}

fn budget_ab() -> Budget {
    let m: BTreeMap<String, Decimal> = [("A".to_string(), dec("1000")), ("B".to_string(), dec("2000"))].into_iter().collect();
    Budget::from_activity_values(m, "test fixture").unwrap()
}

/// Records of the March world, data date Thursday 5 March.
fn march_log(w: &World) -> ProgressLog {
    let mut log = ProgressLog::new();
    log.add(Record::percent(w.e[0], d(3, 3), dec("50"), "site engineer")).unwrap();
    log.add(Record::percent(w.e[0], d(3, 5), dec("80"), "site engineer")).unwrap();
    log.add(Record::percent(w.e[1], d(3, 4), dec("100"), "site engineer")).unwrap();
    log
}

fn user_weights(w: &World) -> BTreeMap<ElementId, Decimal> {
    [(w.e[0], dec("3")), (w.e[1], dec("1")), (w.e[2], dec("1"))].into_iter().collect()
}

fn march_costs() -> ActualCosts {
    let mut c = ActualCosts::new();
    c.add(d(3, 3), None, dec("300"), "period entry");
    c.add(d(3, 5), Some("A"), dec("400"), "activity entry");
    c.add(d(3, 9), None, dec("999"), "after the data date");
    c
}

fn engine<'a>(w: &'a World, budget: &'a Budget, costs: &'a ActualCosts, weights: &'a BTreeMap<ElementId, Decimal>, eac: EacFormula) -> EvmEngine<'a> {
    EvmEngine::new(EvmSetup {
        programme: &w.prog,
        document: &w.doc,
        weights: WeightSources { user: Some(weights), estimate: None },
        budget,
        costs,
        rounding: money2(),
        eac,
    })
    .unwrap()
}

// ---------------------------------------------------------------- records

#[test]
fn records_validate_and_state_is_the_latest_on_or_before() {
    let mut log = ProgressLog::new();
    let e = ElementId(1);
    log.add(Record::percent(e, d(3, 3), dec("50"), "AB").with_note("first").with_file("photos/p1.jpg")).unwrap();
    log.add(Record::percent(e, d(3, 5), dec("80"), "AB")).unwrap();
    // The state on a date is the latest record on or before it.
    assert!(log.state_at(e, d(3, 2)).unwrap().is_none());
    assert_eq!(log.state_at(e, d(3, 3)).unwrap().unwrap().percent, dec("50"));
    assert_eq!(log.state_at(e, d(3, 4)).unwrap().unwrap().percent, dec("50"));
    assert_eq!(log.state_at(e, d(3, 5)).unwrap().unwrap().percent, dec("80"));
    assert_eq!(log.state_at(e, d(4, 1)).unwrap().unwrap().record_date, d(3, 5));
    // Out of range, nobody named, and an empty correction reason are refused and not stored.
    assert!(log.add(Record::percent(e, d(3, 6), dec("100.5"), "AB")).is_err());
    assert!(log.add(Record::percent(e, d(3, 6), dec("-1"), "AB")).is_err());
    assert!(log.add(Record::percent(e, d(3, 6), dec("90"), " ")).is_err());
    assert!(log.add(Record::percent(e, d(3, 6), dec("70"), "AB").as_correction("")).is_err());
    // A decrease without a correction is refused; with a reason it is accepted.
    let err = log.add(Record::percent(e, d(3, 6), dec("70"), "AB")).unwrap_err().to_string();
    assert!(err.contains("correction"), "{err}");
    assert_eq!(log.records().len(), 2);
    log.add(Record::percent(e, d(3, 6), dec("70"), "AB").as_correction("re-measured: the 80% included stockpile")).unwrap();
    assert_eq!(log.state_at(e, d(3, 6)).unwrap().unwrap().percent, dec("70"));
    // A record dated between two others that would make the later one (80% on the 5th) a decrease is
    // refused: 50, 90, 80. One of 75% fits (50, 75, 80, then the correction 70).
    assert!(log.add(Record::percent(e, d(3, 4), dec("90"), "AB")).is_err());
    log.add(Record::percent(e, d(3, 4), dec("75"), "AB")).unwrap();
    // Records on the same date count in the order added.
    log.add(Record::percent(e, d(3, 7), dec("71"), "AB")).unwrap();
    log.add(Record::percent(e, d(3, 7), dec("72"), "AB")).unwrap();
    assert_eq!(log.state_at(e, d(3, 7)).unwrap().unwrap().percent, dec("72"));
}

#[test]
fn quantity_records_become_percent_against_a_target() {
    // Target 200 m. 50 m is 50 * 100 / 200 = 25%. 75 m is 37.5%.
    let mut log = ProgressLog::new();
    let e = ElementId(7);
    let q = Record::quantity(e, d(3, 3), dec("50"), "m", "AB");
    // Without a target the quantity cannot become a percent: input needed.
    let err = log.add(q.clone()).unwrap_err().to_string();
    assert!(err.contains("input needed"), "{err}");
    log.set_target(e, dec("200"), "m").unwrap();
    log.add(q).unwrap();
    log.add(Record::quantity(e, d(3, 4), dec("75"), "M", "AB")).unwrap();
    assert_eq!(log.state_at(e, d(3, 3)).unwrap().unwrap().percent, dec("25"));
    assert_eq!(log.state_at(e, d(3, 4)).unwrap().unwrap().percent, dec("37.5"));
    // Another unit, more than the target, and a lower target that breaks a record are refused.
    assert!(log.add(Record::quantity(e, d(3, 5), dec("80"), "m2", "AB")).is_err());
    assert!(log.add(Record::quantity(e, d(3, 5), dec("200.5"), "m", "AB")).is_err());
    assert!(log.set_target(e, dec("60"), "m").is_err());
    assert_eq!(log.target_of(e).unwrap().quantity, dec("200"));
    // The whole log survives serde.
    let v = serde_json::to_value(&log).unwrap();
    let back: ProgressLog = serde_json::from_value(v).unwrap();
    assert_eq!(back, log);
}

#[test]
fn start_and_completion_dates_follow_the_records() {
    let mut log = ProgressLog::new();
    let e = ElementId(1);
    log.add(Record::percent(e, d(3, 2), dec("0"), "AB")).unwrap();
    log.add(Record::percent(e, d(3, 3), dec("40"), "AB")).unwrap();
    log.add(Record::percent(e, d(3, 4), dec("100"), "AB")).unwrap();
    log.add(Record::percent(e, d(3, 5), dec("90"), "AB").as_correction("part was rework")).unwrap();
    log.add(Record::percent(e, d(3, 6), dec("100"), "AB")).unwrap();
    log.add(Record::percent(e, d(3, 9), dec("100"), "AB")).unwrap();
    // A 0% record is not a start.
    assert_eq!(log.start_date(e, d(3, 3)).unwrap(), Some(d(3, 3)));
    assert_eq!(log.start_date(e, d(3, 2)).unwrap(), None);
    // At 100% on 4 March the run began on the 4th; after the correction and a new 100% it began on the 6th.
    assert_eq!(log.completion_date(e, d(3, 4)).unwrap(), Some(d(3, 4)));
    assert_eq!(log.completion_date(e, d(3, 5)).unwrap(), None);
    assert_eq!(log.completion_date(e, d(3, 9)).unwrap(), Some(d(3, 6)));
}

// ---------------------------------------------------------------- roll-up

#[test]
fn rollup_by_user_weights() {
    // Activity A: element 1 (weight 3) at 80%, element 2 (weight 1) at 100% on Thursday 5 March.
    // (3 * 80 + 1 * 100) / (3 + 1) = 340 / 4 = 85%.
    // Activity B: element 3 has no record, 0%.
    let w = march_world();
    let log = march_log(&w);
    let weights = user_weights(&w);
    let r = Rollup::new(&w.prog, &w.doc, &WeightSources { user: Some(&weights), estimate: None }).unwrap();
    let res = r.at(&log, d(3, 5)).unwrap();
    let a = res.get("A").unwrap();
    assert_eq!(a.percent, dec("85"));
    assert_eq!(a.basis, WeightBasis::UserWeights);
    assert_eq!((a.elements_total, a.elements_recorded), (2, 2));
    assert_eq!(res.get("B").unwrap().percent, dec("0"));
    // The start is inferred from the first record above 0%: element 1 on 3 March. Not complete: no finish.
    let start = a.actual_start.as_ref().unwrap();
    assert_eq!(start.date, d(3, 3));
    assert!(start.basis.starts_with("inferred"));
    assert!(a.actual_finish.is_none());
    // On 4 March element 1 is at 50%: (3 * 50 + 100) / 4 = 62.5%.
    assert_eq!(r.at(&log, d(3, 4)).unwrap().get("A").unwrap().percent, dec("62.5"));
}

#[test]
fn rollup_falls_back_to_equal_weights_and_says_so() {
    // Only element 1 has a user weight, so A cannot use them: equal weights, (80 + 100) / 2 = 90%.
    let w = march_world();
    let log = march_log(&w);
    let partial: BTreeMap<ElementId, Decimal> = [(w.e[0], dec("3"))].into_iter().collect();
    let r = Rollup::new(&w.prog, &w.doc, &WeightSources { user: Some(&partial), estimate: None }).unwrap();
    let a = r.at(&log, d(3, 5)).unwrap();
    let a = a.get("A").unwrap();
    assert_eq!(a.percent, dec("90"));
    assert_eq!(a.basis, WeightBasis::Equal);
    assert!(a.basis_note.as_ref().unwrap().contains("user weights missing"));
    // No sources at all: equal weights, no note.
    let r = Rollup::new(&w.prog, &w.doc, &WeightSources::default()).unwrap();
    assert_eq!(r.activities[0].basis, WeightBasis::Equal);
    assert!(r.activities[0].basis_note.is_none());
    // A weight of zero is refused.
    let zero: BTreeMap<ElementId, Decimal> = w.e.iter().map(|e| (*e, dec("0"))).collect();
    assert!(Rollup::new(&w.prog, &w.doc, &WeightSources { user: Some(&zero), estimate: None }).is_err());
}

/// Two items: X (m, rate 100) measured on element 1 as 10 m, Y (m, rate 300) on element 2 as 10 m.
/// X = 10 * 100 = 1000, Y = 10 * 300 = 3000, bill 4000.
fn priced(ids: &[ElementId]) -> EstimateResult {
    let mut est = Estimate::new("fixture", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("X", "1", "Item X", "m"), Item::measured("Y", "1", "Item Y", "m")];
    est.bill.rules = vec![
        MappingRule::new("x", "X", "x", vec![Condition::equals("which", "x")]),
        MappingRule::new("y", "Y", "y", vec![]),
    ];
    for (code, rate) in [("X", "100"), ("Y", "300")] {
        est.rates.set(Rate { item_code: code.into(), description: "".into(), unit: "m".into(), rate: dec(rate), currency: "UGX".into(), source_note: "fixture".into(), date: None });
    }
    let mut set = QuantitySet::new();
    set.push_decimal(Some(ids[0]), "P1", "x", "", dec("10"), "m", &[("which", "x".to_string())]);
    set.push_decimal(Some(ids[1]), "P2", "y", "", dec("10"), "m", &[]);
    est.compute(&set).unwrap()
}

fn one_activity_world() -> (Document, Vec<ElementId>, Programme) {
    let (doc, e) = elements(2);
    let mut prog = Programme::new("One", d(3, 2), five_day());
    prog.add_activity(Activity::new("A", "1", "Pipework", 5));
    prog.link_elements("A", ElementSelector::Id(e[0]), LinkAction::Construct);
    prog.link_elements("A", ElementSelector::Id(e[1]), LinkAction::Construct);
    (doc, e, prog)
}

#[test]
fn rollup_by_priced_value_and_budget_from_the_estimate() {
    // Element 1 is worth 1000 and element 2 is worth 3000. Element 1 at 100%, element 2 at 50%:
    // (1000 * 100 + 3000 * 50) / (1000 + 3000) = 250000 / 4000 = 62.5%.
    let (doc, e, prog) = one_activity_world();
    let result = priced(&e);
    let mut log = ProgressLog::new();
    log.add(Record::percent(e[0], d(3, 5), dec("100"), "AB")).unwrap();
    log.add(Record::percent(e[1], d(3, 5), dec("50"), "AB")).unwrap();
    let r = Rollup::new(&prog, &doc, &WeightSources { user: None, estimate: Some(&result) }).unwrap();
    let a = r.at(&log, d(3, 5)).unwrap();
    assert_eq!(a.get("A").unwrap().percent, dec("62.5"));
    assert_eq!(a.get("A").unwrap().basis, WeightBasis::PricedValue);
    // The budget of A is the sum of its elements' values: 1000 + 3000 = 4000, nothing unallocated.
    let b = Budget::from_estimate(&r, &result).unwrap();
    assert_eq!(b.by_activity["A"], dec("4000"));
    assert_eq!(b.unallocated, Some(dec("0")));
    // An element reached by two activities cannot be split by the crate.
    let mut prog2 = prog.clone();
    prog2.add_activity(Activity::new("B", "2", "Other", 2));
    prog2.link_elements("B", ElementSelector::Id(e[0]), LinkAction::Temporary);
    let r2 = Rollup::new(&prog2, &doc, &WeightSources { user: None, estimate: Some(&result) }).unwrap();
    assert_eq!(r2.shared_elements, vec![e[0]]);
    assert!(Budget::from_estimate(&r2, &result).is_err());
}

#[test]
fn rollup_resolves_rules_and_never_picks_up_its_own_element() {
    let (mut doc, e) = elements(2);
    let mut prog = Programme::new("Rule", d(3, 2), five_day());
    prog.add_activity(Activity::new("A", "1", "All pipes", 5));
    prog.link_elements("A", ElementSelector::Rule(afrobim_schedule::ElementRule { layer: Some("Pipes".into()), ..Default::default() }), LinkAction::Construct);
    // The progress element sits on its own layer, but a rule by category "pipe" must also not catch it.
    add_progress(&mut doc, "Progress", &ProgressBook::new());
    let r = Rollup::new(&prog, &doc, &WeightSources::default()).unwrap();
    let ids: Vec<ElementId> = r.activities[0].elements.iter().map(|(e, _)| *e).collect();
    assert_eq!(ids, e);
    // Missing ids are reported, and an activity with no element is listed as unlinked.
    prog.add_activity(Activity::new("B", "2", "No elements", 1));
    prog.link_elements("B", ElementSelector::Id(ElementId(999)), LinkAction::Construct);
    let r = Rollup::new(&prog, &doc, &WeightSources::default()).unwrap();
    assert_eq!(r.unlinked_activities, vec!["B".to_string()]);
    assert_eq!(r.missing_elements, vec![("B".to_string(), ElementId(999))]);
}

#[test]
fn updates_apply_to_the_programme_and_the_schedule_reschedules() {
    // Data date Wednesday 4 March. Element 1 (A's only element) is at 10% on the 2nd and 40% on the 4th.
    // A: percent 40, inferred actual start 2 March. Remaining = ceil(5 * 60 / 100) = 3 working days from
    // Thursday 5 March: 5, 6 and 9 March, so A now finishes Monday 9 March (baseline: Friday 6 March).
    // B follows finish-to-start: starts Tuesday 10 March, 5 days: 10, 11, 12, 13 and 16 March.
    let mut w = march_world();
    let mut log = ProgressLog::new();
    log.add(Record::percent(w.e[0], d(3, 2), dec("10"), "AB")).unwrap();
    log.add(Record::percent(w.e[0], d(3, 4), dec("40"), "AB")).unwrap();
    let r = Rollup::new(&w.prog, &w.doc, &WeightSources::default()).unwrap();
    let res = r.at(&log, d(3, 4)).unwrap();
    // A has two elements; element 2 has no record, so with equal weights A is (40 + 0) / 2 = 20%. Link only element 1.
    assert_eq!(res.get("A").unwrap().percent, dec("20"));
    w.prog.links.retain(|l| !matches!(&l.selector, ElementSelector::Id(id) if *id == w.e[1]));
    let r = Rollup::new(&w.prog, &w.doc, &WeightSources::default()).unwrap();
    let res = r.at(&log, d(3, 4)).unwrap();
    assert_eq!(res.get("A").unwrap().percent, dec("40"));
    let report = apply_updates(&mut w.prog, d(3, 4), &res.updates()).unwrap();
    assert!(report.inferred_dates.contains(&("A".to_string(), "actual start".to_string(), d(3, 2))));
    assert_eq!(w.prog.data_date, Some(d(3, 4)));
    assert_eq!(w.prog.activity("A").unwrap().progress.actual_start, Some(d(3, 2)));
    assert_eq!(w.prog.activity("A").unwrap().progress.percent_complete, 40.0);
    let s = schedule(&w.prog).unwrap();
    assert_eq!(s.row("A").unwrap().early_finish, d(3, 9));
    assert_eq!(s.row("B").unwrap().early_start, d(3, 10));
    assert_eq!(s.row("B").unwrap().early_finish, d(3, 16));

    // Delay list: A finishes 1 working day late (Fri 6 to Mon 9), B finishes 1 working day late (Fri 13 to Mon 16).
    let delays = delay_list(&w.prog, &s).unwrap();
    assert_eq!(delays.len(), 2);
    assert!(delays.iter().all(|r| r.finish_variance == 1));
    assert!(delay_csv(&delays).contains("A,Earthworks,in progress,2026-03-02,2026-03-06,2026-03-02,2026-03-09,1,0,"));

    // Look-ahead from the data date, 7 days: 5 to 11 March. A finishes on the 9th, B starts on the 10th.
    let la = look_ahead(&s, d(3, 4), 7);
    assert_eq!(la.iter().map(|r| (r.id.as_str(), r.event)).collect::<Vec<_>>(), vec![("A", Event::Finishes), ("B", Event::Starts)]);
    // 14 days reach the 18th: B now also finishes inside the window.
    let la = look_ahead(&s, d(3, 4), 14);
    assert_eq!(la[1].event, Event::StartsAndFinishes);
    assert!(look_ahead_csv(&la).contains("B,2,Pavement,not started,starts and finishes,2026-03-10,2026-03-16,"));
    // A window that ends before anything is due is empty.
    assert!(look_ahead(&s, d(3, 4), 3).is_empty());
    // Updates dated after the data date are refused.
    let mut late = res.updates();
    late[0].actual_start = Some(InferredDate { date: d(3, 5), basis: "x".into() });
    assert!(apply_updates(&mut w.prog, d(3, 4), &late).is_err());
}

#[test]
fn a_finished_activity_gets_an_inferred_finish() {
    // Both elements of A reach 100%: element 1 on 5 March, element 2 on 4 March. The activity
    // finishes on the later, 5 March; it started on the earliest first record, 3 March.
    let w = march_world();
    let mut log = ProgressLog::new();
    log.add(Record::percent(w.e[0], d(3, 3), dec("50"), "AB")).unwrap();
    log.add(Record::percent(w.e[0], d(3, 5), dec("100"), "AB")).unwrap();
    log.add(Record::percent(w.e[1], d(3, 4), dec("100"), "AB")).unwrap();
    let r = Rollup::new(&w.prog, &w.doc, &WeightSources::default()).unwrap();
    let a = r.at(&log, d(3, 5)).unwrap();
    let a = a.get("A").unwrap();
    assert_eq!(a.percent, dec("100"));
    assert_eq!(a.actual_finish.as_ref().unwrap().date, d(3, 5));
    assert_eq!(a.actual_start.as_ref().unwrap().date, d(3, 3));
}

// ---------------------------------------------------------------- earned value

#[test]
fn earned_value_at_5_march() {
    // BAC = 1000 + 2000 = 3000.00.
    // PV on Thu 5 March: A's baseline span Mon 2 to Fri 6 March is N = 5 working days, and Mon 2 to
    //   Thu 5 is n = 4, so PV(A) = 1000 * 4 / 5 = 800.00. B starts on 9 March: PV(B) = 0. PV = 800.00.
    // EV: A is 85% (see rollup_by_user_weights): 1000 * 85 / 100 = 850.00. B is 0%. EV = 850.00.
    // AC (entered): 300 on 3 March (period entry) + 400 on 5 March (activity A) = 700.00. The 999 on
    //   9 March is after the data date and is left out.
    // SV = 850 - 800 = 50.00.        CV = 850 - 700 = 150.00.
    // SPI = 850 / 800 = 1.0625.      CPI = 850 / 700 = 1.2142857..., 1.214286 at 6 decimals.
    let w = march_world();
    let log = march_log(&w);
    let (budget, costs, weights) = (budget_ab(), march_costs(), user_weights(&w));
    let eng = engine(&w, &budget, &costs, &weights, EacFormula::AcPlusRemainingAtBudget);
    let r = eng.evm(&log, d(3, 5)).unwrap();
    assert_eq!(r.bac, dec("3000"));
    assert_eq!(r.pv, dec("800"));
    assert_eq!(r.ev, dec("850"));
    assert_eq!(r.ac, Figure::Value(dec("700")));
    assert_eq!(r.sv, dec("50"));
    assert_eq!(r.cv, Figure::Value(dec("150")));
    assert_eq!(r.spi, Figure::Value(dec("1.0625")));
    assert_eq!(r.cpi, Figure::Value(dec("1.214286")));
    // Formula 1: EAC = AC + (BAC - EV) = 700 + 2150 = 2850.00. ETC = 2850 - 700 = 2150.00.
    // VAC = 3000 - 2850 = 150.00.
    // TCPI to BAC = (3000 - 850) / (3000 - 700) = 2150 / 2300 = 0.9347826..., 0.934783.
    // TCPI to EAC = 2150 / (2850 - 700) = 1.
    assert_eq!(r.eac.formula, "AC plus remaining work at budget");
    assert_eq!(r.eac.value, Figure::Value(dec("2850")));
    assert_eq!(r.etc, Figure::Value(dec("2150")));
    assert_eq!(r.vac, Figure::Value(dec("150")));
    assert_eq!(r.tcpi_bac, Figure::Value(dec("0.934783")));
    assert_eq!(r.tcpi_eac, Figure::Value(dec("1")));
    // Per activity: A has 800 / 850 and its own entered AC of 400; B has none entered.
    let a = &r.activities[0];
    assert_eq!((a.id.as_str(), a.pv, a.ev, a.percent_complete), ("A", dec("800"), dec("850"), dec("85")));
    assert_eq!(a.ac, Figure::Value(dec("400")));
    assert!(r.activities[1].ac.is_input_needed());
    assert!(r.activities.iter().all(|a| a.measured));
}

#[test]
fn each_eac_formula_is_named_and_matches_its_hand_figure() {
    // Same figures: BAC 3000, PV 800, EV 850, AC 700. CPI = 850/700 = 17/14, SPI = 850/800 = 17/16.
    // 1. AC + (BAC - EV)                  = 700 + 2150                          = 2850.00
    // 2. BAC / CPI = BAC * AC / EV        = 3000 * 700 / 850 = 2470.588235...   = 2470.59
    // 3. AC + (BAC - EV) / (CPI * SPI)    CPI * SPI = 289/224 = 1.290178571...
    //    2150 / 1.290178571 = 1666.435986..., EAC = 2366.435986...              = 2366.44
    // 4. weights 0.8 and 0.2: 0.8 * 17/14 + 0.2 * 17/16 = 0.971428... + 0.2125 = 1.183928...
    //    2150 / 1.183928... = 1815.98793..., EAC = 2515.98793...                = 2515.99
    // 5. bottom-up ETC entered as 2000: 700 + 2000                              = 2700.00
    let w = march_world();
    let log = march_log(&w);
    let (budget, costs, weights) = (budget_ab(), march_costs(), user_weights(&w));
    let cases = [
        (EacFormula::AcPlusRemainingAtBudget, "AC plus remaining work at budget", "2850"),
        (EacFormula::BacOverCpi, "BAC divided by CPI", "2470.59"),
        (EacFormula::CpiAndSpi, "AC plus remaining work divided by CPI times SPI", "2366.44"),
        (EacFormula::CpiSpiWeighted { cpi_weight: dec("0.8"), spi_weight: dec("0.2") }, "AC plus remaining work divided by weighted CPI and SPI", "2515.99"),
        (EacFormula::BottomUp { etc: dec("2000") }, "AC plus bottom-up ETC (user estimate)", "2700"),
    ];
    for (formula, name, expected) in cases {
        let r = engine(&w, &budget, &costs, &weights, formula).evm(&log, d(3, 5)).unwrap();
        assert_eq!(r.eac.formula, name);
        assert_eq!(r.eac.value, Figure::Value(dec(expected)), "{name}");
        assert!(r.eac.definition.starts_with("EAC = "));
        // ETC and VAC follow the chosen EAC: ETC = EAC - AC, VAC = BAC - EAC.
        let e = *r.eac.value.value().unwrap();
        assert_eq!(r.etc, Figure::Value(e - dec("700")));
        assert_eq!(r.vac, Figure::Value(dec("3000") - e));
    }
    // Weights that do not add to 1 are refused.
    let bad = EacFormula::CpiSpiWeighted { cpi_weight: dec("0.8"), spi_weight: dec("0.3") };
    assert!(EvmEngine::new(EvmSetup {
        programme: &w.prog,
        document: &w.doc,
        weights: WeightSources::default(),
        budget: &budget,
        costs: &costs,
        rounding: money2(),
        eac: bad
    })
    .is_err());
}

#[test]
fn without_actual_cost_the_cost_figures_are_input_needed_not_zero() {
    let w = march_world();
    let log = march_log(&w);
    let (budget, weights) = (budget_ab(), user_weights(&w));
    let none = ActualCosts::new();
    let r = engine(&w, &budget, &none, &weights, EacFormula::BacOverCpi).evm(&log, d(3, 5)).unwrap();
    // Schedule figures do not need AC: SV = 850 - 800 = 50, SPI = 1.0625.
    assert_eq!(r.sv, dec("50"));
    assert_eq!(r.spi, Figure::Value(dec("1.0625")));
    for f in [&r.ac, &r.cv, &r.cpi, &r.eac.value, &r.etc, &r.vac, &r.tcpi_bac, &r.tcpi_eac] {
        assert!(f.is_input_needed(), "{f:?}");
    }
    // The formula is still named, and the CSV says "input needed".
    assert_eq!(r.eac.formula, "BAC divided by CPI");
    let csv = evm_csv(&r);
    assert!(csv.contains("CPI,input needed,EV / AC"), "{csv}");
    assert!(csv.contains("EAC (BAC divided by CPI),input needed,"), "{csv}");
    assert!(csv.contains("SPI,1.0625,EV / PV"), "{csv}");
    // An entered zero is not the same as nothing entered: AC = 0 gives an undefined CPI, not "input needed".
    let mut zero = ActualCosts::new();
    zero.add(d(3, 3), None, dec("0"), "nothing spent yet");
    let r = engine(&w, &budget, &zero, &weights, EacFormula::BacOverCpi).evm(&log, d(3, 5)).unwrap();
    assert_eq!(r.ac, Figure::Value(dec("0")));
    assert!(matches!(r.cpi, Figure::Undefined(_)));
    assert!(matches!(r.eac.value, Figure::Undefined(_)));
}

#[test]
fn planned_value_is_linear_in_working_days_of_the_programme_calendar() {
    // A: baseline Mon 2 to Fri 6 March, 5 working days, BAC 1000: 200 per working day.
    let w = march_world();
    let (budget, costs, weights) = (budget_ab(), ActualCosts::new(), user_weights(&w));
    let eng = engine(&w, &budget, &costs, &weights, EacFormula::AcPlusRemainingAtBudget);
    assert_eq!(eng.planned_value("A", d(3, 1)), dec("0")); // Sunday before the start
    assert_eq!(eng.planned_value("A", d(3, 2)), dec("200"));
    assert_eq!(eng.planned_value("A", d(3, 6)), dec("1000"));
    assert_eq!(eng.planned_value("A", d(3, 8)), dec("1000")); // the weekend after
    assert_eq!(eng.planned_value("B", d(3, 10)), dec("800")); // B: 2000 * 2 / 5
    // A rounding case: 1000 / 3 per day over a 3-day activity.
    let mut p = Programme::new("Three", d(3, 2), five_day());
    p.add_activity(Activity::new("T", "1", "Three days", 3));
    let s = schedule(&p).unwrap();
    p.set_baseline("b", &s);
    let m: BTreeMap<String, Decimal> = [("T".to_string(), dec("1000"))].into_iter().collect();
    let b = Budget::from_activity_values(m, "fixture").unwrap();
    let eng = EvmEngine::new(EvmSetup {
        programme: &p,
        document: &w.doc,
        weights: WeightSources::default(),
        budget: &b,
        costs: &costs,
        rounding: money2(),
        eac: EacFormula::AcPlusRemainingAtBudget,
    })
    .unwrap();
    // 1000 * 1 / 3 = 333.333..., 333.33; 1000 * 2 / 3 = 666.666..., 666.67.
    assert_eq!(eng.planned_value("T", d(3, 2)), dec("333.33"));
    assert_eq!(eng.planned_value("T", d(3, 3)), dec("666.67"));
}

#[test]
fn engine_refuses_missing_baseline_and_unknown_activities() {
    let w = march_world();
    let (budget, costs) = (budget_ab(), ActualCosts::new());
    let mut no_base = w.prog.clone();
    no_base.baseline = None;
    fn setup<'a>(w: &'a World, costs: &'a ActualCosts, p: &'a Programme, b: &'a Budget) -> EvmSetup<'a> {
        EvmSetup {
            programme: p,
            document: &w.doc,
            weights: WeightSources::default(),
            budget: b,
            costs,
            rounding: money2(),
            eac: EacFormula::AcPlusRemainingAtBudget,
        }
    }
    assert!(EvmEngine::new(setup(&w, &costs, &no_base, &budget)).is_err());
    let m: BTreeMap<String, Decimal> = [("ZZ".to_string(), dec("5"))].into_iter().collect();
    let bad_budget = Budget::from_activity_values(m, "x").unwrap();
    assert!(EvmEngine::new(setup(&w, &costs, &w.prog, &bad_budget)).is_err());
    assert!(Budget::from_activity_values([("A".to_string(), dec("-1"))].into_iter().collect(), "x").is_err());
}

// ---------------------------------------------------------------- periods, curve, CSV

/// January and February 2026. One activity T "Pipe" of 5 working days from Thursday 29 January:
/// 29, 30 January, 2, 3, 4 February. Budget 1000.00. One element, record 40% on 30 January and
/// 100% on 3 February. AC entered: 350 on 29 January, 700 on 4 February. Data date 10 February.
fn two_month() -> (World2, Budget, ActualCosts) {
    let (doc, e) = elements(1);
    let mut prog = Programme::new("Two months", d(1, 29), five_day());
    prog.add_activity(Activity::new("T", "1", "Pipe", 5));
    prog.link_elements("T", ElementSelector::Id(e[0]), LinkAction::Construct);
    let s = schedule(&prog).unwrap();
    assert_eq!((s.row("T").unwrap().early_start, s.row("T").unwrap().early_finish), (d(1, 29), d(2, 4)));
    prog.set_baseline("b", &s);
    let mut log = ProgressLog::new();
    log.add(Record::percent(e[0], d(1, 30), dec("40"), "AB")).unwrap();
    log.add(Record::percent(e[0], d(2, 3), dec("100"), "AB")).unwrap();
    let b = Budget::from_activity_values([("T".to_string(), dec("1000"))].into_iter().collect(), "fixture").unwrap();
    let mut c = ActualCosts::new();
    c.add(d(1, 29), None, dec("350"), "");
    c.add(d(2, 4), Some("T"), dec("700"), "");
    (World2 { doc, prog, log }, b, c)
}

struct World2 {
    doc: Document,
    prog: Programme,
    log: ProgressLog,
}

fn engine2<'a>(w: &'a World2, b: &'a Budget, c: &'a ActualCosts) -> EvmEngine<'a> {
    EvmEngine::new(EvmSetup {
        programme: &w.prog,
        document: &w.doc,
        weights: WeightSources::default(),
        budget: b,
        costs: c,
        rounding: money2(),
        eac: EacFormula::AcPlusRemainingAtBudget,
    })
    .unwrap()
}

#[test]
fn the_period_table_and_its_csv() {
    // January: PV at 31 January = 1000 * 2 / 5 = 400 (29 and 30 January); EV = 40% of 1000 = 400; AC = 350.
    // February (to the data date, 10 February): PV at 28 February = 1000; EV = 100% = 1000; AC = 350 + 700 = 1050.
    // Period amounts are the differences: PV 600, EV 600, AC 700.
    let (w, b, c) = two_month();
    let eng = engine2(&w, &b, &c);
    let t = eng.period_table(&w.log, d(2, 10)).unwrap();
    assert_eq!(t.rows.len(), 2);
    assert_eq!(
        period_csv(&t),
        "period,from,to,as_of,pv_period,pv_cumulative,ev_period,ev_cumulative,ac_period,ac_cumulative\n\
         2026-01,2026-01-01,2026-01-31,2026-01-31,400.00,400.00,400.00,400.00,350.00,350.00\n\
         2026-02,2026-02-01,2026-02-28,2026-02-10,600.00,1000.00,600.00,1000.00,700.00,1050.00\n"
    );
    // With the data date at 31 January, February is planned only: no EV and no AC yet.
    let t = eng.period_table(&w.log, d(1, 31)).unwrap();
    assert_eq!(t.rows.len(), 2);
    assert!(period_csv(&t).contains("2026-02,2026-02-01,2026-02-28,2026-01-31,600.00,1000.00,undefined (after the data date)"));
    // Without AC the AC columns say "input needed".
    let none = ActualCosts::new();
    let eng = engine2(&w, &b, &none);
    let csv = period_csv(&eng.period_table(&w.log, d(2, 10)).unwrap());
    assert!(csv.contains("400.00,400.00,400.00,400.00,input needed,input needed"), "{csv}");
}

#[test]
fn the_s_curve_data_and_its_csv() {
    let (w, b, c) = two_month();
    let eng = engine2(&w, &b, &c);
    let curve = eng.s_curve(&w.log, d(2, 10)).unwrap();
    assert_eq!(
        s_curve_csv(&curve),
        "date,pv,ev,ac\n\
         2025-12-31,0.00,0.00,0.00\n\
         2026-01-31,400.00,400.00,350.00\n\
         2026-02-10,1000.00,1000.00,1050.00\n\
         2026-02-28,1000.00,undefined (after the data date),undefined (after the data date)\n"
    );
    assert_eq!(curve.bac, dec("1000"));
    // The drawing has a polyline per series that has values: PV 3 segments, EV 2, AC 2, plus grid and axes.
    let (drawing, labels) = s_curve_drawing(&curve, 1000.0, 500.0);
    assert_eq!(drawing.primitives.len(), 5 + 2 + 3 + 2 + 2);
    assert!(labels.iter().any(|l| l.text == "PV") && labels.iter().any(|l| l.text == "EV") && labels.iter().any(|l| l.text == "AC"));
    // Without AC no red line is drawn.
    let none = ActualCosts::new();
    let curve = engine2(&w, &b, &none).s_curve(&w.log, d(2, 10)).unwrap();
    let (drawing, labels) = s_curve_drawing(&curve, 1000.0, 500.0);
    assert_eq!(drawing.primitives.len(), 5 + 2 + 3 + 2);
    assert!(!labels.iter().any(|l| l.text == "AC"));
}

#[test]
fn the_evm_csv_lists_every_measure_with_its_definition() {
    let w = march_world();
    let log = march_log(&w);
    let (budget, costs, weights) = (budget_ab(), march_costs(), user_weights(&w));
    let r = engine(&w, &budget, &costs, &weights, EacFormula::CpiAndSpi).evm(&log, d(3, 5)).unwrap();
    let csv = evm_csv(&r);
    assert!(csv.contains("BAC,3000.00,sum of the activity budgets"));
    assert!(csv.contains("CPI,1.214286,EV / AC"));
    assert!(csv.contains("EAC (AC plus remaining work divided by CPI times SPI),2366.44,EAC = AC + (BAC - EV) / (CPI * SPI)"));
    assert!(csv.contains("TCPI to BAC,0.934783,"));
    let acts = evm_activity_csv(&r);
    assert!(acts.contains("A,Earthworks,1000.00,800.00,85.00,850.00,400.00,true"));
    assert!(acts.contains("B,Pavement,2000.00,0.00,0.00,0.00,input needed,true"));
}

// ---------------------------------------------------------------- payment, persistence

#[test]
fn progress_becomes_the_quantities_certify_expects() {
    // Item X: 10 m at 100. Item Y: 10 m at 300. Element 1 (item X) is at 100%.
    // Element 2 (item Y) has a quantity record: 5 m against its target of 10 m (taken from the
    // estimate) = 50%. Quantities to date: X = 10, Y = 5. Gross = 10 * 100 + 5 * 300 = 2500.
    let (_doc, e, _prog) = one_activity_world();
    let result = priced(&e);
    let mut log = ProgressLog::new();
    log.set_target_from_estimate(&result, e[1], "Y").unwrap();
    assert_eq!(log.target_of(e[1]).unwrap().quantity, dec("10"));
    assert_eq!(log.target_of(e[1]).unwrap().unit, "m");
    log.add(Record::percent(e[0], d(3, 5), dec("100"), "AB")).unwrap();
    log.add(Record::quantity(e[1], d(3, 5), dec("5"), "m", "AB")).unwrap();
    // An element no bill item measures does not get paid; it is reported.
    log.add(Record::percent(ElementId(500), d(3, 5), dec("20"), "AB")).unwrap();
    let rules = afrobim_cost::RoundingRules::default();
    let q = qty_to_date(&result, &log, d(3, 5), &BTreeMap::new(), &rules).unwrap();
    assert_eq!(q.quantities["X"], dec("10"));
    assert_eq!(q.quantities["Y"], dec("5"));
    assert_eq!(q.elements_without_item, vec![ElementId(500)]);
    let c = certify(&result, &PaymentTerms::default(), "IPC 1", &q.quantities, None, Rounding::half_up(0)).unwrap();
    assert_eq!(c.gross_this_period, dec("2500"));
    // On 4 March nothing was measured yet: the quantities are zero.
    let q0 = qty_to_date(&result, &log, d(3, 4), &BTreeMap::new(), &rules).unwrap();
    assert_eq!(q0.quantities["X"], dec("0"));
}

#[test]
fn the_book_lives_in_the_document_and_the_module_draws_the_curve() {
    let (mut doc, e) = elements(1);
    let mut prog = Programme::new("Two months", d(1, 29), five_day());
    prog.add_activity(Activity::new("T", "1", "Pipe", 5));
    prog.link_elements("T", ElementSelector::Id(e[0]), LinkAction::Construct);
    let s = schedule(&prog).unwrap();
    prog.set_baseline("b", &s);
    let budget = Budget::from_activity_values([("T".to_string(), dec("1000"))].into_iter().collect(), "fixture").unwrap();

    let mut book = ProgressBook::new();
    book.log.add(Record::percent(e[0], d(1, 30), dec("40"), "AB")).unwrap();
    book.log.add(Record::percent(e[0], d(2, 3), dec("100"), "AB")).unwrap();
    let id = add_progress(&mut doc, "Progress", &book);
    assert_eq!(doc.find(id).unwrap().category, PROGRESS_CATEGORY);
    assert_eq!(load_progress(&doc, id).unwrap(), book);
    // Before a refresh the module says so and draws nothing.
    let g = ProgressModule.generate(doc.find(id).unwrap()).unwrap();
    assert!(g.drawing.primitives.is_empty());
    assert!(g.notes.iter().any(|n| n.code == "not_refreshed"));

    // Refresh with no actual cost entered: the stored report says "input needed", and the module notes it.
    let params = RefreshParams { programme: &prog, weights: WeightSources::default(), budget: &budget, rounding: money2(), eac: EacFormula::BacOverCpi };
    let rep = refresh_progress(&mut doc, id, &params, d(2, 10)).unwrap();
    assert!(rep.evm.ac.is_input_needed());
    let g = ProgressModule.generate(doc.find(id).unwrap()).unwrap();
    assert!(!g.drawing.primitives.is_empty());
    assert_eq!(g.derived["BAC"], "1000.00");
    assert_eq!(g.derived["EV"], "1000.00");
    assert_eq!(g.derived["AC"], "input needed");
    assert_eq!(g.derived["EAC (BAC divided by CPI)"], "input needed");
    assert!(g.notes.iter().any(|n| n.code == "ac_input_needed"));

    // Enter costs, store, refresh again: AC 1050, CPI = 1000 / 1050 = 0.952381, EAC = 1000 * 1050 / 1000 = 1050.
    let mut book = load_progress(&doc, id).unwrap();
    book.costs.add(d(1, 29), None, dec("350"), "");
    book.costs.add(d(2, 4), Some("T"), dec("700"), "");
    store_progress(&mut doc, id, &book).unwrap();
    let rep = refresh_progress(&mut doc, id, &params, d(2, 10)).unwrap();
    assert_eq!(rep.evm.cpi, Figure::Value(dec("0.952381")));
    assert_eq!(rep.evm.eac.value, Figure::Value(dec("1050")));
    let g = ProgressModule.generate(doc.find(id).unwrap()).unwrap();
    assert_eq!(g.derived["AC"], "1050.00");
    // The whole book survives a round trip through the document's JSON.
    let back = load_progress(&doc, id).unwrap();
    assert_eq!(back.report.as_ref().unwrap().evm.data_date, d(2, 10));
    // A non-progress element is refused.
    assert!(load_progress(&doc, e[0]).is_err());
}
