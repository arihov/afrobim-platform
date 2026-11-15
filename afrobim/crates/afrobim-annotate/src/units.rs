//! Display units: how lengths, stations, areas, volumes and angles are shown and typed.
//!
//! The model is in millimetres (areas mm2, volumes mm3, angles degrees anticlockwise from +X).
//! [`DisplayUnits`] says how those are shown to the user; it is stored in the document (see
//! [`crate::settings`]). Metric only. Every `format` function has a `parse` partner that accepts
//! what it writes, so a value shown on the command line can be typed back.
//!
//! Defaults are neutral starting values and are labelled as defaults: nothing here is a
//! drafting-standard requirement.

use afrobim_alignment::format_station;
use serde::{Deserialize, Serialize};

/// How lengths are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    Millimetre,
    Metre,
}

impl LengthUnit {
    /// Model millimetres per one of this unit.
    pub fn mm_per_unit(self) -> f64 {
        match self {
            LengthUnit::Millimetre => 1.0,
            LengthUnit::Metre => 1000.0,
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            LengthUnit::Millimetre => "mm",
            LengthUnit::Metre => "m",
        }
    }
}

/// How areas are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AreaUnit {
    SquareMetre,
    Hectare,
}

impl AreaUnit {
    /// Model mm2 per one of this unit.
    pub fn mm2_per_unit(self) -> f64 {
        match self {
            AreaUnit::SquareMetre => 1.0e6,
            AreaUnit::Hectare => 1.0e10,
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            AreaUnit::SquareMetre => "m2",
            AreaUnit::Hectare => "ha",
        }
    }
}

/// How angles are shown.
///
/// `Decimal` and `Dms` format a plain angle (a sweep). For a direction ([`DisplayUnits::direction`])
/// the two bearing formats also apply: `QuadrantBearing` is `N 45°30'00" E` and `Azimuth` is the
/// angle clockwise from north in decimal degrees. A sweep shown in a bearing format falls back to
/// `Dms` (a sweep has no compass quadrant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AngleFormat {
    Decimal,
    Dms,
    QuadrantBearing,
    Azimuth,
}

/// The display-unit settings of a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayUnits {
    pub length: LengthUnit,
    pub length_decimals: u8,
    /// Decimals of the metre part of a station, `1+250.000` is 3.
    pub station_decimals: u8,
    pub area: AreaUnit,
    pub area_decimals: u8,
    /// Volumes are always cubic metres.
    pub volume_decimals: u8,
    pub angle: AngleFormat,
    /// Decimals of the degrees (Decimal, Azimuth) or of the seconds (Dms, QuadrantBearing).
    pub angle_decimals: u8,
}

impl Default for DisplayUnits {
    /// Default values, editable: metres to 3 decimals, stations to 3, square metres to 2, cubic
    /// metres to 2, decimal degrees to 2.
    fn default() -> Self {
        DisplayUnits {
            length: LengthUnit::Metre,
            length_decimals: 3,
            station_decimals: 3,
            area: AreaUnit::SquareMetre,
            area_decimals: 2,
            volume_decimals: 2,
            angle: AngleFormat::Decimal,
            angle_decimals: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("'{0}' is not a number")]
    Number(String),
    #[error("'{0}' is not a length (examples: 12.5, 12.5m, 12500mm)")]
    Length(String),
    #[error("'{0}' is not a station (example: 1+250.500)")]
    Station(String),
    #[error("'{0}' is not an area (examples: 300, 300m2, 1.5ha)")]
    Area(String),
    #[error("'{0}' is not a volume (examples: 300, 300m3)")]
    Volume(String),
    #[error("'{0}' is not an angle (examples: 45.5, 45d30'15\", N 45d30'15\" E)")]
    Angle(String),
}

/// `value` with `decimals` decimals, without a negative zero.
pub fn fixed(value: f64, decimals: u8) -> String {
    if !value.is_finite() {
        return "nan".into();
    }
    let s = format!("{:.*}", decimals as usize, value);
    if s.starts_with('-') && !s.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        s[1..].to_string()
    } else {
        s
    }
}

impl DisplayUnits {
    /// A length given in model millimetres, as a number in the display unit.
    pub fn length(&self, mm: f64) -> String {
        fixed(mm / self.length.mm_per_unit(), self.length_decimals)
    }

    /// As [`length`](Self::length) with the unit, e.g. `12.500 m`.
    pub fn length_u(&self, mm: f64) -> String {
        format!("{} {}", self.length(mm), self.length.suffix())
    }

