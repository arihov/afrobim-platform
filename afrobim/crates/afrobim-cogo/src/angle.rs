//! Bearings and angles.
//!
//! A bearing in this crate is a whole circle bearing: degrees clockwise from grid north, in
//! [0, 360). Quadrant bearings (N45-30-00E) can be read and written for deed work.

use crate::error::{CogoError, Result};

pub fn normalize_deg(d: f64) -> f64 {
    let mut x = d % 360.0;
    if x < 0.0 {
        x += 360.0;
    }
    if x >= 360.0 {
        x = 0.0;
    }
    x
}

/// Normalise to (-180, 180].
pub fn signed_deg(d: f64) -> f64 {
    let x = normalize_deg(d);
    if x > 180.0 {
        x - 360.0
    } else {
        x
    }
}

/// Degrees, minutes, seconds to decimal degrees. Minutes (whole) and seconds must be below 60.
pub fn dms_to_deg(d: f64, m: f64, s: f64) -> Result<f64> {
    if !(0.0..60.0).contains(&m) || !(0.0..60.0).contains(&s) || m.fract() != 0.0 {
        return Err(CogoError::BadAngle(format!("{d} {m} {s}")));
    }
    let a = d.abs() + m / 60.0 + s / 3600.0;
    Ok(if d.is_sign_negative() { -a } else { a })
}

/// Parse an angle: decimal degrees (`123.4567`), or degrees minutes seconds in the forms
/// `123°45'30.5"`, `123-45-30.5`, `123 45 30.5`, `123d45m30.5s`. Not normalised.
pub fn parse_dms(text: &str) -> Result<f64> {
    let t = text.trim();
    let bad = || CogoError::BadAngle(text.to_string());
    if t.is_empty() {
        return Err(bad());
    }
    if let Ok(v) = t.parse::<f64>() {
        return if v.is_finite() { Ok(v) } else { Err(bad()) };
    }
    let neg = t.starts_with('-');
    let body = t.trim_start_matches(['-', '+']);
    let cleaned: String = body
        .chars()
        .map(|c| if matches!(c, '°' | '\'' | '"' | '′' | '″' | '-' | 'd' | 'D' | 'm' | 'M' | 's' | 'S' | '’' | '”') { ' ' } else { c })
        .collect();
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(bad());
    }
    let nums: std::result::Result<Vec<f64>, _> = parts.iter().map(|p| p.parse::<f64>()).collect();
    let nums = nums.map_err(|_| bad())?;
    let d = nums[0];
    let m = *nums.get(1).unwrap_or(&0.0);
    let s = *nums.get(2).unwrap_or(&0.0);
    if d < 0.0 || !d.is_finite() {
        return Err(bad());
    }
    let v = dms_to_deg(d, m, s).map_err(|_| bad())?;
    Ok(if neg { -v } else { v })
}

/// Decimal degrees to `DDD°MM'SS.sss"`. Seconds are rounded to `sec_decimals` places and a carry into
/// minutes and degrees is handled. The sign is kept.
pub fn format_dms(deg: f64, sec_decimals: usize) -> String {
    let neg = deg < 0.0;
    let a = deg.abs();
    let scale = 10u128.pow(sec_decimals as u32);
    let total = (a * 3600.0 * scale as f64).round() as u128;
    let per_deg = 3600 * scale;
    let d = total / per_deg;
    let rem = total % per_deg;
    let m = rem / (60 * scale);
    let s = (rem % (60 * scale)) as f64 / scale as f64;
    let sign = if neg && total > 0 { "-" } else { "" };
    let w = if sec_decimals == 0 { 2 } else { 3 + sec_decimals };
    format!("{sign}{d}\u{b0}{m:02}'{s:0w$.sec_decimals$}\"")
}

/// Parse a bearing: whole circle (any `parse_dms` form, normalised to [0,360)) or quadrant form
/// such as `N45-30-00E`, `S 10d15m W`.
pub fn parse_bearing(text: &str) -> Result<f64> {
    let t = text.trim();
    let bad = || CogoError::BadAngle(text.to_string());
    let up = t.to_ascii_uppercase();
    let (first, last) = (up.chars().next(), up.chars().last());
    if let (Some(f), Some(l)) = (first, last) {
        if (f == 'N' || f == 'S') && (l == 'E' || l == 'W') && up.len() >= 3 {
            let inner = &t[1..t.len() - 1];
            let a = parse_dms(inner)?;
            if !(0.0..=90.0).contains(&a) {
                return Err(bad());
            }
            let b = match (f, l) {
                ('N', 'E') => a,
                ('S', 'E') => 180.0 - a,
                ('S', 'W') => 180.0 + a,
                _ => 360.0 - a,
            };
            return Ok(normalize_deg(b));
        }
    }
    Ok(normalize_deg(parse_dms(t)?))
}

/// Whole circle bearing as `DDD°MM'SS"`.
pub fn format_bearing(b: f64, sec_decimals: usize) -> String {
    format_dms(normalize_deg(b), sec_decimals)
}

/// Quadrant bearing `N45°30'00"E`.
pub fn format_quadrant(b: f64, sec_decimals: usize) -> String {
    let b = normalize_deg(b);
    let (p, a, s) = if b <= 90.0 {
        ('N', b, 'E')
    } else if b <= 180.0 {
        ('S', 180.0 - b, 'E')
    } else if b <= 270.0 {
        ('S', b - 180.0, 'W')
    } else {
        ('N', 360.0 - b, 'W')
    };
    format!("{p}{}{s}", format_dms(a, sec_decimals))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_forms() {
        let want = 123.0 + 45.0 / 60.0 + 30.5 / 3600.0;
        for t in ["123°45'30.5\"", "123-45-30.5", "123 45 30.5", "123d45m30.5s"] {
            assert!((parse_dms(t).unwrap() - want).abs() < 1e-12, "{t}");
        }
        assert_eq!(parse_dms("12.5").unwrap(), 12.5);
        assert!(parse_dms("12 75 00").is_err());
        assert!(parse_dms("abc").is_err());
    }
    #[test]
    fn format_carry() {
        assert_eq!(format_dms(10.5, 0), "10\u{b0}30'00\"");
        assert_eq!(format_dms(10.0 + 59.0 / 60.0 + 59.9999 / 3600.0, 2), "11\u{b0}00'00.00\"");
        assert_eq!(format_dms(0.0, 1), "0\u{b0}00'00.0\"");
    }
    #[test]
    fn quadrant() {
        assert!((parse_bearing("N45-30-00E").unwrap() - 45.5).abs() < 1e-12);
        assert!((parse_bearing("S10-00-00W").unwrap() - 190.0).abs() < 1e-12);
        assert!((parse_bearing("N10-00-00W").unwrap() - 350.0).abs() < 1e-12);
        assert!((parse_bearing("S10-00-00E").unwrap() - 170.0).abs() < 1e-12);
        assert_eq!(format_quadrant(190.0, 0), "S10\u{b0}00'00\"W");
        assert!((parse_bearing("-10").unwrap() - 350.0).abs() < 1e-12);
        assert!(parse_bearing("N95-00-00E").is_err());
    }
}
