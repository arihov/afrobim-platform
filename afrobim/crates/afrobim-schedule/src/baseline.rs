// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Baseline: a stored copy of a schedule, and variance of the current schedule against it.

use serde::{Deserialize, Serialize};

use crate::cpm::{Schedule, ScheduleError};
use crate::date::Date;
use crate::model::Programme;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub id: String,
    pub start: Date,
    pub finish: Date,
    /// Working days ([`crate::Row::duration`]).
    pub duration: u32,
    pub total_float: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub name: String,
    pub entries: Vec<BaselineEntry>,
    pub project_finish: Date,
}

impl Baseline {
    pub fn from_schedule(name: &str, s: &Schedule) -> Baseline {
        Baseline {
            name: name.to_string(),
            entries: s
                .rows
                .iter()
                .map(|r| BaselineEntry { id: r.id.clone(), start: r.early_start, finish: r.early_finish, duration: r.duration, total_float: r.total_float })
                .collect(),
            project_finish: s.project_finish,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarianceStatus {
    Unchanged,
    Changed,
    /// In the schedule but not in the baseline.
    Added,
    /// In the baseline but no longer in the schedule.
    Removed,
}

/// Current minus baseline, per activity. Date variances are working days of the activity's
/// calendar (positive means later); `None` for added and removed activities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variance {
    pub id: String,
    pub name: String,
    pub status: VarianceStatus,
    pub start_variance: Option<i64>,
    pub finish_variance: Option<i64>,
    pub duration_variance: Option<i64>,
    pub float_variance: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VarianceReport {
    pub baseline: String,
    pub rows: Vec<Variance>,
    /// Working days of the programme calendar from the baseline project finish to the current one.
    pub project_finish_variance: i64,
}

impl Programme {
    /// Stores the schedule as the baseline, replacing any earlier one.
    pub fn set_baseline(&mut self, name: &str, s: &Schedule) {
        self.baseline = Some(Baseline::from_schedule(name, s));
    }

    /// Variance of `current` against the stored baseline.
    pub fn variance(&self, current: &Schedule) -> Result<VarianceReport, ScheduleError> {
        let b = self.baseline.as_ref().ok_or_else(|| ScheduleError::Other("the programme has no baseline".into()))?;
        let pcal = self.calendar_named(&self.calendar).ok_or_else(|| ScheduleError::Other("the programme calendar is not defined".into()))?;
        let mut rows = Vec::new();
        for r in &current.rows {
            let Some(e) = b.entries.iter().find(|e| e.id == r.id) else {
                rows.push(Variance { id: r.id.clone(), name: r.name.clone(), status: VarianceStatus::Added, start_variance: None, finish_variance: None, duration_variance: None, float_variance: None });
                continue;
            };
            let cal = self.calendar_named(&r.calendar).unwrap_or(pcal);
            let v = Variance {
                id: r.id.clone(),
                name: r.name.clone(),
                status: VarianceStatus::Unchanged,
                start_variance: Some(cal.wd_between(e.start, r.early_start)),
                finish_variance: Some(cal.wd_between(e.finish, r.early_finish)),
                duration_variance: Some(r.duration as i64 - e.duration as i64),
                float_variance: Some(r.total_float - e.total_float),
            };
            let changed = [v.start_variance, v.finish_variance, v.duration_variance, v.float_variance].iter().any(|x| *x != Some(0));
            rows.push(Variance { status: if changed { VarianceStatus::Changed } else { VarianceStatus::Unchanged }, ..v });
        }
        for e in &b.entries {
            if current.row(&e.id).is_none() {
                rows.push(Variance { id: e.id.clone(), name: String::new(), status: VarianceStatus::Removed, start_variance: None, finish_variance: None, duration_variance: None, float_variance: None });
            }
        }
        Ok(VarianceReport { baseline: b.name.clone(), rows, project_finish_variance: pcal.wd_between(b.project_finish, current.project_finish) })
    }
}