    /// A station in metres (the displayed station), as `k+mmm` with the chosen decimals.
    pub fn station(&self, station_m: f64) -> String {
        format_station(station_m, self.station_decimals as usize)
    }

    /// An area given in model mm2.
    pub fn area(&self, mm2: f64) -> String {
        fixed(mm2 / self.area.mm2_per_unit(), self.area_decimals)
    }

    pub fn area_u(&self, mm2: f64) -> String {
        format!("{} {}", self.area(mm2), self.area.suffix())
    }

    /// A volume given in model mm3, in cubic metres.
    pub fn volume(&self, mm3: f64) -> String {
        fixed(mm3 / 1.0e9, self.volume_decimals)
    }

    pub fn volume_u(&self, mm3: f64) -> String {
        format!("{} m3", self.volume(mm3))
    }

    /// A plain angle (a sweep) in degrees.
    pub fn angle(&self, deg: f64) -> String {
        match self.angle {
            AngleFormat::Decimal | AngleFormat::Azimuth => format!("{}\u{b0}", fixed(deg, self.angle_decimals)),
            AngleFormat::Dms | AngleFormat::QuadrantBearing => dms(deg, self.angle_decimals),
        }
    }

    /// A direction: `math_deg` is anticlockwise from +X (east). Bearings measure from north.
    pub fn direction(&self, math_deg: f64) -> String {
        let az = azimuth_of(math_deg);
        match self.angle {
            AngleFormat::Decimal => format!("{}\u{b0}", fixed(math_deg.rem_euclid(360.0), self.angle_decimals)),
            AngleFormat::Dms => dms(math_deg.rem_euclid(360.0), self.angle_decimals),
            AngleFormat::Azimuth => format!("{}\u{b0}", fixed(az, self.angle_decimals)),
            AngleFormat::QuadrantBearing => {
                let (ns, theta, ew) = if az <= 90.0 {
                    ('N', az, 'E')
                } else if az <= 180.0 {
                    ('S', 180.0 - az, 'E')
                } else if az <= 270.0 {
                    ('S', az - 180.0, 'W')
                } else {
                    ('N', 360.0 - az, 'W')
                };
                format!("{ns} {} {ew}", dms(theta, self.angle_decimals))
            }
        }
    }

    /// Parses a length typed by the user into model millimetres. A bare number is in the display
    /// unit; a trailing `m` or `mm` overrides it.
    pub fn parse_length(&self, text: &str) -> Result<f64, ParseError> {
        let t = text.trim();
        let (num, factor) = if let Some(n) = strip_suffix_ci(t, "mm") {
            (n, 1.0)
        } else if let Some(n) = strip_suffix_ci(t, "m") {
            (n, 1000.0)
        } else {
            (t, self.length.mm_per_unit())
        };
        number(num).map(|v| v * factor).map_err(|_| ParseError::Length(text.to_string()))
    }

    /// Parses `k+mmm` (or a bare number of metres) into metres. After the `+` the metre part must
    /// be exactly three integer digits (so `1+5` is refused rather than guessed).
    pub fn parse_station(&self, text: &str) -> Result<f64, ParseError> {
        parse_station(text)
    }

    /// Parses an area into model mm2. A bare number is in the display unit; `m2`, `m^2` or `ha` override it.
    pub fn parse_area(&self, text: &str) -> Result<f64, ParseError> {
        let t = text.trim();
        let (num, factor) = if let Some(n) = strip_suffix_ci(t, "ha") {
            (n, AreaUnit::Hectare.mm2_per_unit())
        } else if let Some(n) = strip_suffix_ci(t, "m2").or_else(|| strip_suffix_ci(t, "m^2")) {
            (n, AreaUnit::SquareMetre.mm2_per_unit())
        } else {
            (t, self.area.mm2_per_unit())
        };
        number(num).map(|v| v * factor).map_err(|_| ParseError::Area(text.to_string()))
    }

    /// Parses a volume into model mm3. A bare number is cubic metres; `m3` or `m^3` is accepted.
    pub fn parse_volume(&self, text: &str) -> Result<f64, ParseError> {
        let t = text.trim();
        let num = strip_suffix_ci(t, "m3").or_else(|| strip_suffix_ci(t, "m^3")).unwrap_or(t);
        number(num).map(|v| v * 1.0e9).map_err(|_| ParseError::Volume(text.to_string()))
    }

    /// Parses a plain angle (decimal degrees or degrees-minutes-seconds) into degrees.
    pub fn parse_angle(&self, text: &str) -> Result<f64, ParseError> {
        parse_angle(text)
    }

