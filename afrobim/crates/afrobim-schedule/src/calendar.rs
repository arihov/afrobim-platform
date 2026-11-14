//! Working calendars: working weekdays and holidays entered by the user.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::date::{Date, Weekday};

/// A working calendar. Nothing is built in: the working weekdays and every holiday are entered
/// by the user. A day is a working day when its weekday is listed and it is not a holiday.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub name: String,
    pub working_days: BTreeSet<Weekday>,
    #[serde(default)]
    pub holidays: BTreeSet<Date>,
}

/// Upper bound on the day-by-day searches, so a calendar with no usable day cannot hang.
const SEARCH_LIMIT: i64 = 366 * 100;

impl Calendar {
    pub fn new(name: &str, working_days: &[Weekday]) -> Calendar {
        Calendar { name: name.to_string(), working_days: working_days.iter().copied().collect(), holidays: BTreeSet::new() }
    }

    /// Adds a holiday (a non-working day).
    pub fn with_holiday(mut self, d: Date) -> Calendar {
        self.holidays.insert(d);
        self
    }

    pub fn is_working(&self, d: Date) -> bool {
        self.working_days.contains(&d.weekday()) && !self.holidays.contains(&d)
    }

    /// `d` when it is a working day, otherwise the next working day after it.
    pub fn first_on_or_after(&self, d: Date) -> Date {
        let mut x = d;
        for _ in 0..SEARCH_LIMIT {
            if self.is_working(x) {
                return x;
            }
            x = x.add_days(1);
        }
        x
    }

    /// `d` when it is a working day, otherwise the last working day before it.
    pub fn last_on_or_before(&self, d: Date) -> Date {
        let mut x = d;
        for _ in 0..SEARCH_LIMIT {
            if self.is_working(x) {
                return x;
            }
            x = x.add_days(-1);
        }
        x
    }

    /// Moves `n` working days from `d` (later for positive `n`, earlier for negative). A
    /// non-working `d` is first moved to the next working day.
    pub fn shift(&self, d: Date, n: i64) -> Date {
        let mut x = self.first_on_or_after(d);
        let step = if n >= 0 { 1 } else { -1 };
        let mut left = n.abs();
        let mut guard = 0;
        while left > 0 && guard < SEARCH_LIMIT * 10 {
            x = x.add_days(step);
            if self.is_working(x) {
                left -= 1;
            }
            guard += 1;
        }
        x
    }

    /// Working days in `[a, b)`, negative when `b` is before `a`. So `a` itself counts when it is
    /// a working day and `b` does not.
    pub fn wd_between(&self, a: Date, b: Date) -> i64 {
        let (lo, hi, sign) = if a <= b { (a, b, 1) } else { (b, a, -1) };
        let mut n = 0;
        let mut x = lo;
        while x < hi {
            if self.is_working(x) {
                n += 1;
            }
            x = x.add_days(1);
        }
        n * sign
    }

    /// The working days from `from` to `to`, both inclusive.
    pub fn working_days_inclusive(&self, from: Date, to: Date) -> Vec<Date> {
        let mut out = Vec::new();
        let mut x = from;
        while x <= to {
            if self.is_working(x) {
                out.push(x);
            }
            x = x.add_days(1);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_weekends_and_entered_holidays() {
        let d = |m, day| Date::from_ymd(2026, m, day).unwrap();
        let cal = Calendar::new("5-day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]).with_holiday(d(3, 16));
        assert_eq!(cal.shift(d(3, 13), 1), d(3, 17)); // Fri +1 skips Sat, Sun and the Monday holiday
        assert_eq!(cal.shift(d(3, 17), -1), d(3, 13));
        assert_eq!(cal.wd_between(d(3, 13), d(3, 17)), 1);
        assert_eq!(cal.wd_between(d(3, 17), d(3, 13)), -1);
        assert_eq!(cal.first_on_or_after(d(3, 14)), d(3, 17));
        assert_eq!(cal.last_on_or_before(d(3, 15)), d(3, 13));
    }
}
