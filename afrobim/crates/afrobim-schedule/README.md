# afrobim-schedule

The 4D stage of AfroBIM: a construction programme (critical path method) linked to model
elements. It computes dates and float over working-day calendars, tracks progress against a
baseline, says what every linked model element looks like on any date, draws a Gantt chart and an
S-curve, and exchanges programmes with Microsoft Project (MSPDI XML).

Nothing is assumed. There are no default durations, productivities or holidays: the working
weekdays and every day off of a calendar are entered by the user.

## Dependencies

`afrobim-draw`, `afrobim-kernel`, `serde`, `serde_json`, `thiserror`, and `roxmltree` 0.20. There
is no date crate: `Date` is days since 1970-01-01 with Gregorian conversion written here (about 60
lines, tested), so the crate pulls no `chrono`. `roxmltree` is already in the workspace for
`afrobim-corridor`; it is used only to read MSPDI, and the writer is hand-made.

## Conventions

* A date is a whole calendar day. An activity of `d` working days starts at the **start** of its
  early start day and finishes at the **end** of its early finish day, which is `d - 1` working days
  of its own calendar later. A start milestone sits at the start of its day, a finish milestone at
  the end of its day (`MilestoneKind`, default finish, like MS Project). Use a start milestone for
  "Notice to Proceed"; a finish milestone with no predecessor falls on the end of the first working
  day.
* Lag is in working days of the **successor's** calendar, in both passes. Negative lag is overlap.
* Float is in working days of the activity's own calendar.
* The data date is the **end** of that day. Remaining work cannot happen before the next working
  day. Actual dates may be on the data date, not after it.

Notation: `D(n)` is the n-th working day of a calendar, `b(x)` the number of working days before
date `x` (so `D(b(x))` is the first working day on or after `x`), `shift(w, n)` moves `n` working
days from working day `w`. Calendar `C` is the one of the activity being computed.

## Forward pass (early dates)

For each activity `i` in topological order, every predecessor `p` gives a lower bound. With the
predecessor's effective start `S_p` and finish `F_p` (its early start and finish; for a milestone,
the start milestone's finish is the working day before it and the finish milestone's start is the
working day after it) and lag `L`:

| Type | Bound | Formula |
|------|-------|---------|
| FS | start of `i` | `shift(first working day on or after F_p + 1 day, L)` |
| SS | start of `i` | `shift(first working day on or after S_p, L)` |
| FF | finish of `i` | `shift(first working day on or after F_p, L)` |
| SF | finish of `i` | `shift(first working day on or after S_p, L - 1)` |

A finish bound `f` gives the start `shift(f, -(d - 1))`. The early start is the largest bound, also
bounded below by the first working day on or after the programme start (or, with progress, the day
after the data date) and by a *start no earlier than* date. *Must start on* replaces the result with
that date; if the logic wanted later, the activity is flagged (`mso_overrides_logic`) and the float of
its predecessors goes negative in the backward pass. Early finish `= shift(ES, d - 1)`. A
milestone takes the start bound directly if it is a start milestone (a finish bound `f` gives the
first working day after `f`); a finish milestone takes the finish bound directly (a start bound
`s` gives the working day before `s`).

SF with lag 0 means the successor finishes the working day before the predecessor starts, the way
FS with lag 0 starts it the working day after the predecessor finishes. Working time is treated
as continuous across non-working days for these two, as P6 and MS Project do.

The project finish is the latest early finish. The finish limit is the *required finish* (a
contract completion date) when one is entered, else the project finish.

## Backward pass (late dates)

In reverse topological order. A terminal activity (no live successor) has late finish
`last working day on or before the finish limit`. Each successor `s`, with its late start `LS_s` and
late finish `LF_s` (effective values for milestones), gives `p` an upper bound, in `C_s` for the
shift and then snapped back with `last working day on or before` in `C_p`:

| Type | Bound on `p` | Formula |
|------|--------------|---------|
| FS | finish | `last_on_or_before_p(shift_s(LS_s, -L) - 1 day)` |
| SS | start | `last_on_or_before_p(shift_s(LS_s, -L))` |
| FF | finish | `last_on_or_before_p(shift_s(LF_s, -L))` |
| SF | start | `last_on_or_before_p(shift_s(LF_s, 1 - L))` |

The late start is the smallest bound (finish bounds converted with `shift(f, -(d - 1))`), also
capped by *finish no later than* and *must start on*. Late finish `= shift(LS, d - 1)`. These are
the exact inverses of the forward formulae. A completed activity keeps its actual dates and
constrains nothing.

## Float and criticality

* **Total float** `= working days in [EF, LF)`, negative when `LF` is before `EF`: the number of
  working days the finish can move without moving the finish limit.
* **Negative float** arises when a *finish no later than* constraint, a *must start on* date or the
  required finish cannot be met: the late dates fall before the early dates. It is reported in the
  row, and as a `negative_float` warning. It propagates back along the logic to every activity that
  drives the missed date.
* **Free float** `= min over successors of (working days from the date the relationship alone
  requires of the successor to the successor's early date)`, counted in the successor's calendar, and
  not more than total float. With no successor it equals total float. Successors already started
  or complete are skipped.
* **Critical** means total float of zero or less and not complete. With a required finish later than
  the computed finish nothing is critical; the longest path is then the chain with the smallest
  float. Longest-path marking is not implemented.

