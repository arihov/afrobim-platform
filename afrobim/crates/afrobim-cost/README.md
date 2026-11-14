# afrobim-cost

The 5D stage of AfroBIM: a bill of quantities and cost estimate built from the quantities the
model already reports. It takes the take-off of each discipline crate, maps it onto a bill the
user defines, prices it from a rate library the user enters, adds the user's additions, phases the
cost in time and supports interim payments.

## What this crate does not contain

**No rates, prices, exchange rates, percentages (preliminaries, contingency, provisional sums,
dayworks, VAT, retention, advance recovery), productivity figures, item numbers or text of any
standard bill.** Every one of them is entered by the user or loaded from the user's file. The crate
does not offer the MoWT General Specifications series numbering or any other standard
structure: its specification contents list was not fetched and read from an official source, so
the structure is left to the user (`Bill::neutral(n)` gives numbered sections to rename, or load
your own with `Bill::from_structure_csv`). The numbers in the tests are fixtures chosen for easy
hand arithmetic and are not prices, rates or percentages of any contract.

The interim payment module encodes **no contract terms**: no FIDIC clause numbers, no percentages.
The user enters the terms of their own contract as rules; the module does arithmetic on them. It
does not decide what a contract entitles anyone to.

## Data flow

```text
Document elements
   | collect_all / collect_<crate>          one collector per crate, element by element
   v
QuantitySet { lines: QuantityLine { element, mark, category, description,
                                    quantity, unit, attributes } }
   | Bill::measure        mapping rules: category + attribute conditions, first rule wins
   v
MeasuredBill  items (lines, elements, adjustments, manual quantity -> quantity)
              unmapped lines (never dropped), ambiguous lines
   | Estimate::compute    rates (RateLibrary), currencies, rounding rules, additions
   v
EstimateResult  priced items, section totals, bill total, additions, grand total,
                UGX and USD side by side, unpriced items, the full audit trail
   |--> priced_boq_csv / unpriced_boq_csv / estimate_summary_csv / audit_csv / unmapped_csv
   |--> compare / Estimate::compare_revisions      change per item between two estimates or revisions
   |--> Estimate::cash_flow                        month by month and S-curve
   '--> certify / quantities_from_progress         interim certificates
```

A module `EstimateModule` (category `estimate`) lets an estimate live in the document: the element
stores the estimate the user entered and, after `refresh_estimate`, a summary. `generate` draws one
bar per section and reports the totals as derived values and the unmapped/unpriced/skipped counts
as notes.

## Money and quantities: `Decimal`

Amounts, rates, quantities, percentages and exchange rates are `Decimal`: an exact signed count of
millionths in an `i128`. Not `f64`: a float cannot hold 0.1 or 1250.35 exactly, so sums of rounded
lines drift from the rounded sum and a total disagrees with the lines above it. With `Decimal` every
sum is exact and every rounding is a stated decision. Six decimals cover a quantity (3 are normal), a
rate, an exchange rate and a percentage. The only place a float enters is `Decimal::from_f64`, where a
quantity computed by another crate becomes a line (`COLLECTED_DECIMALS` = 6 places). A product is
computed exactly (up to 12 decimals) and rounded once. `Decimal` serialises as a string.

Rounding modes: `HalfAwayFromZero` (2.5 gives 3, -2.5 gives -3), `HalfEven` (2.5 gives 2),
`AwayFromZero` (2.1 gives 3), `TowardZero` (2.9 gives 2).

## Collectors and the lines they produce

All are `collect_*(doc, &mut QuantitySet)`; `collect_all(doc, signs, options)` runs them in this order.
Each measures a document element with the crate's own take-off, so the line carries the element id and
mark. An element the crate cannot measure goes to `QuantitySet::skipped` with the crate's reason.

