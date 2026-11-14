# afrobim-schedule

The 4D stage of AfroBIM: a construction programme (critical path method) linked to model
elements. It computes dates and float over working-day calendars, tracks progress against a
baseline, says what every linked model element looks like on any date, draws a Gantt chart and an
S-curve, a time-location chart for linear works, and exchanges programmes with Microsoft Project
(MSPDI XML) and Primavera P6 (XER).

Nothing is assumed. There are no default durations, productivities or holidays: the working
weekdays and every day off of a calendar are entered by the user.

## Dependencies

`afrobim-draw`, `afrobim-kernel`, `serde`, `serde_json`, `thiserror`, and `roxmltree` 0.20. XER is read and written by hand (no dependency). There
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
* **Critical** means, by default, total float of zero or less and not complete (`CriticalBasis::TotalFloat`).
  With a required finish later than the computed finish nothing is critical by that test.
* **Longest path** is the alternative basis, `schedule_with(&p, &ScheduleOptions { critical: CriticalBasis::LongestPath })`.
  `Row::on_longest_path` is always filled in; the option decides whether `Row::critical` follows it.
  Definition used here: start from every unfinished activity whose early finish is the project finish. An
  unfinished predecessor joins the path when its relationship is *driving*, that is when the date that
  relationship alone requires of the successor equals the successor's scheduled (remaining) start;
  repeat from each activity that joined. A constraint, the data date or the programme start that holds an
  activity later than its logic ends the chain there; spent SS links of work in progress never drive. This
  is this crate's definition and it has not been compared with what P6 or MS Project mark as the longest
  path (P6 exports its own flag in `TASK.driving_path_flag`, which this crate does not read).

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
* `activity_table_csv`, `variance_csv`, `s_curve_csv`, `wbs_rollup_csv`, `time_location_csv`.
* `s_curve` is planned progress by early dates: each activity earns its weight evenly over its
  working days. `SCurveBasis::Duration` weighs by working days; `SCurveBasis::Weight` by the weight
  entered on each activity (every activity must have one). `s_curve_drawing` plots it.

## Several constraints per activity

`Activity::constraint` stays the primary constraint (its type is unchanged, so existing code compiles).
`Activity::extra_constraints` is a list of further ones; `Activity::constraints()` yields all of them.
Forward pass: every *start no earlier than* counts (the latest wins), then the first *must start on*
overrides the logic. Backward pass: every *finish no later than* caps the late finish (the earliest
wins). Several different *must start on* dates give a `conflicting_constraints` warning and the first is
used. The activity table CSV lists all constraints, separated by `; `.

Only the three kinds of `Constraint` exist. *Finish no earlier than*, *start no later than*, *as late as
possible* and the mandatory finish are **not** modelled: adding a variant to `Constraint` would break the
exhaustive `match` in `afrobim-app`'s `schedule_ui.rs`, which this crate must not do. MSPDI holds one
constraint per task and XER two, so those exports write the first one or two and list the rest in the
export report.

## Work breakdown structure (summary tasks)

A programme can carry `Programme::summaries` (`SummaryTask { id, wbs, name, parent }`), and an activity
names its summary in `Activity::parent`. Summary tasks are not in the scheduling network, so they have no
logic, duration or float of their own; `schedule` is unchanged and a flat programme works as before
(no summaries, no parents).

`wbs_rollup(&programme, &schedule, RollupBasis)` returns a `SummaryRow` per summary task in outline order
(`outline(&programme)` gives that order: each summary, its sub-summaries, then its own activities;
activities without a parent last):

* `start` and `finish`: the earliest early start and the latest early finish of every activity below, at
  any depth.
* `percent_complete`, rule stated by the basis: `RollupBasis::Duration` is
  `sum(duration * percent) / sum(duration)` over the activities below, where the weight is the entered
  duration in working days and a milestone weighs nothing; `RollupBasis::Weight` is the same formula
  with the weight entered on each activity (`Activity::weight`), and fails with a list of the activities
  that lack one. When the weights add up to zero (only milestones) the percent is the share of activities
  complete. Complete work counts as 100.
