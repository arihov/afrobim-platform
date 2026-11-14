//! Calendar dates (proleptic Gregorian) with day counting, for exchange-rate dates, rate dates and
//! the day-count proration of cash flow. No time zones and no time of day.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CostError, Result};

/// A calendar date. Orders chronologically; serialises as `YYYY-MM-DD`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

/// Days in a month of a year.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Date> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(CostError::Date(format!("{year:04}-{month:02}-{day:02}")));
        }
        Ok(Date { year, month, day })
    }

    pub fn parse(text: &str) -> Result<Date> {
        let bad = || CostError::Date(text.to_string());
        let mut parts = text.trim().split('-');
        let (y, m, d) = (parts.next().ok_or_else(bad)?, parts.next().ok_or_else(bad)?, parts.next().ok_or_else(bad)?);
        if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return Err(bad());
        }
        Date::new(y.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?).map_err(|_| bad())
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }

    /// Days since 1970-01-01 (negative before).
    pub fn to_days(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub fn from_days(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        Date { year: (y + i64::from(m <= 2)) as i32, month: m as u8, day: d as u8 }
    }

    pub fn add_days(self, n: i64) -> Date {
        Date::from_days(self.to_days() + n)
    }

    /// The last day of this date's month.
    pub fn end_of_month(self) -> Date {
        Date { year: self.year, month: self.month, day: days_in_month(self.year, self.month) }
    }

    /// The first day of this date's month.
    pub fn start_of_month(self) -> Date {
        Date { year: self.year, month: self.month, day: 1 }
    }

    /// Whole days from `self` to `other` (negative when `other` is earlier).
    pub fn days_until(self, other: Date) -> i64 {
        other.to_days() - self.to_days()
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Date, D::Error> {
        let s = String::deserialize(d)?;
        Date::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_and_back() {
        assert_eq!(Date::new(1970, 1, 1).unwrap().to_days(), 0);
        assert_eq!(Date::new(2000, 3, 1).unwrap().to_days(), 11_017);
        for n in [-800_000i64, -1, 0, 59, 60, 11_016, 20_500, 100_000] {
            assert_eq!(Date::from_days(n).to_days(), n);
        }
        assert_eq!(Date::new(2024, 2, 29).unwrap().add_days(1), Date::new(2024, 3, 1).unwrap());
        assert_eq!(Date::new(2026, 1, 20).unwrap().days_until(Date::new(2026, 3, 10).unwrap()), 49);
    }

    #[test]
    fn parsing_and_validity() {
        assert_eq!(Date::parse("2026-02-28").unwrap().to_string(), "2026-02-28");
        assert!(Date::parse("2026-02-29").is_err());
        assert!(Date::parse("2026-2-1").is_err());
        assert!(Date::parse("2026-13-01").is_err());
        assert_eq!(Date::new(2100, 2, 1).unwrap().end_of_month().day(), 28);
        assert_eq!(Date::new(2000, 2, 1).unwrap().end_of_month().day(), 29);
    }
}