| collector (crate) | categories (unit) | attributes |
|---|---|---|
| `collect_signs` (fabrication) | `sign.face_area` m2, `sign.face` nr, `sign.legend` nr | kind, scheme, ground, catalog_key, speed_kph, width_mm, height_mm |
| `collect_supports` (supports) | `post.length` m, `post.count` nr, `post.coating_area` m2, `post.mass` kg; `foundation.concrete`, `.blinding`, `.excavation` m3, `foundation.formwork` m2, `foundation.count` nr | section, material, finish, length_mm, embedment_mm; shape, concrete, top_below_ground_mm |
| `collect_markings` (markings) | `marking.length` m, `marking.painted_length` m, `marking.painted_area` m2, `marking.marks` nr | line_type, diagram, width_mm, colour, material, alignment |
| `collect_drainage` (drainage) | `drain.length` m, `drain.excavation` m3, `drain.lining_area` m2, `drain.lining_volume` m3; `culvert.length` m, `culvert.barrels` nr | section, lining, lining_thickness_mm, alignment; internal_diameter_mm, wall_thickness_mm, material, barrels, alignment |
| `collect_gantries` (gantry) | `gantry.count` nr, `gantry.leg_length` m, `gantry.boom_length` m, `gantry.foundation_concrete` m3, `gantry.sign_area` m2, `gantry.steel_mass` kg (only when every member has a mass per metre) | gantry_type, alignment, span_or_reach_mm, legs |
| `collect_lighting` (lighting) | `lighting.column` nr, `lighting.luminaire` nr | mounting_height_m, arrangement; product, power_w |
| `collect_restraint` (restraint) | `barrier.length` m, `barrier.posts` nr, `barrier.terminal` nr (one per terminal end), `barrier.transition` nr (one per transition) | system, containment, working_width, impact_severity, traffic_face, alignment; terminal_class, terminal_description; connects_to, transition_containment |
| `collect_pavement` (pavement) | `pavement.layer_area` m2, `pavement.layer_volume` m3 (compacted), per layer of each strip | strip, layer, material, thickness_mm |
| `collect_corridors` / `collect_earthworks` (corridor) | `earthworks.cut` m3, `earthworks.fill` m3 | corridor |

Lines that measure one element in different ways (a post as length and as a count, a sign as area and
as a count) are alternatives: map one of them, or the element is priced twice. The alignment and
other crates have nothing to cost and are not collected.

Signs need the sign engine to lay out and measure (`afrobim_fabrication::measure`), so the caller passes
the resulting `SignQuantity` rows; the collector finds the element whose name equals the sign code.
Corridors are collected from the summary the corridor crate stores (`CorridorSummary`), so a corridor not
yet computed is skipped; `collect_earthworks` takes an `Earthworks` you hold instead.

## The bill

`Bill` holds `sections` (code, title), `items` (code, section, description, unit, kind, optional
`rate_code` and `rate_currency`), `rules`, `adjustments` and `rounding`. Item kinds: `Measured` (from the
model) and `Manual { quantity, reason }` (entered, reason required).