* `activities`, `critical` (how many of them are critical) and `total_float` (the smallest float of the
  unfinished ones).

`validate` reports `duplicate_summary`, `unknown_parent` and `wbs_cycle` as errors. `wbs_rollup_csv`
gives the roll-up as CSV. `gantt_drawing_wbs` draws summary rows (black bars with end brackets, the
rolled-up percent as a grey strip, names indented by level); `GanttDrawing::rows` holds `wbs:<id>` for a
summary row. `gantt_drawing` itself is unchanged and draws activities only.

## Time-location (time-chainage) chart

For linear works. An activity can carry `Activity::location` (`LocationRange { alignment, from_m, to_m,
direction, dwell_days }`, stations in metres, either order). `resolve_locations(&programme, Some(&document))`
returns the range of every activity that has one, entered or derived. Derived: for an activity without a
range, the linked elements (by id or rule, as in 4D) that have `Element::site` with an alignment name and
a `station_m`; if they all lie on one alignment the range is their lowest to highest station and the
direction is `Fixed` (the placements say where the elements are, not the order they are built); if they
lie on several alignments, or none has a station, nothing is derived and `ResolvedLocations::notes` says
why. Elements without a station are left out and noted.

`time_location_chart(&programme, &schedule, &resolved, &options)` plots one alignment (the one in the
options, else the first). Station is across the page and time up it (`time_vertical: false` swaps them).
Dates are whole days: an activity starts at the **start** of its early start day and finishes at the
**end** of its early finish day.

| Direction | Drawn as |
|-----------|----------|
| `Increasing` | line from (low station, start) to (high station, finish); a parallelogram when `dwell_days` is set |
| `Decreasing` | line from (high station, start) to (low station, finish); likewise |
| `Fixed`, or a range of one station | bar from low to high station, from start to finish (a culvert or structure) |
| milestone | a diamond at its date |

A parallelogram of dwell `w` days has the corners (a, start), (b, finish - w), (b, finish), (a, start + w),
with a and b the stations the front starts and ends at. `dwell_days` is entered by the user, never
assumed. Critical activities are red, complete grey, the rest blue; the baseline is a thin grey outline
of the same shape; the data date is a green line and the required finish a brown one. Labels (title,
station scale, month and week scale, activity ids, data date, required finish) come back separately in
`TimeLocationChart::labels`, as with the Gantt chart, since the draw crate has no text. `chart.pos(station_m, days)`
and `chart.pos_at(station_m, date, end_of_day)` give drawing positions (position = margin + value * scale,
so `x = margin + (station - station_min) * metre_scale` and `y = margin + days * day_scale` when time is
vertical). `time_location_csv` gives the plotted data: id, name, alignment, source (entered or derived),
kind, direction, critical, status, percent, start station and date, end station and finish date, dwell
and baseline dates.

## 4D playback

`element_timelines(&programme, &schedule, &document, from, to)` returns, in one pass, a timeline per linked
element: its state at the end of `from` and the later changes (a date, the phase and whether the element is
present, each holding until the next entry). `Playback::change_dates` lists the dates in `(from, to]` on
which anything changes, so an animation redraws only then; `ElementTimeline::at(date)` and
`Playback::timeline(id)` look up the state on any date without touching the schedule again. The result
agrees date by date with `element_states` (tested over a six-week range, for all four kinds of link
including an element with two links). An element in progress changes on every working day of its
activity (the fraction moves); the cost is one evaluation per element per working day of its
activities, not per calendar day of the range. `element_state_at(&programme, &schedule, &element, date)`
is the single-element function both use.

## MS Project XML (MSPDI)