    /// Parses a direction into degrees anticlockwise from +X. Accepts a quadrant bearing
    /// (`N 45d30'00" E`) and, for a bare angle, reads it as an azimuth (clockwise from north) when
    /// the display format is a bearing format, otherwise as anticlockwise from +X.
    pub fn parse_direction(&self, text: &str) -> Result<f64, ParseError> {
        let t = text.trim();
        if let Some(d) = parse_quadrant(t) {
            return d.ok_or_else(|| ParseError::Angle(text.to_string()));
        }
        let a = parse_angle(t)?;
        Ok(match self.angle {
            AngleFormat::QuadrantBearing | AngleFormat::Azimuth => (90.0 - a).rem_euclid(360.0),
            _ => a.rem_euclid(360.0),
        })
    }
}

/// Azimuth (clockwise from north, 0 to 360) of a direction given anticlockwise from +X.
pub fn azimuth_of(math_deg: f64) -> f64 {
    (90.0 - math_deg).rem_euclid(360.0)
}

/// Degrees as `D°MM'SS.ss"` with `sec_decimals` decimals of the seconds. Carries cleanly:
/// 59.9999 seconds rounds up into the minute.
pub fn dms(deg: f64, sec_decimals: u8) -> String {
    if !deg.is_finite() {
        return "nan".into();
    }
    let scale = 10f64.powi(sec_decimals as i32);
    // Work in whole units of the last shown second decimal so the carry is exact.
    let total = (deg.abs() * 3600.0 * scale).round() as u128;
    let per_min = 60 * scale as u128;
    let per_deg = 3600 * scale as u128;
    let d = total / per_deg;
    let m = (total % per_deg) / per_min;
    let s = (total % per_min) as f64 / scale;
    let neg = deg < 0.0 && total > 0;
    let sec = fixed(s, sec_decimals);
    // Width of the seconds field: two integer digits.
    let sec = if sec.split('.').next().map_or(0, str::len) < 2 { format!("0{sec}") } else { sec };
    format!("{}{d}\u{b0}{m:02}'{sec}\"", if neg { "-" } else { "" })
}

fn number(s: &str) -> Result<f64, ParseError> {
    let t = s.trim();
    t.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| ParseError::Number(s.to_string()))
}

fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let n = s.len().checked_sub(suffix.len())?;
    if s.is_char_boundary(n) && s[n..].eq_ignore_ascii_case(suffix) {
        Some(s[..n].trim_end())
    } else {
        None
    }
}

/// `k+mmm[.fff]`, or a plain number of metres. Free function for callers without a `DisplayUnits`.
pub fn parse_station(text: &str) -> Result<f64, ParseError> {
    let err = || ParseError::Station(text.to_string());
    let t = text.trim();
    let Some((km, m)) = t.split_once('+') else {
        return number(t).map_err(|_| err());
    };
    let (neg, km) = match km.trim().strip_prefix('-') {
        Some(k) => (true, k),
        None => (false, km.trim()),
    };
    let m = m.trim();
    let int_digits = m.split('.').next().unwrap_or("");
    if km.is_empty() || !km.bytes().all(|b| b.is_ascii_digit()) || int_digits.len() != 3 || !int_digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err());
    }
    let mval = number(m).map_err(|_| err())?;
    if m.starts_with('-') || m.starts_with('+') {
        return Err(err());
    }
    let km: f64 = km.parse().map_err(|_| err())?;
    let v = km * 1000.0 + mval;
    Ok(if neg { -v } else { v })
}

/// Degrees from `45.5`, `45d30m15s`, `45°30'15"` or `45 30 15`. A leading `-` negates.
pub fn parse_angle(text: &str) -> Result<f64, ParseError> {
    let err = || ParseError::Angle(text.to_string());
    let t = text.trim();
    if t.is_empty() {
        return Err(err());
    }
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b.trim_start()),
        None => (false, t),
    };
    let v = parse_dms_body(body).ok_or_else(err)?;
    Ok(if neg { -v } else { v })
}

