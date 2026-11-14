//! Calendar dates by plain arithmetic (proleptic Gregorian), no external date crate.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A calendar date, stored as days since 1970-01-01. Serialises as `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun];

    /// Monday = 0 ... Sunday = 6.
    pub fn index(self) -> usize {
        self as usize
    }
}

impl Date {
    /// `None` when the month or day does not exist.
    pub fn from_ymd(y: i32, m: u32, d: u32) -> Option<Date> {
        if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
            return None;
        }
        // Howard Hinnant's days_from_civil.
        let y = y as i64 - if m <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m as i64 + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Some(Date(era * 146097 + doe - 719468))
    }

    pub fn ymd(self) -> (i32, u32, u32) {
        let z = self.0 + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        (y as i32, m, d)
    }

    /// Parses `YYYY-MM-DD`; a trailing time (`T08:00:00`) is ignored.
    pub fn parse(text: &str) -> Option<Date> {
        let t = text.trim().get(..10)?;
        let mut it = t.split('-');
        let y: i32 = it.next()?.parse().ok()?;
        let m: u32 = it.next()?.parse().ok()?;
        let d: u32 = it.next()?.parse().ok()?;
        if it.next().is_some() {
            return None;
        }
        Date::from_ymd(y, m, d)
    }

    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday (index 3).
        Weekday::ALL[(self.0 + 3).rem_euclid(7) as usize]
    }

    /// The date `n` calendar days later (earlier when negative).
    pub fn add_days(self, n: i64) -> Date {
        Date(self.0 + n)
    }

    /// Calendar days from `earlier` to `self` (negative when `self` is earlier).
    pub fn days_since(self, earlier: Date) -> i64 {
        self.0 - earlier.0
    }

    pub fn month_name(self) -> &'static str {
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][self.ymd().1 as usize - 1]
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Date, D::Error> {
        let text = String::deserialize(d)?;
        if text.len() != 10 {
            return Err(serde::de::Error::custom(format!("'{text}' is not a date (YYYY-MM-DD)")));
        }
        Date::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("'{text}' is not a date (YYYY-MM-DD)")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_weekdays() {
        let d = Date::from_ymd(2026, 3, 2).unwrap();
        assert_eq!(d.weekday(), Weekday::Mon);
        assert_eq!(d.to_string(), "2026-03-02");
        assert_eq!(Date::parse("2026-03-02T08:00:00"), Some(d));
        assert_eq!(Date::from_ymd(1970, 1, 1).unwrap().weekday(), Weekday::Thu);
        assert_eq!(Date::from_ymd(2024, 2, 29).unwrap().add_days(1).to_string(), "2024-03-01");
        assert!(Date::from_ymd(2025, 2, 29).is_none());
        for n in [-800, -1, 0, 1, 59, 20000] {
            let (y, m, dd) = Date(n).ymd();
            assert_eq!(Date::from_ymd(y, m, dd), Some(Date(n)));
        }
    }
}
