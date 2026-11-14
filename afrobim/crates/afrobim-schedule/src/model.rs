//! The programme: activities, logic, calendars, progress, baseline and 4D links.

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::baseline::Baseline;
use crate::calendar::Calendar;
use crate::date::Date;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkType {
    /// Finish to start.
    FS,
    /// Start to start.
    SS,
    /// Finish to finish.
    FF,
    /// Start to finish.
    SF,
}

/// A date constraint on one activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Constraint {
    /// Start no earlier than the date (start of that day).
    StartNoEarlierThan(Date),
    /// Finish no later than the date (end of that day). Creates negative float when missed.
    FinishNoLaterThan(Date),
    /// Must start on the date (start of that day): overrides the logic.
    MustStartOn(Date),
}

/// Which instant a zero-duration activity marks. It only matters for how the milestone meets
/// working time: a start milestone is the start of its working day, a finish milestone the end of
/// its working day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MilestoneKind {
    Start,
    #[default]
    Finish,
}

/// Progress entered on an activity as of the programme's data date.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Progress {
    /// 0 to 100.
    #[serde(default)]
    pub percent_complete: f64,
    #[serde(default)]
    pub actual_start: Option<Date>,
    #[serde(default)]
    pub actual_finish: Option<Date>,
    /// Working days still to do. When absent: `ceil(duration * (100 - percent) / 100)`, at least 1.
    #[serde(default)]
    pub remaining_duration: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    NotStarted,
    InProgress,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    /// Unique within the programme; dependencies and links refer to it.
    pub id: String,
    pub wbs: String,
    pub name: String,
    /// Working days (of the activity's calendar). Zero makes it a milestone.
    pub duration: u32,
    /// Name of a calendar of the programme; `None` uses the programme calendar.
    #[serde(default)]
    pub calendar: Option<String>,
    #[serde(default)]
    pub constraint: Option<Constraint>,
    #[serde(default)]
    pub milestone_kind: MilestoneKind,
    /// Weight for the S-curve (any positive unit: cost, quantity, man-hours). Optional.
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub progress: Progress,
}

impl Activity {
    pub fn new(id: &str, wbs: &str, name: &str, duration: u32) -> Activity {
        Activity {
            id: id.to_string(),
            wbs: wbs.to_string(),
            name: name.to_string(),
            duration,
            calendar: None,
            constraint: None,
            milestone_kind: MilestoneKind::default(),
            weight: None,
            progress: Progress::default(),
        }
    }

    pub fn is_milestone(&self) -> bool {
        self.duration == 0
    }

    pub fn status(&self) -> Status {
        let p = &self.progress;
        if p.actual_finish.is_some() || p.percent_complete >= 100.0 {
            Status::Complete
        } else if p.actual_start.is_some() || p.percent_complete > 0.0 {
            Status::InProgress
        } else {
            Status::NotStarted
        }
    }

    /// Working days still to do for an activity in progress.
    pub fn remaining(&self) -> u32 {
        match self.status() {
            Status::Complete => 0,
            Status::NotStarted => self.duration,
            Status::InProgress => self.progress.remaining_duration.unwrap_or_else(|| {
                let r = (self.duration as f64 * (100.0 - self.progress.percent_complete) / 100.0).ceil();
                (r as u32).max(1)
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub predecessor: String,
    pub successor: String,
    pub kind: LinkType,
    /// Working days of the successor's calendar; negative means overlap (lead).
    #[serde(default)]
    pub lag: i32,
}

/// What a linked element does in the activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkAction {
    /// The element is built: absent before, growing during, present after.
    Construct,
    /// The element exists and is removed: present before, shrinking during, gone after.
    Demolish,
    /// The element exists only while the activity runs (formwork, diversions): absent before,
    /// present during, removed after.
    Temporary,
}

/// A rule that picks model elements; every field given must match. A rule with no field given
/// matches nothing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ElementRule {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    /// Matched against the element's name (its mark). `*` matches any run of characters and `?`
    /// any one character; case-sensitive.
    #[serde(default)]
    pub mark_pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElementSelector {
    Id(ElementId),
    Rule(ElementRule),
}

/// Links an activity to model elements for the 4D view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityLink {
    pub activity: String,
    pub selector: ElementSelector,
    pub action: LinkAction,
}

/// The whole programme. It serialises with serde (see [`crate::SCHEDULE_CATEGORY`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Programme {
    pub name: String,
    /// The earliest date any activity may start.
    pub start: Date,
    pub calendars: Vec<Calendar>,
    /// Name of the calendar used by activities without one of their own.
    pub calendar: String,
    pub activities: Vec<Activity>,
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub links: Vec<ActivityLink>,
    /// Progress is reported as of the end of this day; remaining work cannot happen before the next
    /// working day.
    #[serde(default)]
    pub data_date: Option<Date>,
    /// A contract completion date. When present, float is measured against it.
    #[serde(default)]
    pub required_finish: Option<Date>,
    #[serde(default)]
    pub baseline: Option<Baseline>,
}

impl Programme {
    pub fn new(name: &str, start: Date, calendar: Calendar) -> Programme {
        let cal_name = calendar.name.clone();
        Programme {
            name: name.to_string(),
            start,
            calendars: vec![calendar],
            calendar: cal_name,
            activities: Vec::new(),
            dependencies: Vec::new(),
            links: Vec::new(),
            data_date: None,
            required_finish: None,
            baseline: None,
        }
    }

    pub fn add_activity(&mut self, a: Activity) -> &mut Activity {
        self.activities.push(a);
        self.activities.last_mut().expect("just pushed")
    }

    pub fn link(&mut self, predecessor: &str, successor: &str, kind: LinkType, lag: i32) {
        self.dependencies.push(Dependency { predecessor: predecessor.into(), successor: successor.into(), kind, lag });
    }

    pub fn activity(&self, id: &str) -> Option<&Activity> {
        self.activities.iter().find(|a| a.id == id)
    }

    pub fn activity_mut(&mut self, id: &str) -> Option<&mut Activity> {
        self.activities.iter_mut().find(|a| a.id == id)
    }

    pub fn calendar_named(&self, name: &str) -> Option<&Calendar> {
        self.calendars.iter().find(|c| c.name == name)
    }

    /// The calendar an activity works to.
    pub fn calendar_of(&self, a: &Activity) -> Option<&Calendar> {
        self.calendar_named(a.calendar.as_deref().unwrap_or(&self.calendar))
    }

    /// Links the activity to model elements for the 4D view.
    pub fn link_elements(&mut self, activity: &str, selector: ElementSelector, action: LinkAction) {
        self.links.push(ActivityLink { activity: activity.into(), selector, action });
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a programme always serialises")
    }

    pub fn from_value(v: &serde_json::Value) -> Result<Programme, String> {
        serde_json::from_value(v.clone()).map_err(|e| format!("not a valid programme: {e}"))
    }
}
