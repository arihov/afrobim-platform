# afrobim-progress

The construction stage of AfroBIM: site progress records per model element, roll-up to the
programme, earned value at a data date, period reporting, and the link to interim payment.

## What this crate does not contain

**No rate, percentage, threshold, productivity or "typical" value of any kind.** Weights, budgets,
actual costs, the EAC formula and its weights, rounding, the look-ahead length: all are entered by
the user. The crate encodes **no FIDIC clause logic** and no contract terms. It does arithmetic on
what it is given; it does not decide what anyone is entitled to. The numbers in the tests are
fixtures chosen for easy hand arithmetic, not data of any project.

## Sources

**No external source was held while writing this crate.** The PMI Practice Standard for Earned
Value Management and the PMBOK Guide were neither fetched nor available, so nothing below is
quoted or cited from them. Every quantity is defined algebraically from first principles in this
README and in the doc comment of the code that computes it. Where a name (EAC, TCPI) is in common
use, the formula implemented is the one written here, and the output names it. If your contract or
organisation defines a quantity differently, check this README against that definition before using
the figure.

## Dependencies

`afrobim-schedule` (Programme, Schedule, Date, element links), `afrobim-cost` (`Decimal`,
`EstimateResult`, `quantities_from_progress`), `afrobim-kernel`, `afrobim-draw`, `serde`,
`serde_json`, `thiserror`. Dates are the schedule crate's `Date`; money, percentages and quantities
are the cost crate's exact `Decimal` (an `i128` count of millionths), never `f64`. The only float is
the percent complete written to the schedule crate's `Progress` (it stores `f64`).

## Progress records

A `Record` is one site measurement of one element: element id, date, a `Measurement`, who recorded
it (required), a note, file paths (paths only, never opened) and an optional correction reason.