`export_mspdi(&programme, Some(&schedule))` (or `export_mspdi_with_report`, which also returns what was not
carried) and `import_mspdi(xml)`. Carried: tasks (name, WBS, duration, milestone flag, calendar, constraint of
type 2, 4 or 7, percent complete, actual dates, remaining duration), summary tasks (`Summary` 1, with
`OutlineLevel` and `OutlineNumber`; on import the parent comes from the outline level), predecessor links
with type (FF 0, FS 1, SF 2, SS 3) and lag, calendars' working weekdays and non-working exception dates,
the status date as data date, and the activity id (and the summary id) in the `Text1` custom field. A
working day is written as 480 minutes (08:00-12:00, 13:00-17:00) because MSPDI measures durations in
time; on import the file's own `MinutesPerDay` is used and a file without it is refused. Skipped with a
warning in the `ImportReport`: null tasks, the project summary task (UID 0, silently), links to summary or
null tasks, other constraint types, elapsed or percentage lags, working exceptions, work weeks, recurring
exceptions beyond their date range, cross-project links. Milestone kind, weights, links to model
elements, baseline, required finish, location ranges and extra constraints are not written; imported
milestones are finish milestones. Within a summary the export lists sub-summaries before activities, so
a file that interleaved them comes back reordered.

Element names and enumerations were checked against Microsoft's MSPDI reference on Microsoft Learn.
No MSPDI file written by this crate has been opened in MS Project.

## Primavera P6 XER

`export_xer(&programme, Some(&schedule))` (or `export_xer_with_report`, returning an `ExportReport`) and
`import_xer(text)` / `import_xer_bytes(bytes)` (UTF-8, else Windows-1252) returning `(Programme, ImportReport)`.

**Format as used here.** Line 1 is `ERMHDR` followed by tab-separated header fields (version, date, export
type, user, ...). Then for each table a `%T<TAB>name` line, a `%F` line listing the field names, one `%R`
line per row, and a final `%E` line. Fields are tab-separated and not quoted. Dates are
`YYYY-MM-DD HH:MM` (some files carry seconds; only the date and, for the data date, the hour are read).
Durations and lags are in hours.

