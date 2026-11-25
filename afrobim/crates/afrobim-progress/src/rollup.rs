// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Roll-up of element progress to activity progress, and the update a programme takes.
//!
//! The elements of an activity are the ones its [`afrobim_schedule::ActivityLink`]s reach, by id
//! or by rule, whatever the link action (progress is progress of the work: Construct, Demolish and
//! Temporary links all count). The programme element and progress elements themselves are never
//! picked up by a rule.
//!
//! Activity percent = sum(w_i * p_i) / sum(w_i) over the activity's elements, where `p_i` is the
//! element's percent on the date (0 when it has no record yet) and `w_i` its weight. The weights
//! come from one of three bases, chosen per activity and reported with the result.

use std::collections::{BTreeMap, BTreeSet};

use afrobim_cost::{Decimal, EstimateResult, Rounding};
use afrobim_kernel::{Document, ElementId};
use afrobim_schedule::{Date, ElementSelector, Programme, SCHEDULE_CATEGORY};
use serde::{Deserialize, Serialize};

use crate::error::{invalid, Result};
use crate::module::PROGRESS_CATEGORY;
use crate::record::ProgressLog;

/// Which weights an activity's roll-up used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeightBasis {
    /// Weights entered by the user, per element.
    UserWeights,
    /// The priced value of each element in the estimate ([`EstimateResult::element_cost`]).
    PricedValue,
    /// Every element weighs the same.
    Equal,
}

impl WeightBasis {
    pub fn label(self) -> &'static str {
        match self {
            WeightBasis::UserWeights => "user weights",
            WeightBasis::PricedValue => "priced value",
            WeightBasis::Equal => "equal weights",
        }
    }
}

/// Where the weights may come from. Per activity the first usable source is taken, in the order
/// user weights, priced value, equal; the result says which was used and why a better one was not.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeightSources<'a> {
    /// A weight (above zero) per element. Used for an activity only when it covers all of the
    /// activity's elements.
    pub user: Option<&'a BTreeMap<ElementId, Decimal>>,
    /// Priced value per element comes from here. Used for an activity only when every element has
    /// a value above zero (so no element silently weighs nothing).
    pub estimate: Option<&'a EstimateResult>,
}

/// An activity's elements and weights, fixed for a document, programme and weight sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityWeights {
    pub activity: String,
    pub elements: Vec<(ElementId, Decimal)>,
    pub basis: WeightBasis,
    /// Why a preferred source was not used, when one was offered and skipped.
    pub basis_note: Option<String>,
}

/// The resolved elements and weights of all linked activities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rollup {
    pub activities: Vec<ActivityWeights>,
    /// Activities no element reaches: the roll-up has no progress for them.
    pub unlinked_activities: Vec<String>,
    /// `(activity, element)` for links by id to an element not in the document.
    pub missing_elements: Vec<(String, ElementId)>,
    /// Elements reached by more than one activity (their progress counts in each).
    pub shared_elements: Vec<ElementId>,
}

/// A date inferred from records, with how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferredDate {
    pub date: Date,
    pub basis: String,
}

/// One activity's progress on a date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityProgress {
    pub activity: String,
    pub percent: Decimal,
    pub basis: WeightBasis,
    pub basis_note: Option<String>,
    pub elements_total: usize,
    /// Elements with at least one record on or before the date.
    pub elements_recorded: usize,
    /// Inferred: the earliest date an element of the activity went above 0%.
    pub actual_start: Option<InferredDate>,
    /// Inferred: only when the activity is at 100%, the latest date an element reached 100%.
    pub actual_finish: Option<InferredDate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollupResult {
    pub date: Date,
    pub activities: Vec<ActivityProgress>,
    pub unlinked_activities: Vec<String>,
}

impl RollupResult {
    pub fn get(&self, activity: &str) -> Option<&ActivityProgress> {
        self.activities.iter().find(|a| a.activity == activity)
    }

    /// The updates a programme takes ([`apply_updates`]).
    pub fn updates(&self) -> Vec<ActivityUpdate> {
        self.activities
            .iter()
            .map(|a| ActivityUpdate {
                activity: a.activity.clone(),
                percent_complete: a.percent,
                actual_start: a.actual_start.clone(),
                actual_finish: a.actual_finish.clone(),
                basis: a.basis,
            })
            .collect()
    }
}