fn parse_dms_body(body: &str) -> Option<f64> {
    // Tokenise into (number, marker) pairs.
    let mut parts: Vec<(f64, Option<char>)> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let v: f64 = chars[start..i].iter().collect::<String>().parse().ok()?;
            // Optional marker directly after (spaces allowed).
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let mut marker = None;
            if j < chars.len() {
                let m = match chars[j] {
                    '\u{b0}' | 'd' | 'D' => Some('d'),
                    '\'' | '\u{2032}' | 'm' | 'M' => Some('m'),
                    '"' | '\u{2033}' | 's' | 'S' => Some('s'),
                    _ => None,
                };
                if let Some(m) = m {
                    marker = Some(m);
                    i = j + 1;
                    // Swallow "deg" and the second quote of '' used as seconds.
                    if m == 'd' && chars.get(i..i + 2).is_some_and(|r| r.iter().collect::<String>().eq_ignore_ascii_case("eg")) {
                        i += 2;
                    }
                    if m == 'm' && chars.get(i) == Some(&'\'') {
                        marker = Some('s');
                        i += 1;
                    }
                }
            }
            parts.push((v, marker));
        } else {
            return None;
        }
    }
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let any_marker = parts.iter().any(|p| p.1.is_some());
    if !any_marker {
        return match parts.len() {
            1 => Some(parts[0].0),
            2 => Some(parts[0].0 + parts[1].0 / 60.0),
            _ => Some(parts[0].0 + parts[1].0 / 60.0 + parts[2].0 / 3600.0),
        };
    }
    // With markers: each part must carry one, in the order d, m, s, each at most once.
    let mut total = 0.0;
    let mut last_rank = -1i32;
    for (v, m) in &parts {
        let (rank, div) = match (*m)? {
            'd' => (0, 1.0),
            'm' => (1, 60.0),
            _ => (2, 3600.0),
        };
        if rank <= last_rank {
            return None;
        }
        last_rank = rank;
        total += v / div;
    }
    Some(total)
}