## Cycles and dangling logic

`validate` returns every issue; `schedule` refuses a programme with any error and the error text
lists them. Errors: logic loop (`cycle`, with the activity path `A -> B -> C -> A`), a dependency naming
an unknown activity (`unknown_activity`), a link from an activity to itself, duplicate ids, unknown
calendar, a calendar with no working weekday, bad progress. Warnings: open start (no predecessor),
open end (no successor), duplicate links.

## Progress and rescheduling

Activity progress: percent complete, actual start and finish, remaining duration (default
`ceil(duration * (100 - percent) / 100)`, at least 1), all as of the programme's `data_date`.
Complete work keeps its actual dates. Work in progress keeps its actual start; its remaining
duration is scheduled from the first working day after the data date or later if an unfinished
predecessor's logic demands it (**retained logic**: FS, FF and SF links still apply, SS links are
spent because the activity has started). Work not started cannot start on or before the data date.
Call `schedule` again after changing progress: that is the reschedule. A row's `duration` for work
started is the span from actual start to its (actual or forecast) finish.

## Baseline

`programme.set_baseline(name, &schedule)` stores start, finish, duration and total float per
activity. `programme.variance(&schedule)` returns current minus baseline for start, finish,
duration and float (date variances in working days of the activity's calendar; positive is later),
with added and removed activities flagged, and the project finish variance.

## 4D links

An `ActivityLink` ties an activity to model elements by `ElementId` or by an `ElementRule`
(category, layer, mark pattern with `*` and `?`; all given fields must match). Rules resolve against
the document each time, so new elements are picked up. Actions:

| Action | before start | during | after finish |
|--------|--------------|--------|--------------|
| Construct | not started, absent | in progress (fraction), present | complete, present |
| Demolish | not started, present | in progress (fraction), present | removed, absent |
| Temporary | not started, absent | in progress (fraction), present | removed, absent |

`element_states(&programme, &schedule, &document, date)` gives the state at the **end** of `date`:
not started when the date is before the early start, complete (or removed) on or after the early
finish, in progress between with fraction `working days from start to date inclusive / working
days of the activity`. A one-day activity goes straight from not started to complete. An element
with several links shows the state of the latest-starting link that has begun. Elements that no link
reaches are returned in `unlinked`, and `link_report` also lists activities without links, links to
missing element ids and rules that match nothing.

## Outputs

* `gantt_drawing` returns an `afrobim_draw::Drawing` (lower-left on the origin) and a separate
  list of labels with positions, as `afrobim-corridor` does for sections. Bars blue, critical red,
  complete grey, progress as a black strip, milestones as diamonds, dependency lines with arrowheads
  (red between two critical activities), baseline as a thin grey bar, the data date green and the
  required finish brown, a month and week scale on top. `GanttDrawing::x_of(date)` maps a date to
  x.
* `activity_table_csv`, `variance_csv`, `s_curve_csv`.
* `s_curve` is planned progress by early dates: each activity earns its weight evenly over its
  working days. `SCurveBasis::Duration` weighs by working days; `SCurveBasis::Weight` by the weight
  entered on each activity (every activity must have one). `s_curve_drawing` plots it.

## MS Project XML (MSPDI)

`export_mspdi(&programme, Some(&schedule))` and `import_mspdi(xml)`. Carried: tasks (name, WBS,
duration, milestone flag, calendar, constraint of type 2, 4 or 7, percent complete, actual dates,
remaining duration), predecessor links with type (FF 0, FS 1, SF 2, SS 3) and lag, calendars'
working weekdays and non-working exception dates, the status date as data date, and the activity
id in the `Text1` custom field. A working day is written as 480 minutes (08:00-12:00, 13:00-17:00)
because MSPDI measures durations in time; on import the file's own `MinutesPerDay` is used and a
file without it is refused. Skipped with a warning in the `ImportReport`: summary and null tasks
(links to them dropped), other constraint types, elapsed or percentage lags, working exceptions,
work weeks, recurring exceptions beyond their date range, cross-project links. Milestone kind,
weights, links to model elements, baseline and required finish are not written; imported milestones
are finish milestones.

Element names and enumerations were checked against Microsoft's MSPDI reference on Microsoft Learn;
the report that accompanied this crate lists which were verified and which could not be.

## Persistence

The whole programme serialises with serde (`Programme::to_value` / `from_value`). The app stores it
as one element of category `schedule` (`SCHEDULE_CATEGORY`) in the document: `add_programme`,
`store_programme`, `load_programme`. `ScheduleModule` implements `afrobim_kernel::Module` for that
category: `generate` runs the schedule, draws the Gantt chart, and returns issues as notes and the
finish, activity and critical counts as derived values. The module cannot return labels
(`Generated` has none), so a front end that wants the text calls `gantt_drawing`.

## Not done

* Resources, productivities and levelling.
* Costs: the cost crate is separate. The S-curve weight is a plain number.
* Primavera P6 XER import and export: next.
* **Time-location (linear) charts**: for road work, activities by chainage against time are the
  natural next view, and the 4D links could carry a chainage range per activity.
* Longest-path criticality, calendars per relationship, multiple baselines, activity splitting,
  expected-finish dates, *finish no earlier than* and *start no later than* constraints, summary
  tasks and WBS roll-up.
* Free float across calendars is counted in the successor's calendar, a simplification.
