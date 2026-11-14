//! 4D BIM: a construction programme linked to model elements.
//!
//! A [`Programme`] holds activities, logic (four relationship types with lags), calendars, progress
//! and a baseline. [`schedule`] runs the critical path method over working-day calendars;
//! [`element_states`] says what each linked model element looks like on a date; [`gantt_drawing`],
//! [`activity_table_csv`] and [`s_curve`] report it; [`export_mspdi`] and [`import_mspdi`]
//! exchange it with Microsoft Project. The programme serialises with serde and lives in the
//! document as a `schedule` element ([`add_programme`], [`ScheduleModule`]).
//!
//! Nothing is assumed: no default durations, productivities or holidays. Every calendar day off is
//! entered by the user.

mod baseline;
mod calendar;
mod cpm;
mod date;
mod links;
mod model;
mod location;
mod module;
mod mspdi;
mod output;
mod playback;
mod wbs;
mod xer;

pub use baseline::{Baseline, BaselineEntry, Variance, VarianceReport, VarianceStatus};
pub use calendar::Calendar;
pub use cpm::{schedule, schedule_with, validate, CriticalBasis, Issue, Row, Schedule, ScheduleError, ScheduleOptions};
pub use date::{Date, Weekday};
pub use links::{element_state_at, element_states, link_report, ElementState, ElementStates, LinkReport, LinkState, Phase};
pub use location::{
    resolve_locations, time_location_chart, time_location_csv, PlotKind, PlottedActivity, ResolvedLocation, ResolvedLocations, TimeLocationChart, TimeLocationOptions, TlLabel, TlLabelKind,
    TIME_LOCATION_CSV_HEADER,
};
pub use model::{
    Activity, ActivityLink, Constraint, Dependency, ElementRule, ElementSelector, LinkAction, LinkType, LocationRange, MilestoneKind, Programme, Progress, Status, SummaryTask, WorkDirection,
};
pub use module::{add_programme, load_programme, store_programme, ScheduleModule, SCHEDULE_CATEGORY};
pub use mspdi::{export_mspdi, export_mspdi_with_report, import_mspdi, ImportReport, MspdiError, MINUTES_PER_DAY};
pub use output::{
    activity_table_csv, gantt_drawing, gantt_drawing_wbs, s_curve, s_curve_csv, s_curve_drawing, variance_csv, GanttDrawing, GanttLabel, GanttLabelKind, GanttOptions, SCurve, SCurveBasis,
    SCurveLabel, SPoint, ACTIVITY_CSV_HEADER, VARIANCE_CSV_HEADER,
};
pub use playback::{element_timelines, ElementTimeline, Playback, StateChange};
pub use wbs::{outline, wbs_rollup, wbs_rollup_csv, OutlineItem, OutlineKind, Rollup, RollupBasis, SummaryRow, ROLLUP_CSV_HEADER};
pub use xer::{decode_xer_bytes, export_xer, export_xer_with_report, import_xer, import_xer_bytes, ExportReport, XerError, HOURS_PER_DAY};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_t3;