**Mapping rules** (`MappingRule`): category (exact, a trailing `*` for a prefix, `*` for all) and
conditions on attributes (`Equals`, `OneOf`, `Contains`, `Present`, `AtLeast`, `AtMost`). `mark`,
`description` and `unit` can be tested like attributes. Text compares trimmed and case-insensitive; two
numbers compare as numbers (`100` equals `100.0`). Rules are tried in order and a line goes to the first
rule that fits. A line another rule also fits is listed in `ambiguous`. The line's unit must equal the item's
unit unless the rule gives a `factor` (the user's own conversion, multiplied into the line quantity).
A line that fails the unit check is not mapped through that rule; if nothing else takes it, it is
unmapped with reason `UnitMismatch`.

**Unmapped lines** are never dropped: `MeasuredBill::unmapped` holds each with its reason (`NoRule` or
`UnitMismatch`), `lines_mapped + unmapped.len() == lines_offered`, and `audit_csv` / `unmapped_csv` list them.

**Adjustments** (`Adjustment { item, delta, reason }`): a reason is required. Validation (`Bill::validate`,
run by `measure`) refuses a bill with duplicate codes, a missing section or unit, a rule to a missing or
manual item, a manual item or adjustment without a reason, or a zero factor; all problems are listed.

**Audit trail**: for each item, every `Contribution` (line, element, mark, rule, factor, quantity), the
adjustments and the manual quantity; `MeasuredItem::elements()` and `quantity_of(element)`.

**CSV**: `structure_to_csv` / `from_structure_csv` (`kind,code,section,description,unit`; `kind` is
`section` or `item`) and `rules_to_csv` / `rules_from_csv` (`name,item,category,factor,conditions`;
conditions `key=value`, `key=a|b`, `key~text`, `key>=n`, `key<=n`, `key?`, separated by `;`).

## Formulas

Quantity of a measured item:

```text
from_lines = sum over mapped lines of (line.quantity * factor)       factor = 1 without one
unrounded  = from_lines + sum(adjustment.delta) + manual quantity
quantity   = round(unrounded, quantity rule for the item's unit)     default 3 decimals, half away
```

Rounding (`RoundingRules`): `quantity` (and `quantity_by_unit`, e.g. `nr` rounded up to whole
numbers), `amount_mode`, `amount_rounding` and `build_up`. The decimals of an amount are the
decimals of its currency (`Currency::decimals`, the user's choice).

Pricing, per item (`Estimate::compute`):

```text
rate        = library rate for rate_code (default: item code), in rate_currency if given,
              else in the reporting currency if there is one, else the first
amount      = round(quantity * rate, native currency decimals)          rate's currency
amount_rep  = convert(amount, native -> reporting), rounded to reporting decimals
amount_sec  = convert(amount, native -> secondary), rounded to secondary decimals
```

With `AmountRounding::TotalsOnly` item amounts keep full precision and sections are rounded; with
`PerItem` (default) every amount is rounded, so a total always equals the sum of what is printed.
An item with no rate, or with a rate per a different unit, is priced at zero and listed in `unpriced`.

```text
section total = sum of its items' amount_rep          (rounded under TotalsOnly)
bill total    = sum of section totals
```

Currency conversion (`CurrencyTable`): a rate is "1 `from` = `rate` `to`" with the date it was taken
and a note. Direct: `amount * rate`. Reverse: `amount / rate`. Both rounded once. The rate used is the
latest entered on or before `Estimate::as_of` (the latest of all when unset). No chaining through a third
currency. A missing rate is an error, never a guess. Secondary totals are sums of converted parts, so they
can differ by rounding from converting the grand total.

Additions (`Estimate::additions`), applied in list order, each rounded to the reporting currency:

```text
percentage = round(base * percent / 100)
  base: BillTotal | RunningTotal (bill total + additions above) | Section(code) | Additions([ids above])
lump sum   = amount in its currency, converted to reporting
grand total = bill total + sum of additions
```

An addition may refer only to additions above it.

Comparison (`compare`, `Estimate::compare_revisions`): per item code, quantity and amount old, new and
change; `percent_change = change * 100 / old amount` to 2 decimals, none when the old amount is zero;
status `Added`, `Removed`, `Changed`, `Unchanged`; then bill total, additions and grand total.

Rate build-up (`BuildUp`): direct components (labour, plant, materials, other) are entered amounts per
unit; overheads and profit are entered amounts or percentages of `Direct`, `DirectAndOverheads` or
`Running` (everything before). Each percentage amount is rounded by the build-up rule; the rate is the sum.

## Rates (CSV)

`RateLibrary::to_csv` / `from_csv`: `item_code,description,unit,rate,currency,source_note,date`
(date `YYYY-MM-DD`, may be empty). A rate that is not a number, or a bad date, is an error naming the
row. A library holds one rate per item code and currency. The library starts empty.

## Time-phased cost

Input (`Phasing`): `target` (`Item(code)`, `Element(id)`, `Addition(id)`), `start`, `finish`,
`distribution` (`Linear` only). The crate does not depend on the schedule crate; the application joins
its tasks to these.

```text
days = finish - start + 1                                  both days count
month amount = round(amount * days in month within the window / days)
last month   = amount - sum of the other months            so the months add back exactly
```

An item window phases the item's whole amount. An element window phases the element's quantity in each
item times that item's rate in the reporting currency, rounded once per item; items phased as a whole
are not phased twice (a warning says so). Whatever has no window is `unphased_total`
(`grand_total - phased_total`). Months run from the first to the last with gaps as zero rows;
`cumulative_percent` is cumulative over the phased total, 2 decimals; `CashFlow::s_curve()` gives a zero point on
the day before the first month and one point per month end. Worked: 100,000 over 20 Jan to 10 Mar 2026
is 50 days, so January 24,000 (12), February 56,000 (28), March 20,000 (10).

## Interim payment

`certify(result, terms, label, quantities_to_date, previous_state, rounding)` for one period:

```text
amount to date (item)  = round(quantity to date * rate in reporting currency)
amount this period     = amount to date - amount at previous certificate
gross to date          = sum of item amounts to date
```

An item missing from `quantities_to_date` stays at its previous quantity. Quantities from percent
complete: `quantities_from_progress` sums, per item, each element's quantity times its percent
complete, plus the item's adjustments and manual quantity times the item's own percent. Absent means 0%.

`PaymentTerms` rules, all entered by the user, cumulative to date with the period being the change:

```text
Retention        to date = min(cap, percent of gross to date)                  deducts
AdvanceRecovery  to date = min(advance, percent of max(0, gross to date - start_threshold))   deducts
PercentOfGross   ToDate:     percent of gross to date
                 ThisPeriod: previous + percent of this period's gross         adds or deducts
Lump             to date = previous + amount                                   adds or deducts
net this period = gross this period + additions - deductions (each this period)
net to date     = gross to date + additions - deductions (each to date)
```

Worked (fixtures): gross 60,000; 10% retention is 6,000; advance 30,000 recovered at 25% of gross above
20,000 is 10,000; a 5% addition of the period's gross is 3,000; net 47,000. Next period gross to date 170,000:
retention capped at 15,000 (9,000 this period), recovery 30,000 (20,000 this period).
`Certificate::state` is passed to the next period. Warnings: quantity to date above the bill quantity, or
below the previous.

## Outputs

`priced_boq_csv` (sections, items with quantity, rate, native amount, reporting amount and secondary
amount, section totals, bill total, additions, grand total), `unpriced_boq_csv` (quantities only),
`estimate_summary_csv`, `audit_csv`, `unmapped_csv`, `comparison_csv`, `cash_flow_csv`,
`certificate_csv`, `lines_to_csv`. Everything is serde (`Estimate`, `EstimateResult`, `Comparison`,
`Certificate`, `CashFlow`, ...).

## Call example

```rust
let mut est = Estimate::new("Road 12 markings", Currency::new("UGX", 0));
est.currencies.add_currency(Currency::new("USD", 2));
est.currencies.add_rate("USD", "UGX", rate_entered_by_user, rate_date, "source note")?;
est.secondary_currency = Some("USD".into());
est.bill.from_structure_csv(&std::fs::read_to_string("bill.csv")?)?;     // the user's structure
est.bill.rules_from_csv(&std::fs::read_to_string("rules.csv")?)?;
est.rates = RateLibrary::from_csv(&std::fs::read_to_string("rates.csv")?)?;
let id = add_estimate(&mut doc, "Estimate 1", &est)?;                    // in the document
let result = refresh_estimate(&mut doc, id, &sign_rows, &CollectOptions::default())?;
std::fs::write("boq.csv", priced_boq_csv(&result))?;
let (_, cash) = est.compute_with_cash_flow(&collect_all(&doc, &sign_rows, &Default::default()))?;
```

## Limits

- Only the collectors listed. Quantities the crates do not report (reinforcement, cable, earthwork
  by material class, pavement by chainage) are not here.
- One collected line per quantity per element; the bill cannot split a line between two items.
- Mapping is first-match; there is no weighting or percentage split between items.
- Exchange rates convert directly or by reciprocal only; no triangulation.
- Additions apply to the bill total, a section or other additions; not to individual items.
- Cash flow is linear by calendar day only, by month (no weeks, no calendars of working days).
- Payment rules base on gross value; no rule takes a percentage of the net after other deductions,
  no dayworks or variation orders as such (enter them as bill items or lump rules).
- A lump rule is added in every period it is certified, as the rule says "this period only" and the
  module has no memory of which periods apply: for a once-only amount put it in one period's terms.
- Amounts are limited by `i128` at 6 decimals, far beyond any national budget.
- Item text and rule names are free text; CSV conditions cannot contain `;` or `|` in values.

## Not done

- The MoWT General Specifications structure (series numbering): not offered, as its contents list
  was not read from an official source.
- Rates and productivity data of any kind, price indices and escalation, inflation or currency
  movement during the contract.
- Escalation and price adjustment formulae, valuation of variations, dayworks sheets, claims, final account.
- S-curve against a schedule: the schedule crate is not a dependency; the app supplies `Phasing`.
- Non-linear distributions (front-loaded, bell, user curve).
- Cost of elements whose crate has no quantity function (alignments, surfaces).
- Spreadsheet (xlsx) output; CSV only.