/// `Some(Some(math_deg))` for a valid quadrant bearing, `Some(None)` when it looks like one but
/// is out of range, `None` when it is not a bearing at all.
fn parse_quadrant(t: &str) -> Option<Option<f64>> {
    let up: Vec<char> = t.chars().collect();
    let first = up.first()?.to_ascii_uppercase();
    let last = up.last()?.to_ascii_uppercase();
    if up.len() < 3 || !matches!(first, 'N' | 'S') || !matches!(last, 'E' | 'W') {
        return None;
    }
    let mid: String = up[1..up.len() - 1].iter().collect();
    let Some(theta) = parse_dms_body(mid.trim()) else { return Some(None) };
    if !(0.0..=90.0).contains(&theta) {
        return Some(None);
    }
    let az = match (first, last) {
        ('N', 'E') => theta,
        ('N', 'W') => 360.0 - theta,
        ('S', 'E') => 180.0 - theta,
        _ => 180.0 + theta,
    };
    Some(Some((90.0 - az).rem_euclid(360.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn lengths_in_both_units_round_trip() {
        let mut u = DisplayUnits::default();
        assert_eq!(u.length(12500.0), "12.500");
        assert_eq!(u.length_u(12500.0), "12.500 m");
        assert!(close(u.parse_length("12.5").unwrap(), 12500.0)); // bare number is metres
        assert!(close(u.parse_length("12500 mm").unwrap(), 12500.0));
        assert!(close(u.parse_length("12.5M").unwrap(), 12500.0));
        u.length = LengthUnit::Millimetre;
        u.length_decimals = 1;
        assert_eq!(u.length_u(12345.67), "12345.7 mm");
        assert!(close(u.parse_length("12345.7").unwrap(), 12345.7));
        assert!(u.parse_length("abc").is_err());
        assert!(u.parse_length("1e999").is_err());
        assert_eq!(u.length(-0.0004), "0.0"); // no negative zero
    }

    #[test]
    fn stations_format_and_parse() {
        let u = DisplayUnits::default();
        assert_eq!(u.station(1250.5), "1+250.500");
        assert_eq!(u.station(999.9996), "1+000.000");
        assert_eq!(u.station(-50.0), "-0+050.000");
        assert!(close(u.parse_station("1+250.500").unwrap(), 1250.5));
        assert!(close(u.parse_station("0+050").unwrap(), 50.0));
        assert!(close(u.parse_station("-0+050.000").unwrap(), -50.0));
        assert!(close(u.parse_station("1250.5").unwrap(), 1250.5));
        assert!(u.parse_station("1+5").is_err());
        assert!(u.parse_station("1+2500").is_err());
        assert!(u.parse_station("a+250").is_err());
        let mut u2 = u.clone();
        u2.station_decimals = 1;
        assert_eq!(u2.station(12345.678), "12+345.7");
        assert!(close(u2.parse_station(&u2.station(12345.678)).unwrap(), 12345.7));
    }

    #[test]
    fn areas_and_volumes() {
        let mut u = DisplayUnits::default();
        // 300 m2 is 3e8 mm2.
        assert_eq!(u.area_u(3.0e8), "300.00 m2");
        assert!(close(u.parse_area("300 m2").unwrap(), 3.0e8));
        assert!(close(u.parse_area("300").unwrap(), 3.0e8));
        assert!(close(u.parse_area("1.5ha").unwrap(), 1.5e10 * 1.0));
        u.area = AreaUnit::Hectare;
        u.area_decimals = 4;
        assert_eq!(u.area_u(1.5e10), "1.5000 ha");
        assert!(close(u.parse_area("1.5").unwrap(), 1.5e10));
        assert_eq!(u.volume_u(2.5e9), "2.50 m3");
        assert!(close(u.parse_volume("2.5 m3").unwrap(), 2.5e9));
        assert!(u.parse_volume("x").is_err());
    }

    #[test]
    fn dms_is_hand_worked() {
        // 45.5 deg = 45 deg 30 min 0 sec.
        assert_eq!(dms(45.5, 0), "45\u{b0}30'00\"");
        // 12.3456789 deg: 0.3456789*60 = 20.740734 min; 0.740734*60 = 44.44404 sec.
        assert_eq!(dms(12.3456789, 2), "12\u{b0}20'44.44\"");
        // Carry: 29.99999 sec shown to 0 decimals is 30; 59.9999 s rounds into the next minute.
        assert_eq!(dms(10.0 + 59.0 / 60.0 + 59.9999 / 3600.0, 2), "11\u{b0}00'00.00\"");
        assert_eq!(dms(-0.5, 0), "-0\u{b0}30'00\"");
    }

    #[test]
    fn angles_round_trip() {
        let mut u = DisplayUnits::default();
        assert_eq!(u.angle(45.5), "45.50\u{b0}");
        assert!(close(u.parse_angle("45.50\u{b0}").unwrap(), 45.5));
        u.angle = AngleFormat::Dms;
        u.angle_decimals = 1;
        let s = u.angle(123.456);
        assert_eq!(s, "123\u{b0}27'21.6\"");
        assert!((u.parse_angle(&s).unwrap() - 123.456).abs() < 0.05 / 3600.0);
        assert!(close(u.parse_angle("45d30m").unwrap(), 45.5));
        assert!(close(u.parse_angle("45 30 0").unwrap(), 45.5));
        assert!(close(u.parse_angle("-10d30'").unwrap(), -10.5));
        assert!(u.parse_angle("10m 20d").is_err());
        assert!(u.parse_angle("1x").is_err());
        assert!(u.parse_angle("").is_err());
    }

    #[test]
    fn bearings_round_trip_in_every_quadrant() {
        let mut u = DisplayUnits { angle: AngleFormat::QuadrantBearing, angle_decimals: 0, ..DisplayUnits::default() };
        // East is math 0, azimuth 90: N 90 E. North is math 90: N 0 E. West: math 180, azimuth 270.
        assert_eq!(u.direction(90.0), "N 0\u{b0}00'00\" E");
        assert_eq!(u.direction(0.0), "N 90\u{b0}00'00\" E");
        // Azimuth 135 (south-east, 45 from south): math = 90 - 135 = -45 = 315.
        assert_eq!(u.direction(315.0), "S 45\u{b0}00'00\" E");
        assert_eq!(u.direction(225.0), "S 45\u{b0}00'00\" W"); // azimuth 225
        assert_eq!(u.direction(135.0), "N 45\u{b0}00'00\" W"); // azimuth 315
        for math in [10.0, 80.0, 100.0, 170.0, 190.0, 260.0, 280.0, 350.0] {
            let s = u.direction(math);
            assert!((u.parse_direction(&s).unwrap() - math).abs() < 1.0 / 3600.0, "{s}");
        }
        assert!(close(u.parse_direction("N45E").unwrap(), 45.0));
        assert!(u.parse_direction("N 120 E").is_err());
        // Azimuth: bare numbers are azimuths when the format is a bearing format.
        u.angle = AngleFormat::Azimuth;
        u.angle_decimals = 2;
        assert_eq!(u.direction(0.0), "90.00\u{b0}");
        assert!(close(u.parse_direction("90").unwrap(), 0.0));
        u.angle = AngleFormat::Decimal;
        assert!(close(u.parse_direction("90").unwrap(), 90.0));
    }
}
