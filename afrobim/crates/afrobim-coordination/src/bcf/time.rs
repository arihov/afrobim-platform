//! `xs:dateTime` text for Unix seconds, without a date library.
//!
//! The civil-from-days and days-from-civil conversions are the proleptic Gregorian algorithms
//! (H. Hinnant, "chrono-Compatible Low-Level Date Algorithms"); the tests pin known dates.

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DDThh:mm:ssZ` (UTC).
pub fn format_datetime(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// Parses an `xs:dateTime` (or a bare `xs:date`): `YYYY-MM-DDThh:mm:ss`, optional fractional
/// seconds (dropped), optional `Z` or `+hh:mm` / `-hh:mm` offset. No offset means UTC.
pub fn parse_datetime(text: &str) -> Option<i64> {
    let t = text.trim();
    let (date, rest) = match t.find('T') {
        Some(i) => (&t[..i], &t[i + 1..]),
        None => (t, ""),
    };
    let (neg, date) = match date.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, date),
    };
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    if dp.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if neg { -y } else { y };
    let mut secs = 0i64;
    let mut offset = 0i64;
    if !rest.is_empty() {
        let (clock, tz) = match rest.find(['Z', '+', '-']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let mut cp = clock.split(':');
        let h: i64 = cp.next()?.parse().ok()?;
        let mi: i64 = cp.next()?.parse().ok()?;
        let s_text = cp.next().unwrap_or("0");
        let s: i64 = s_text.split('.').next()?.parse().ok()?;
        if !(0..=24).contains(&h) || !(0..60).contains(&mi) || !(0..=60).contains(&s) {
            return None;
        }
        secs = h * 3600 + mi * 60 + s;
        if let Some(sign) = tz.chars().next().filter(|c| *c == '+' || *c == '-') {
            let body = &tz[1..];
            let (oh, om) = body.split_once(':').unwrap_or((body, "0"));
            let off = oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60;
            offset = if sign == '+' { off } else { -off };
        } else if !tz.is_empty() && tz != "Z" {
            return None;
        }
    }
    Some(days_from_civil(y, m, d) * 86_400 + secs - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(format_datetime(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_datetime(951_868_800), "2000-03-01T00:00:00Z");
        assert_eq!(format_datetime(1_709_208_000), "2024-02-29T12:00:00Z");
        assert_eq!(format_datetime(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn parse_inverts_format_over_a_range() {
        let mut t = -3_000_000_000i64;
        while t < 8_000_000_000 {
            assert_eq!(parse_datetime(&format_datetime(t)), Some(t), "{t}");
            t += 7_919_113;
        }
    }

    #[test]
    fn parse_accepts_offsets_fractions_and_dates() {
        assert_eq!(parse_datetime("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(parse_datetime("2024-02-29T12:00:00"), Some(1_709_208_000));
        assert_eq!(parse_datetime("2024-02-29T14:00:00+02:00"), Some(1_709_208_000));
        assert_eq!(parse_datetime("2024-02-29T10:30:00-01:30"), Some(1_709_208_000));
        assert_eq!(parse_datetime("2024-02-29T12:00:00.987Z"), Some(1_709_208_000));
        assert_eq!(parse_datetime("2000-03-01"), Some(951_868_800));
        assert_eq!(parse_datetime("tomorrow"), None);
        assert_eq!(parse_datetime("2024-13-01T00:00:00Z"), None);
    }
}
