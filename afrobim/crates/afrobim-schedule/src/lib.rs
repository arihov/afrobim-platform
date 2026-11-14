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
mod module;
mod mspdi;
mod output;

pub use baseline::{Baseline, BaselineEntry, Variance, VarianceReport, VarianceStatus};
pub use calendar::Calendar;
pub use cpm::{schedule, validate, Issue, Row, Schedule, ScheduleError};
pub use date::{Date, Weekday};
pub use links::{element_states, link_report, ElementState, ElementStates, LinkReport, LinkState, Phase};
pub use model::{
    Activity, ActivityLink, Constraint, Dependency, ElementRule, ElementSelector, LinkAction, LinkType, MilestoneKind, Programme, Progress, Status,
};
pub use module::{add_programme, load_programme, store_programme, ScheduleModule, SCHEDULE_CATEGORY};
pub use mspdi::{export_mspdi, import_mspdi, ImportReport, MspdiError, MINUTES_PER_DAY};
pub use output::{
    activity_table_csv, gantt_drawing, s_curve, s_curve_csv, s_curve_drawing, variance_csv, GanttDrawing, GanttLabel, GanttLabelKind, GanttOptions,
    SCurve, SCurveBasis, SCurveLabel, SPoint, ACTIVITY_CSV_HEADER, VARIANCE_CSV_HEADER,
};

#[cfg(test)]
mod tests;