/// What to write on one activity of a programme. The dates are inferred from the records, not
/// entered by anyone, and are marked so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityUpdate {
    pub activity: String,
    pub percent_complete: Decimal,
    pub actual_start: Option<InferredDate>,
    pub actual_finish: Option<InferredDate>,
    pub basis: WeightBasis,
}

/// What [`apply_updates`] did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyReport {
    pub data_date: Date,
    pub applied: Vec<String>,
    /// Updates naming an activity the programme does not have.
    pub unknown_activities: Vec<String>,
    /// Every actual date written, all inferred: `(activity, "actual start" or "actual finish", date)`.
    pub inferred_dates: Vec<(String, String, Date)>,
}

/// `n / d` rounded half away from zero (d above zero).
fn div_round(n: i128, d: i128) -> i128 {
    let q = n / d;
    let r = n % d;
    if r.abs() * 2 >= d {
        q + n.signum()
    } else {
        q
    }
}

/// Exact weighted mean of percentages: `sum(w*p) / sum(w)`, rounded once at 6 decimals.
fn weighted_mean(pairs: &[(Decimal, Decimal)]) -> Result<Decimal> {
    let mut num: i128 = 0;
    let mut den: i128 = 0;
    for (w, p) in pairs {
        num = w.units().checked_mul(p.units()).and_then(|x| num.checked_add(x)).ok_or_else(|| crate::error::ProgressError::Invalid("weights too large to roll up".into()))?;
        den += w.units();
    }
    if den <= 0 {
        return invalid("the weights of an activity add up to zero");
    }
    Ok(Decimal::from_units(div_round(num, den)))
}

impl Rollup {
    /// Resolves the links of the programme against the document and fixes the weights.
    pub fn new(programme: &Programme, doc: &Document, sources: &WeightSources) -> Result<Rollup> {
        let mut missing = Vec::new();
        let mut per_activity: Vec<(String, Vec<ElementId>)> = Vec::new();
        let mut unlinked = Vec::new();
        for a in &programme.activities {
            let mut ids: Vec<ElementId> = Vec::new();
            for l in programme.links.iter().filter(|l| l.activity == a.id) {
                match &l.selector {
                    ElementSelector::Id(id) => {
                        if doc.find(*id).is_some() {
                            ids.push(*id);
                        } else {
                            missing.push((a.id.clone(), *id));
                        }
                    }
                    ElementSelector::Rule(r) => {
                        for e in doc.elements() {
                            if e.category != SCHEDULE_CATEGORY && e.category != PROGRESS_CATEGORY && r.matches(e) {
                                ids.push(e.id);
                            }
                        }
                    }
                }
            }
            let set: BTreeSet<ElementId> = ids.into_iter().collect();
            if set.is_empty() {
                unlinked.push(a.id.clone());
            } else {
                per_activity.push((a.id.clone(), set.into_iter().collect()));
            }
        }
        let mut seen: BTreeMap<ElementId, usize> = BTreeMap::new();
        for (_, els) in &per_activity {
            for e in els {
                *seen.entry(*e).or_default() += 1;
            }
        }
        let shared_elements = seen.iter().filter(|(_, n)| **n > 1).map(|(e, _)| *e).collect();

        let mut activities = Vec::new();
        for (id, els) in per_activity {
            let mut note: Option<String> = None;
            let mut chosen: Option<(WeightBasis, Vec<Decimal>)> = None;
            if let Some(user) = sources.user {
                let missing_w: Vec<String> = els.iter().filter(|e| !user.contains_key(e)).map(|e| e.0.to_string()).collect();
                if !missing_w.is_empty() {
                    note = Some(format!("user weights missing for element(s) {}", missing_w.join(", ")));
                } else {
                    let ws: Vec<Decimal> = els.iter().map(|e| user[e]).collect();
                    if let Some(bad) = ws.iter().find(|w| **w <= Decimal::ZERO) {
                        return invalid(format!("activity {id}: a user weight is {bad}; weights must be above zero"));
                    }
                    chosen = Some((WeightBasis::UserWeights, ws));
                }
            }
            if chosen.is_none() {
                if let Some(est) = sources.estimate {
                    let r = Rounding::half_up(est.reporting.decimals);
                    let ws: Vec<Decimal> = els.iter().map(|e| est.element_cost(*e, r)).collect();
                    let zero: Vec<String> = els.iter().zip(&ws).filter(|(_, w)| **w <= Decimal::ZERO).map(|(e, _)| e.0.to_string()).collect();
                    if zero.is_empty() {
                        chosen = Some((WeightBasis::PricedValue, ws));
                    } else {
                        let why = format!("no priced value for element(s) {}", zero.join(", "));
                        note = Some(match note {
                            Some(n) => format!("{n}; {why}"),
                            None => why,
                        });
                    }
                }
            }
            let (basis, ws) = chosen.unwrap_or_else(|| (WeightBasis::Equal, vec![Decimal::ONE; els.len()]));
            activities.push(ActivityWeights { activity: id, elements: els.into_iter().zip(ws).collect(), basis, basis_note: note });
        }
        Ok(Rollup { activities, unlinked_activities: unlinked, missing_elements: missing, shared_elements })
    }