* `Measurement::Percent(p)`: percent complete, 0 to 100.
* `Measurement::Quantity { measured, unit }`: quantity done to date. It becomes a percent
  `measured * 100 / target` (6 decimals) against the element's target quantity, set with
  `ProgressLog::set_target` or taken from a priced estimate with `set_target_from_estimate` (the
  quantity the element contributes to a bill item, in the item's unit). A quantity record with no
  target, in a different unit, or above the target is refused (the first with "input needed").

`ProgressLog::add` is append-only and atomic: a refused record is not stored. It refuses a percent
outside 0 to 100, a negative quantity, a missing recorder, an empty correction reason, and a
**decrease** against the previous record (or, for a record dated earlier than others, a record that
would make a later one a decrease) unless the lower record is marked `as_correction(reason)`.

**State at a date** (`state_at`, `percents_at`): the latest record on or before the date; records on
the same date count in the order added; no record means no state (0% in roll-ups).

## Roll-up to the activities

The elements of an activity are the ones its `ActivityLink`s reach (by id, or by rule resolved
against the document each time), whatever the link action: Construct, Demolish and Temporary all
count, because progress is progress of the work. Schedule and progress elements are never picked up
by a rule.

```text
activity percent = sum(w_i * p_i) / sum(w_i)       over the activity's elements
```

`p_i` is the element's percent on the date (0 with no record). The sum is exact in `Decimal` units
and rounded once, half away from zero, at 6 decimals. **Weights**, chosen per activity from
`WeightSources` in this order, and reported in the result as `basis` with a `basis_note` when a
preferred source was skipped:

1. `UserWeights`: a weight above zero per element, used when it covers all the activity's elements.
2. `PricedValue`: the element's value in the estimate, `EstimateResult::element_cost`, used when
   every element of the activity has a value above zero (so none silently weighs nothing).
3. `Equal`: every element weighs the same.

**Inferred dates.** Actual start is the earliest date, over the activity's elements, of the first
record above 0% (an element standing at 0% on the date has no start). Actual finish is set only when
the activity is at 100%: the latest date over its elements at which each reached 100% in its current
unbroken run (a correction down and back up starts a new run). Both are returned as `InferredDate`
with a `basis` text that begins "inferred", and `apply_updates` lists every date it wrote in
`ApplyReport::inferred_dates`.

**Applying to a programme.** `rollup.at(&log, date)?.updates()` gives `ActivityUpdate`s;
`apply_updates(&mut programme, data_date, &updates)` sets `programme.data_date`, and for each
updated activity replaces `percent_complete`, `actual_start`, `actual_finish` (a missing inferred
date clears the field) and clears `remaining_duration` so the schedule crate's own default applies.
Activities that no element reaches keep whatever progress they have. Updates dated after the data
date are refused. Then `afrobim_schedule::schedule(&programme)` reschedules from the data date.

## Earned value

Notation: for each activity `a` with a budget, `BAC(a)` is its budget at completion in the reporting
currency. The activity must have baseline dates (`programme.set_baseline`).

```text
BAC = sum over activities of BAC(a)
```

**Budget** (`Budget`): entered per activity (`from_activity_values`), or from a priced estimate
(`from_estimate`): `BAC(a)` is the sum over the activity's elements of the element's priced value
(`element_cost`, in the reporting currency). Adjustments and manual items belong to no element and
stay unallocated (`Budget::unallocated`, outside BAC, reported in the result notes); additions to
the bill are outside too. An element reached by two activities cannot be split by the crate, and
`from_estimate` is then refused: enter the activity budgets.

**PV, planned value**, at the end of a date `x`, from the baseline dates of each activity, linear in
working days of the **programme calendar** (not the activity's own calendar):

```text
N = working days from baseline start to baseline finish, both counted
n = working days from baseline start to min(x, baseline finish), both counted   (0 before the start)
PV(a) = round(BAC(a) * n / N)            PV = sum of PV(a)
```

An activity with N = 0 (a milestone, or no working day in its span) has PV(a) = BAC(a) from its
baseline finish date, zero before.

**EV, earned value**: `EV(a) = round(BAC(a) * percent complete(a) / 100)`, with the percent from the
roll-up at the date; `EV = sum of EV(a)`. A budgeted activity with no linked element has no progress
source: EV zero, flagged `measured = false` and noted.

**AC, actual cost**, is an **input** and is never derived: `ActualCosts` holds entries (date,
optional activity, amount, note). `AC = sum of the entries dated on or before the data date`. No
entries at all means not entered. Per-activity `AC` is the sum of the entries naming that activity
(input needed when it has none). An entered zero is a value, not a missing input.

```text
SV  = EV - PV                       CV  = EV - AC
SPI = EV / PV                       CPI = EV / AC
ETC = EAC - AC                      VAC = BAC - EAC
TCPI to BAC = (BAC - EV) / (BAC - AC)       TCPI to EAC = (BAC - EV) / (EAC - AC)
```

**Missing and undefined figures** (`Figure<T>`): `Value`, `InputNeeded(what)` or `Undefined(why)`.
With no AC entered, AC, CV, CPI, EAC, ETC, VAC and both TCPI are `InputNeeded` (CSV: "input needed"),
never zero; SV and SPI do not need AC and are still reported. A ratio is shown only where its
denominator is above zero, otherwise `Undefined` with the reason (a presentation decision of this
crate, not a standard). Ratios are rounded to 6 decimals; money to the rounding the caller gives.

**EAC, estimate at completion**, is chosen by the caller as an `EacFormula`; there is no default and
the result carries the formula's name and definition (`EacOutcome`):

| Variant | Formula | Meaning |
|---|---|---|
| `AcPlusRemainingAtBudget` | `EAC = AC + (BAC - EV)` | the remaining work costs what was budgeted |
| `BacOverCpi` | `EAC = BAC / CPI = BAC * AC / EV` | the whole job ends at the cost performance so far |
| `CpiAndSpi` | `EAC = AC + (BAC - EV) / (CPI * SPI)` | remaining work scaled by both indices |
| `CpiSpiWeighted { cpi_weight, spi_weight }` | `EAC = AC + (BAC - EV) / (wc * CPI + ws * SPI)` | user weights, each at least 0, adding to 1 |
| `BottomUp { etc }` | `EAC = AC + ETC`, ETC entered | the user's own estimate of the remaining work |

`BacOverCpi` and `CpiAndSpi` are computed from the amounts, not from rounded indices:
`BAC * AC / EV` is one exact step, and `(BAC - EV) * AC * PV / EV^2` is two exact steps each carried
at 6 decimals, then EAC is rounded once. `CpiSpiWeighted` uses the indices rounded to 6 decimals. A
formula whose index is undefined or zero (EV, AC or PV not above zero where needed) gives
`Undefined`.

Which formula is right depends on why the cost deviated and on the project; the crate does not
advise. Name the formula you used wherever you quote the EAC.

## Period reporting

* `EvmEngine::period_table(&log, data_date)`: one row per calendar month from the month of the
  earliest baseline start of a budgeted activity to the month of the later of the last baseline
  finish and the data date. PV is cumulative at the month end. EV and AC are cumulative at the month
  end, or at the data date in the month that contains it; later months show "undefined (after the
  data date)". Period amounts are differences of cumulative values.
* `EvmEngine::s_curve(&log, data_date)`: points at the day before the first month (zero), each
  month end and the data date, each with cumulative PV, EV, AC. `s_curve_drawing` draws it
  (PV blue, EV green, AC red; a series with no values is not drawn) and returns the labels.
* `look_ahead(&schedule, from, days)`: activities not started whose early start, or not complete
  whose early finish, falls in `from + 1` to `from + days` (calendar days, inclusive; `from` is the
  end of its day), from a schedule computed after `apply_updates`.
* `delay_list(&programme, &schedule)`: activities whose finish is later than the baseline finish,
  worst first, with the variance in working days of the activity's own calendar (the schedule
  crate's convention, working days in `[baseline finish, forecast finish)`).
* CSV: `evm_csv`, `evm_activity_csv`, `period_csv`, `s_curve_csv`, `look_ahead_csv`, `delay_csv`.
  Money has the decimals of the rounding given; unavailable figures read "input needed" or
  "undefined (reason)".

## Link to payment

`qty_to_date(&estimate_result, &log, date, &item_percent, &bill.rounding)` returns the
`quantities` map `afrobim_cost::certify` expects as `quantities_to_date`. It calls the cost crate's
`quantities_from_progress` with each element's percent on the date; `item_percent` gives the percent
for adjustments and manual quantities, which belong to no element (absent means 0%). Elements above
0% that feed no bill item are listed in `elements_without_item`, because they will not be paid.

## Persistence and module

`ProgressBook { log, costs, report }` serialises with serde and lives in the document as one element
of category `progress` (`PROGRESS_CATEGORY`): `add_progress`, `store_progress`, `load_progress`.
`ProgressModule` implements `afrobim_kernel::Module`. Its `generate` sees only the element, so it
draws the S-curve and reports the figures **stored by the last `refresh_progress`**, which recomputes
the figures and curve at a data date from the book and the document and stores them. `Generated`
carries no labels, so a front end that wants the axis text calls `s_curve_drawing` on
`book.report.curve`.

## API example

```rust
use afrobim_progress::*;
use afrobim_schedule::{schedule, Date};
use afrobim_cost::{Decimal, Rounding};

// Site measurements (element ids come from the document).
let mut log = ProgressLog::new();
log.set_target_from_estimate(&result, pipe_id, "2.01")?;               // optional, for quantity records
log.add(Record::percent(wall_id, Date::from_ymd(2026, 3, 3).unwrap(), Decimal::parse("50")?, "J. Okello")
    .with_note("formwork up").with_file("photos/wall-0303.jpg"))?;
log.add(Record::quantity(pipe_id, date, Decimal::parse("12.5")?, "m", "J. Okello"))?;

// Roll up, apply, reschedule.
let weights = WeightSources { user: None, estimate: Some(&result) };    // priced value, else equal
let rollup = Rollup::new(&programme, &doc, &weights)?;
let progress = rollup.at(&log, data_date)?;
apply_updates(&mut programme, data_date, &progress.updates())?;
let sched = schedule(&programme)?;                                      // programme needs a baseline set earlier

// Earned value. Budget, costs and the EAC formula are the caller's.
let budget = Budget::from_estimate(&rollup, &result)?;                  // or from_activity_values
let mut costs = ActualCosts::new();
costs.add(date, Some("A"), Decimal::parse("400000")?, "supplier invoice");   // input, never derived
let engine = EvmEngine::new(EvmSetup {
    programme: &programme, document: &doc, weights, budget: &budget, costs: &costs,
    rounding: Rounding::half_up(0), eac: EacFormula::BacOverCpi,
})?;
let (evm, table, curve) = engine.report(&log, data_date)?;
std::fs::write("evm.csv", evm_csv(&evm))?;
std::fs::write("periods.csv", period_csv(&table))?;
std::fs::write("look_ahead.csv", look_ahead_csv(&look_ahead(&sched, data_date, 28)))?;
std::fs::write("delays.csv", delay_csv(&delay_list(&programme, &sched)?))?;

// Payment: quantities to date for afrobim_cost::certify.
let q = qty_to_date(&result, &log, data_date, &Default::default(), &estimate.bill.rounding)?;
let cert = afrobim_cost::certify(&result, &terms, "IPC 3", &q.quantities, previous.as_ref(), Rounding::half_up(0))?;

// In the document.
let book = ProgressBook { log, costs, report: None };
let id = add_progress(&mut doc, "Progress", &book);
refresh_progress(&mut doc, id, &RefreshParams { programme: &programme, weights, budget: &budget,
    rounding: Rounding::half_up(0), eac: EacFormula::BacOverCpi }, data_date)?;
```

## Limits and not done

* Activities with no linked element have no progress source: their EV is zero and they are flagged.
  There are no activity-level progress records (only element records) and no resource, labour or
  duration based progress.
* An element reached by more than one activity counts its progress in each; its priced value cannot
  be split, so `Budget::from_estimate` refuses and the budgets must be entered.
* PV is linear in programme-calendar working days between baseline dates, nothing else (no
  front-loaded or user curves). It uses one baseline (the schedule crate keeps one).
* Actual cost is entered by hand. There is no import from accounts, no accruals, no distinction of
  committed and incurred cost, and no escalation or currency conversion (one reporting currency).
* Weights and budgets of the roll-up are as stable as the model: a rule link picks up new elements
  and changes the percent of an activity retroactively when it is rebuilt at an earlier date.
* Period reporting is by calendar month only. EV and AC after the data date are not forecast;
  there is no forecast S-curve from the rescheduled programme.
* Inferred actual dates are inferences from records. Replace them in the programme by hand if the
  site diary says otherwise (re-applying updates will overwrite them).
* Records hold file paths only; photos and files are not read, stored or checked.
* The EAC and TCPI formulae listed are the ones implemented; no other variant is offered, and the
  definitions are not taken from a published standard (see Sources).
* Time-location charts, claims, variation valuation and extension of time analysis are not done.
