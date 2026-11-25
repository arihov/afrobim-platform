// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The work breakdown structure: summary tasks whose dates span their children and whose percent
//! complete rolls up by a stated rule.
//!
//! The tree is stored as parent pointers ([`crate::SummaryTask::parent`],
//! [`crate::Activity::parent`]); the scheduling network never sees summary tasks, so a summary has
//! no logic and no float of its own. Its rows are computed here from a [`Schedule`].

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::cpm::{Schedule, ScheduleError};
use crate::date::Date;
use crate::model::Programme;

/// How percent complete rolls up through summary tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollupBasis {
    /// Each activity weighs its entered duration in working days (milestones weigh nothing).
    /// `percent = sum(duration * percent) / sum(duration)` over every activity below the summary.
    /// When those durations add up to zero (only milestones) the percent is the share of activities
    /// that are complete.
    Duration,
    /// Each activity weighs the weight entered on it ([`crate::Activity::weight`]); every activity
    /// below the summary must have one. Same formula as `Duration`.
    Weight,
}

/// One position in the outline: a summary task or an activity, with its depth (1 at the top).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineKind {
    /// Index into `Programme::summaries`.
    Summary(usize),
    /// Index into `Programme::activities`.
    Activity(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutlineItem {
    pub kind: OutlineKind,
    pub level: usize,
}

/// The programme in outline order: each summary task, then its sub-summaries (in programme order),
/// then its own activities (in programme order). Activities without a parent come after every
/// summary subtree. A programme without summaries gives its activities in programme order at level
/// 1. A parent that does not exist is treated as no parent (`validate` reports it as an error).
pub fn outline(p: &Programme) -> Vec<OutlineItem> {
    let known: HashSet<&str> = p.summaries.iter().map(|s| s.id.as_str()).collect();
    let mut kids_s: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut kids_a: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    let mut orphans = Vec::new();
    for (i, s) in p.summaries.iter().enumerate() {
        match s.parent.as_deref().filter(|q| known.contains(q) && *q != s.id) {
            Some(q) => kids_s.entry(q).or_default().push(i),
            None => roots.push(i),
        }
    }
    for (i, a) in p.activities.iter().enumerate() {
        match a.parent.as_deref().filter(|q| known.contains(q)) {
            Some(q) => kids_a.entry(q).or_default().push(i),
            None => orphans.push(i),
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    fn walk<'a>(
        p: &'a Programme,
        i: usize,
        level: usize,
        kids_s: &HashMap<&'a str, Vec<usize>>,
        kids_a: &HashMap<&'a str, Vec<usize>>,
        seen: &mut HashSet<usize>,
        out: &mut Vec<OutlineItem>,
    ) {
        if !seen.insert(i) {
            return;
        }
        out.push(OutlineItem { kind: OutlineKind::Summary(i), level });
        let id = p.summaries[i].id.as_str();
        for &c in kids_s.get(id).into_iter().flatten() {
            walk(p, c, level + 1, kids_s, kids_a, seen, out);
        }
        for &a in kids_a.get(id).into_iter().flatten() {
            out.push(OutlineItem { kind: OutlineKind::Activity(a), level: level + 1 });
        }
    }
    for r in roots {
        walk(p, r, 1, &kids_s, &kids_a, &mut seen, &mut out);
    }
    for a in orphans {
        out.push(OutlineItem { kind: OutlineKind::Activity(a), level: 1 });
    }
    out
}

/// One summary task as rolled up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryRow {
    pub id: String,
    pub wbs: String,
    pub name: String,
    pub parent: Option<String>,
    /// 1 at the top.
    pub level: usize,
    /// Earliest early start of the activities below; `None` when there are none.
    pub start: Option<Date>,
    /// Latest early finish of the activities below.
    pub finish: Option<Date>,
    /// 0 to 100, by the [`RollupBasis`].
    pub percent_complete: f64,
    /// Activities below, at any depth.
    pub activities: usize,
    /// Of those, how many are critical.
    pub critical: usize,
    /// The smallest total float of the unfinished activities below (working days of each activity's
    /// own calendar), `None` when all are complete or there are none.
    pub total_float: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rollup {
    pub basis: RollupBasis,
    /// In outline order (parents before children).
    pub rows: Vec<SummaryRow>,
}

impl Rollup {
    pub fn row(&self, id: &str) -> Option<&SummaryRow> {
        self.rows.iter().find(|r| r.id == id)
    }
}

#[derive(Default)]
struct Acc {
    start: Option<Date>,
    finish: Option<Date>,
    weighted: f64,
    weight: f64,
    complete: usize,
    count: usize,
    critical: usize,
    float: Option<i64>,
}

/// Rolls the schedule up through the summary tasks. Fails with `ScheduleError::Other` when
/// `RollupBasis::Weight` is asked for and an activity below a summary has no weight.
pub fn wbs_rollup(p: &Programme, s: &Schedule, basis: RollupBasis) -> Result<Rollup, ScheduleError> {
    let idx: HashMap<&str, usize> = p.summaries.iter().enumerate().map(|(i, x)| (x.id.as_str(), i)).collect();
    let mut acc: Vec<Acc> = p.summaries.iter().map(|_| Acc::default()).collect();
    let mut missing: Vec<String> = Vec::new();
    for r in &s.rows {
        let Some(a) = p.activity(&r.id) else { continue };
        let w = match basis {
            RollupBasis::Duration => a.duration as f64,
            RollupBasis::Weight => match a.weight {
                Some(w) => w,
                None => {
                    if a.parent.is_some() {
                        missing.push(a.id.clone());
                    }
                    0.0
                }
            },
        };
        // Walk up the chain of parents (bounded, so a loop cannot hang).
        let mut cur = a.parent.as_deref();
        let mut guard = 0;
        while let Some(id) = cur {
            let Some(&k) = idx.get(id) else { break };
            let c = &mut acc[k];
            c.start = Some(c.start.map_or(r.early_start, |x| x.min(r.early_start)));
            c.finish = Some(c.finish.map_or(r.early_finish, |x| x.max(r.early_finish)));
            c.weighted += w * r.percent_complete;
            c.weight += w;
            c.count += 1;
            if r.status == crate::Status::Complete {
                c.complete += 1;
            } else {
                c.float = Some(c.float.map_or(r.total_float, |f| f.min(r.total_float)));
            }
            if r.critical {
                c.critical += 1;
            }
            cur = p.summaries[k].parent.as_deref();
            guard += 1;
            if guard > p.summaries.len() {
                break;
            }
        }
    }
    if !missing.is_empty() {
        return Err(ScheduleError::Other(format!("the roll-up by weight needs a weight on every activity under a summary task; missing on: {}", missing.join(", "))));
    }
    let mut rows = Vec::new();
    for it in outline(p) {
        if let OutlineKind::Summary(k) = it.kind {
            let sm = &p.summaries[k];
            let c = &acc[k];
            let percent = if c.count == 0 {
                0.0
            } else if c.weight > 0.0 {
                c.weighted / c.weight
            } else {
                100.0 * c.complete as f64 / c.count as f64
            };
            rows.push(SummaryRow {
                id: sm.id.clone(),
                wbs: sm.wbs.clone(),
                name: sm.name.clone(),
                parent: sm.parent.clone(),
                level: it.level,
                start: c.start,
                finish: c.finish,
                percent_complete: percent,
                activities: c.count,
                critical: c.critical,
                total_float: c.float,
            });
        }
    }
    Ok(Rollup { basis, rows })
}

pub const ROLLUP_CSV_HEADER: &str = "id,wbs,name,level,start,finish,percent_complete,activities,critical,total_float_wd";

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The roll-up as CSV, one row per summary task in outline order. Percent has two decimals.
pub fn wbs_rollup_csv(r: &Rollup) -> String {
    let mut out = String::from(ROLLUP_CSV_HEADER);
    out.push('\n');
    for x in &r.rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{:.2},{},{},{}\n",
            csv(&x.id),
            csv(&x.wbs),
            csv(&x.name),
            x.level,
            x.start.map(|d| d.to_string()).unwrap_or_default(),
            x.finish.map(|d| d.to_string()).unwrap_or_default(),
            x.percent_complete,
            x.activities,
            x.critical,
            x.total_float.map(|f| f.to_string()).unwrap_or_default()
        ));
    }
    out
}
