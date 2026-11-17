//! Angle units and the vertical angle types. Inside the database every angle is decimal degrees,
//! every length is metres and every vertical angle is a zenith angle. Other units only enter or leave
//! through these conversions, with explicit factors for distances.

use afrobim_cogo::parse_dms;
use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AngleUnit {
    /// Decimal degrees.
    Degrees,
    /// Gradians (400 per circle).
    Gon,
    /// Degrees, minutes, seconds written with separators: `123-45-30.5`, `123 45 30.5`, `123d45m30.5s`
    /// (the forms `afrobim_cogo::parse_dms` reads).
    DmsText,
    /// Packed DDD.MMSSss as used by surveying packages: 100.2045 is 100 degrees 20 minutes 45 seconds.
    DmsPacked,
}

/// How the vertical angle of a record is measured. Stored internally as a zenith angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalKind {
    /// Zero at the zenith, 90 degrees horizontal.
    Zenith,
    /// Zero horizontal, positive up (the "horizontal" type of the equipment dialog of Civil 3D).
    Elevation,
    /// Measured from straight down: 270 degrees is horizontal for a level line (nadir = zenith + 180).
    Nadir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HzDirection {
    /// Angles and circle readings increase clockwise.
    Right,
    /// Angles and circle readings increase anticlockwise.
    Left,
}

pub fn parse_angle(text: &str, unit: AngleUnit) -> Result<f64> {
    let t = text.trim();
    let bad = || SurveyError::Invalid(format!("cannot read angle '{text}'"));
    match unit {
        AngleUnit::Degrees => t.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(bad),
        AngleUnit::Gon => t.parse::<f64>().ok().filter(|v| v.is_finite()).map(|g| g * 0.9).ok_or_else(bad),
        AngleUnit::DmsText => parse_dms(t).map_err(|_| bad()),
        AngleUnit::DmsPacked => parse_packed(t).ok_or_else(bad),
    }
}

fn parse_packed(t: &str) -> Option<f64> {
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if int.is_empty() || !int.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let d: f64 = int.parse().ok()?;
    let mut f = frac.to_string();
    while f.len() < 4 {
        f.push('0');
    }
    let mm: f64 = f[0..2].parse().ok()?;
    let ss: f64 = format!("{}.{}", &f[2..4], &f[4..]).trim_end_matches('.').parse().ok()?;
    if mm >= 60.0 || ss >= 60.0 {
        return None;
    }
    let v = d + mm / 60.0 + ss / 3600.0;
    Some(if neg { -v } else { v })
}

/// Decimal degrees to the text form of a unit (round trips with `parse_angle` to the digits given).
pub fn format_angle(deg: f64, unit: AngleUnit, decimals: usize) -> String {
    match unit {
        AngleUnit::Degrees => format!("{deg:.decimals$}"),
        AngleUnit::Gon => format!("{:.decimals$}", deg / 0.9),
        AngleUnit::DmsText => afrobim_cogo::format_dms(deg, decimals.saturating_sub(4)),
        AngleUnit::DmsPacked => {
            let neg = deg < 0.0;
            let a = deg.abs();
            let scale = 10f64.powi(decimals.saturating_sub(4) as i32);
            let total = (a * 3600.0 * scale).round();
            let per_deg = 3600.0 * scale;
            let d = (total / per_deg).floor();
            let rem = total - d * per_deg;
            let m = (rem / (60.0 * scale)).floor();
            let s = rem - m * 60.0 * scale;
            let sd = decimals.saturating_sub(4);
            format!("{}{}.{:02}{:0w$}", if neg { "-" } else { "" }, d as i64, m as i64, s as i64, w = 2 + sd)
        }
    }
}

/// Zenith angle in degrees from a vertical angle of the given kind (degrees).
pub fn to_zenith(deg: f64, kind: VerticalKind) -> f64 {
    match kind {
        VerticalKind::Zenith => deg,
        VerticalKind::Elevation => 90.0 - deg,
        VerticalKind::Nadir => deg - 180.0,
    }
}

pub fn from_zenith(z: f64, kind: VerticalKind) -> f64 {
    match kind {
        VerticalKind::Zenith => z,
        VerticalKind::Elevation => 90.0 - z,
        VerticalKind::Nadir => z + 180.0,
    }
}

/// Smallest signed difference a - b in (-180, 180].
pub fn diff_deg(a: f64, b: f64) -> f64 {
    afrobim_cogo::angle::signed_deg(a - b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_and_gon() {
        let v = parse_angle("100.2045", AngleUnit::DmsPacked).unwrap();
        assert!((v - (100.0 + 20.0 / 60.0 + 45.0 / 3600.0)).abs() < 1e-12);
        assert!(parse_angle("89.5370", AngleUnit::DmsPacked).is_err(), "70 seconds is not valid");
        assert!((parse_angle("100", AngleUnit::Gon).unwrap() - 90.0).abs() < 1e-12);
        assert!((parse_angle("-0.0030", AngleUnit::DmsPacked).unwrap() + 30.0 / 3600.0).abs() < 1e-12);
    }

    #[test]
    fn vertical_kinds() {
        // 89 zenith = 1 horizontal = 269 nadir (Civil 3D equipment dialog)
        assert!((to_zenith(1.0, VerticalKind::Elevation) - 89.0).abs() < 1e-12);
        assert!((to_zenith(269.0, VerticalKind::Nadir) - 89.0).abs() < 1e-12);
    }

    #[test]
    fn format_round_trip() {
        let v = 123.456789;
        for u in [AngleUnit::Degrees, AngleUnit::Gon] {
            let t = format_angle(v, u, 8);
            assert!((parse_angle(&t, u).unwrap() - v).abs() < 1e-7);
        }
        let t = format_angle(v, AngleUnit::DmsPacked, 6);
        assert!((parse_angle(&t, AngleUnit::DmsPacked).unwrap() - v).abs() < 1e-5, "{t}");
    }
}