    /// Activity progress at the end of `date`. An element without a record counts at 0%.
    pub fn at(&self, log: &ProgressLog, date: Date) -> Result<RollupResult> {
        let percents = log.percents_at(date)?;
        let mut out = Vec::new();
        for a in &self.activities {
            let pairs: Vec<(Decimal, Decimal)> = a.elements.iter().map(|(e, w)| (*w, percents.get(e).copied().unwrap_or_default())).collect();
            let percent = weighted_mean(&pairs)?;
            let mut start: Option<(Date, ElementId)> = None;
            let mut finish: Option<(Date, ElementId)> = None;
            for (e, _) in &a.elements {
                if let Some(d) = log.start_date(*e, date)? {
                    if start.is_none_or(|(s, _)| d < s) {
                        start = Some((d, *e));
                    }
                }
                if percent >= Decimal::from_int(100) {
                    if let Some(d) = log.completion_date(*e, date)? {
                        if finish.is_none_or(|(f, _)| d > f) {
                            finish = Some((d, *e));
                        }
                    }
                }
            }
            out.push(ActivityProgress {
                activity: a.activity.clone(),
                percent,
                basis: a.basis,
                basis_note: a.basis_note.clone(),
                elements_total: a.elements.len(),
                elements_recorded: a.elements.iter().filter(|(e, _)| percents.contains_key(e)).count(),
                actual_start: start.map(|(d, e)| InferredDate { date: d, basis: format!("inferred: first record above 0% (element {})", e.0) }),
                actual_finish: finish.map(|(d, e)| InferredDate { date: d, basis: format!("inferred: last element to reach 100% (element {})", e.0) }),
            });
        }
        Ok(RollupResult { date, activities: out, unlinked_activities: self.unlinked_activities.clone() })
    }
}

/// Writes the updates onto the programme and sets its data date, so that
/// [`afrobim_schedule::schedule`] can reschedule from it.
///
/// For each updated activity the percent complete, actual start and actual finish are
/// **replaced** (a missing inferred date clears the field) and the remaining duration is cleared,
/// so the schedule crate's own default applies. Activities that are not in the updates (those no
/// element reaches) keep whatever progress they have. An update with a date after `data_date` is
/// refused.
pub fn apply_updates(programme: &mut Programme, data_date: Date, updates: &[ActivityUpdate]) -> Result<ApplyReport> {
    for u in updates {
        for d in [&u.actual_start, &u.actual_finish].into_iter().flatten() {
            if d.date > data_date {
                return invalid(format!("activity {}: inferred date {} is after the data date {data_date}", u.activity, d.date));
            }
        }
    }
    programme.data_date = Some(data_date);
    let mut report = ApplyReport { data_date, applied: Vec::new(), unknown_activities: Vec::new(), inferred_dates: Vec::new() };
    for u in updates {
        let Some(a) = programme.activity_mut(&u.activity) else {
            report.unknown_activities.push(u.activity.clone());
            continue;
        };
        a.progress.percent_complete = u.percent_complete.to_f64();
        a.progress.actual_start = u.actual_start.as_ref().map(|d| d.date);
        a.progress.actual_finish = u.actual_finish.as_ref().map(|d| d.date);
        a.progress.remaining_duration = None;
        if let Some(d) = &u.actual_start {
            report.inferred_dates.push((u.activity.clone(), "actual start".into(), d.date));
        }
        if let Some(d) = &u.actual_finish {
            report.inferred_dates.push((u.activity.clone(), "actual finish".into(), d.date));
        }
        report.applied.push(u.activity.clone());
    }
    Ok(report)
}