**Tables written** (in P6's own order): PROJECT, CALENDAR, SCHEDOPTIONS, PROJWBS, TASK, TASKPRED.
Imported: the same, plus ERMHDR; every other table (resources, codes, costs, user fields) is ignored.

| Table.field | Meaning here |
|-------------|--------------|
| PROJECT `proj_short_name`, `clndr_id`, `plan_start_date`, `plan_end_date`, `last_recalc_date`, `scd_end_date`, `export_flag`, `name_sep_char`, `critical_path_type` | programme name (written; the name read is the root WBS node's), default calendar, programme start, required finish ("Must Finish By"), data date, schedule finish, which project to read, WBS code separator, a warning when not `CT_TotFloat` |
| CALENDAR `clndr_id`, `clndr_name`, `clndr_type`, `base_clndr_id`, `day_hr_cnt`, `clndr_data`, `default_flag` | calendars; `day_hr_cnt` converts hours to days |
| PROJWBS `wbs_id`, `parent_wbs_id`, `proj_node_flag`, `wbs_short_name`, `wbs_name`, `seq_num` | the root node (flag `Y`) is the project; the others become summary tasks, `wbs` = short names joined from the top down with `.` |
| TASK `task_id`, `wbs_id`, `clndr_id`, `task_code` (activity id), `task_name`, `task_type`, `status_code`, `complete_pct_type`, `phys_complete_pct`, `target_drtn_hr_cnt`, `remain_drtn_hr_cnt`, `act_start_date`, `act_end_date`, `cstr_type`, `cstr_date`, `cstr_type2`, `cstr_date2`, `early_*`, `late_*`, `target_*`, `total_float_hr_cnt`, `free_float_hr_cnt`, `driving_path_flag` | activities; the early, late, float and driving fields are written from the schedule and ignored on import (the programme is rescheduled) |
| TASKPRED `task_id` (successor), `pred_task_id`, `pred_type`, `lag_hr_cnt`, `proj_id`, `pred_proj_id` | relationships |
| SCHEDOPTIONS `sched_retained_logic`, `sched_progress_override`, `sched_calendar_on_relationship_lag`, `sched_float_type`, `sched_use_project_end_date_for_float` | read for warnings; written with the values the engine implies |

**Codes.** `task_type`: `TT_Task` task; `TT_Mile` start milestone; `TT_FinMile` finish milestone; `TT_Rsrc` read as task with
a warning; `TT_LOE` and `TT_WBS` skipped with a warning (and their relationships dropped). `status_code`:
`TK_NotStart`, `TK_Active`, `TK_Complete`. `complete_pct_type`: `CP_Phys` (percent from `phys_complete_pct`;
written by the exporter), `CP_Drtn` (percent = 1 - remaining/target), `CP_Units` read as duration with a
warning. `pred_type`: `PR_FS`, `PR_SS`, `PR_FF`, `PR_SF`. Constraints (`cstr_type`, primary, and `cstr_type2`,
secondary): `CS_MSOA` is start no earlier than, `CS_MEOB` finish no later than, `CS_MANDSTART` must start on;
`CS_MSO` (start on) is read as start no earlier than with a warning; `CS_MSOB`, `CS_MEO`, `CS_MEOA`,
`CS_MANDFIN`, `CS_ALAP` are skipped with a warning. The primary maps to `Activity::constraint`, the
secondary to `extra_constraints`.

**Hours and days.** Hours are divided by the `day_hr_cnt` of the calendar concerned (an activity's own
calendar for durations, the successor's for lag); a fraction is rounded to a whole day with a warning, and a
calendar whose working days differ in length is warned about. The exporter writes calendars of 8 hours
(08:00-12:00 and 13:00-17:00), a file convention (`HOURS_PER_DAY`), not a planning value; a file's own
`day_hr_cnt` is always used on import. **Data date.** `last_recalc_date` is read as the data date when any
activity has progress (that is how `xerparser` and the sample files use it). This crate takes the data date as the
*end* of a day, so a time before 12:00 means nothing was worked that day and the previous day is used,
with a warning. The exporter writes the data date at 17:00.

**Calendars.** `clndr_data` is a nested text such as
`(0||CalendarData()((0||DaysOfWeek()((0||1()())(0||2()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))...(0||7()())))(0||Exceptions()((0||0(d|43650)())...))))`.
Read: the seven day nodes (1 is Sunday; a day with shifts is a working weekday), and the `Exceptions`
nodes with a date `d|n` and no shifts, which are non-working days (`n` counts days from 1899-12-30, so 43650
is 4 July 2019). Written: the same grammar; the test reproduces the data string of a P6 export byte for
byte from its weekdays and holidays. Not supported, with a warning: exceptions that make a normally
non-working day *working* (read only from `xerparser`'s expression, never seen in a file), shifts other than
the day length, resource calendars read as ordinary calendars, and calendars that cannot be parsed
(skipped; their activities are skipped). A project calendar that lists no weekdays and names a base
calendar is given the base calendar's weekdays and holidays (assumed to be what P6 means; unverified).
No calendar-based `rcal_*` lag option other than `rcal_Successor` is implemented; others give a warning.

**Not carried by the exporter** (listed in `ExportReport::warnings`): the baseline, links to model elements,
activity weights, location ranges, a third and later constraint, and the free-text `wbs` code of activities
that have no summary task (XER holds the WBS only as nodes, so give the programme summary tasks). Importer
limits: one project per call (the first with `export_flag` `Y`; others are named in a warning), no resources,
costs, activity codes, user fields, expected finish dates, steps, activity splitting, relationships to other
projects, or P6's schedule options other than the warnings above.

### Where each name was verified

Neither the exporter nor the importer has been run against Primavera P6 itself, and no XER file written here
has been opened in P6.

* **Oracle, "XER Import/Export Data Map Guide (Project)"**, docs.oracle.com, `cd/F74773_01/English/Mapping_and_Schema/xer_import_export_data_map_project/`,
  pages `97885.htm` (PROJECT), `97869.htm` (CALENDAR), `97890.htm` (PROJWBS), `97906.htm` (TASK),
  `97914.htm` (TASKPRED), `97903.htm` (SCHEDOPTIONS), published 3/28/2023. These list every column name used above
  with its P6 label (for example `plan_end_date` is "Must Finish By", `day_hr_cnt` "Work Hours Per Day",
  `driving_path_flag` "Longest Path", `cstr_type2` "Secondary Constraint"). They do not list the allowed
  codes or describe the file syntax.
* **`xerparser` 0.13.9** (PyPI, source on GitHub as jjCode01/xerparser), read in full for `PROJECT`, `PROJWBS`, `TASK`,
  `TASKPRED`, `CALENDAR` and `SCHEDOPTIONS`: the `ERMHDR` field positions, the date format
  `%Y-%m-%d %H:%M`, the code lists (`TT_*`, `TK_*`, `CP_*`, `CS_*`, `CA_*`, `PR_*`), the
  calendar-data expressions, the 1899-12-30 day numbering and its use of `last_recalc_date` as the data date.
* **`PyP6XER` 2.0** (PyPI), read for its code constants (`TT_FinMile`, `TT_LOE`, `TT_WBS`, `PR_FF`, `PR_SF`,
  `DT_*`), its writer (table order, `%E` as the last line, tab separation without quoting) and its calendar parser.
* **MPXJ `ConstraintTypeHelper.java`** (the `XER_TYPE_MAP`): `CS_MSO`, `CS_MSOB`, `CS_MSOA`, `CS_MEO`,
  `CS_MEOB`, `CS_MEOA`, `CS_ALAP`, `CS_MANDSTART`, `CS_MANDFIN` with their meanings; agrees with `xerparser`.
* **Published sample files**, opened and imported by this crate's tests when the directory is supplied
  (`AFROBIM_XER_SAMPLES`; not stored in the repository): MPXJ's `SuccessorCalendar.xer`,
  `PredecessorCalendar.xer`, `ProjectDefaultCalendar.xer`, `TwentyFourHourCalendar.xer` (header says P6 8.2.0,
  2020; full table and field lists, a project calendar with 14 holidays, `CS_MSOA`, `TK_Complete`,
  `last_recalc_date`, `rcal_Successor`, `rcal_Predecessor`, `rcal_ProjDefault`, `FT_FF`, `CT_TotFloat`,
  `DT_FixedDrtn`), Aspose's `testXer.xer` (83 activities, `TT_Mile`, `PR_FS`, `PR_SS`, dates with seconds,
  empty `proj_id` and `clndr_id` columns, a project calendar with no weekdays and a base calendar) and a synthetic
  file. All of them import and schedule. Their calendar date numbers decode to dates that are real public
  holidays on the right weekdays (43650 is Thursday 4 July 2019, 43710 is Monday 2 September 2019).
* Seen in a sample file: tables and fields above, `TT_Task`, `TT_Mile`, `TK_NotStart`, `TK_Active`, `TK_Complete`,
  `CP_Drtn`, `PR_FS`, `PR_SS`, `CS_MSOA`, `CA_Base`, `CA_Project`, `DT_FixedDrtn`. Verified only in the Python
  sources, not in a file I opened: `TT_FinMile`, `TT_LOE`, `TT_WBS`, `CP_Phys`, `CP_Units`, `PR_FF`, `PR_SF`, the
  other `CS_*` codes, the grammar of working exceptions.
* **Third-party parse of exported files.** The test `xer_file_parses_with_xerparser` (skipped when python3
  or the package is missing; point `AFROBIM_XER_PYTHON` at an interpreter that has `pip install xerparser`) exports
  a programme with summary tasks, progress, two calendars and every relationship type, reads it with `xerparser`
  0.13.9, and compares every activity's name, type, status, hours, start, finish, WBS code and calendar, both
  constraints, every relationship with its lag, the WBS codes and the holidays. It passed. The same file was also read
  by `PyP6XER` 2.0 (activity, relationship, WBS and calendar counts and the activity fields, checked by hand, not by a test).
  This shows the file is well formed for those two readers; it does not show P6 accepts it.

## Persistence

The whole programme serialises with serde (`Programme::to_value` / `from_value`); old files without the new
fields (`parent`, `extra_constraints`, `location`, `summaries`) still load. The app stores it
as one element of category `schedule` (`SCHEDULE_CATEGORY`) in the document: `add_programme`,
`store_programme`, `load_programme`. `ScheduleModule` implements `afrobim_kernel::Module` for that
category: `generate` runs the schedule, draws the Gantt chart, and returns issues as notes and the
finish, activity and critical counts as derived values. The module cannot return labels
(`Generated` has none), so a front end that wants the text calls `gantt_drawing`.

## API examples

```rust
use afrobim_schedule::*;

// Import a P6 file, schedule it, and read the warnings.
let (programme, report) = import_xer(&std::fs::read_to_string("job.xer")?)?;   // or import_xer_bytes(&bytes)
for w in &report.warnings { eprintln!("import: {w}"); }
let schedule = schedule(&programme)?;

// Export both formats; the report lists what the file does not carry.
let (xer_text, export_report) = export_xer_with_report(&programme, Some(&schedule));
let (mspdi_text, _) = export_mspdi_with_report(&programme, Some(&schedule));

// Critical by the longest path instead of total float.
let s = schedule_with(&programme, &ScheduleOptions { critical: CriticalBasis::LongestPath })?;

// WBS roll-up and the Gantt chart with summary rows.
let rollup = wbs_rollup(&programme, &s, RollupBasis::Duration)?;       // or RollupBasis::Weight
let gantt = gantt_drawing_wbs(&programme, &s, &GanttOptions::default(), &rollup);
let csv = wbs_rollup_csv(&rollup);

// Time-location chart. Derive missing ranges from the placed elements of the document.
let resolved = resolve_locations(&programme, Some(&document));
let options = TimeLocationOptions { alignment: Some("MAIN".into()), ..Default::default() };
if let Some(chart) = time_location_chart(&programme, &s, &resolved, &options) {
    // chart.drawing: primitives; chart.labels: text with kinds; chart.plotted: what was drawn.
}
let plotted_csv = time_location_csv(&programme, &s, &resolved, &options);

// 4D animation: one pass, then look states up per frame.
let pb = element_timelines(&programme, &s, &document, start_date, end_date);
for date in &pb.change_dates { /* redraw on these dates only */ }
if let Some(state) = pb.timeline(element_id).and_then(|t| t.at(some_date)) {
    // state.phase: Phase::{NotStarted, InProgress { fraction }, Complete, Removed}; state.present: bool
}
```

## Not done

* Resources, productivities and levelling.
* Costs: the cost crate is separate. The S-curve weight is a plain number.
* The constraint kinds *finish no earlier than*, *start no later than*, *as late as possible*, mandatory
  finish (see above); calendars per relationship, multiple baselines, activity splitting, expected-finish
  dates.
* XER: resources, costs, activity codes, user fields, several projects per file, working exceptions in
  calendars, calendars per relationship, P6's progress-override scheduling.
* Interleaved order of summaries and activities inside a summary survives no exchange format here.
* The time-location chart plots one alignment per call and has no text primitives (labels are returned).
* Free float across calendars is counted in the successor's calendar, a simplification.
