// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Where a sign stands beside an alignment.
//!
//! [`resolve`], [`rotation`], [`normalize_rad`] and [`location_report`] are a port of the C#
//! reference (`SignCore.Cli/SignPlacement.cs`: `SignPlacement.Resolve`, `Rotation`,
//! `NormalizeRad` and `LocationReport.Write`) and are held to it by `tests/golden.rs`: same
//! points, same warning text, same CSV bytes. [`place`] is new: it joins those rules to the
//! alignment geometry of this crate, and [`place_with`] adds the choices that the profile,
//! superelevation and station equations bring: whether the location's station is an internal or a
//! displayed one, and whether the level is adjusted for cross-fall.
//!
//! Conventions (unchanged from the reference):
//!
//! - **Side.** When `side` is set it decides the side: `Left` places the sign at −|offset|,
//!   `Right` at +|offset| and `Both` places two signs, one at each. A signed offset that
//!   disagrees with `side` is overridden and reported. Without `side`, the offset's sign decides;
//!   a zero or missing offset then needs an explicit side.
//! - **Facing.** A sign serves the traffic on its own side of the road: with left-hand traffic a
//!   left-side sign faces traffic travelling up-station and a right-side sign faces traffic
//!   travelling down-station (mirrored for right-hand traffic).
//! - **Rotation.** The sign face is square to the alignment. The sign is turned so that its +Y
//!   axis points in the served traffic's direction of travel and its +X axis (its width) runs
//!   across the road to the driver's right: the face (−Y) looks at the approaching traffic. So
//!   rotation = heading of the served traffic − 90°.

use std::cmp::Ordering;

use afrobim_kernel::SitePlacement;
use serde::{Deserialize, Serialize};

use crate::geometry::Alignment;
use crate::profile::ProfileError;
use crate::stationing::StationEquationError;

/// Which side of the road a sign stands on, looking in the direction of increasing station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignSide {
    Left,
    Right,
    Both,
}

impl SignSide {
    /// The schedule token: `left`, `right` or `both`.
    pub fn token(self) -> &'static str {
        match self {
            SignSide::Left => "left",
            SignSide::Right => "right",
            SignSide::Both => "both",
        }
    }
}

/// Which side of the road traffic keeps to. The UK and Uganda drive on the left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrafficSide {
    #[default]
    Left,
    Right,
}

/// Where a sign stands along an alignment (the schedule columns `alignment`, `station`, `offset`,
/// `side`). Station and offset are in metres; the offset is measured square to the alignment,
/// positive to the right and negative to the left looking up-station. `side`, when set, wins over
/// the offset's sign.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SignLocation {
    #[serde(default)]
    pub alignment: Option<String>,
    #[serde(default)]
    pub station: Option<f64>,
    #[serde(default)]
    pub offset: Option<f64>,
    #[serde(default)]
    pub side: Option<SignSide>,
}

impl SignLocation {
    /// True when the location names an alignment and a station, so it can be placed.
    pub fn is_placeable(&self) -> bool {
        self.alignment.as_deref().is_some_and(|a| !a.trim().is_empty()) && self.station.is_some()
    }
}

/// One sign to insert: the side it stands on, its signed offset (m, + right) and its rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacementPoint {
    pub side: SignSide,
    pub offset: f64,
    pub rotation_rad: f64,
}

impl PlacementPoint {
    pub fn rotation_deg(&self) -> f64 {
        self.rotation_rad * 180.0 / std::f64::consts::PI
    }
}

/// The inserts a location resolves to, plus a note when the offset's sign was overridden.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementPlan {
    pub points: Vec<PlacementPoint>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlacementError {
    #[error("a sign with a zero or missing offset needs side = left, right or both")]
    NeedsSide,
    #[error("rotation needs a single side")]
    RotationNeedsSingleSide,
    #[error("the location has no station")]
    NoStation,
    #[error("station {station} is off alignment '{alignment}', which runs from {start} to {end}")]
    StationOffAlignment { alignment: String, station: String, start: String, end: String },
    #[error("alignment '{alignment}': {source}")]
    Station { alignment: String, source: StationEquationError },
    #[error("alignment '{alignment}': no level for the sign: {source}")]
    Profile { alignment: String, source: ProfileError },
    #[error("alignment '{alignment}' has no profile, so there is no level to adjust for cross-fall")]
    NoProfile { alignment: String },
    #[error("alignment '{alignment}' has no stored cross-fall for the {side} side at station {station}")]
    CrossFallUnknown { alignment: String, station: String, side: &'static str },
}

/// Turns a schedule location into inserts along an alignment whose direction at the station is
/// `tangent_rad` (radians from +X, counter-clockwise). The station itself is not used here.
pub fn resolve(location: &SignLocation, tangent_rad: f64, drive: TrafficSide) -> Result<PlacementPlan, PlacementError> {
    let off = location.offset.unwrap_or(0.0);
    let mag = off.abs();
    let mut warning = None;

    let side = match location.side {
        Some(s) => {
            if (s == SignSide::Left && off > 0.0) || (s == SignSide::Right && off < 0.0) {
                warning = Some(format!(
                    "offset {} m is on the other side from side={}; placed at {}{} m",
                    dotnet_fixed(off, 0, 3),
                    s.token(),
                    if s == SignSide::Left { "-" } else { "+" },
                    dotnet_fixed(mag, 0, 3)
                ));
            }
            s
        }
        None if off < 0.0 => SignSide::Left,
        None if off > 0.0 => SignSide::Right,
        None => return Err(PlacementError::NeedsSide),
    };

    let mut points = Vec::with_capacity(2);
    if matches!(side, SignSide::Left | SignSide::Both) {
        points.push(PlacementPoint { side: SignSide::Left, offset: -mag, rotation_rad: rotation(SignSide::Left, tangent_rad, drive)? });
    }
    if matches!(side, SignSide::Right | SignSide::Both) {
        points.push(PlacementPoint { side: SignSide::Right, offset: mag, rotation_rad: rotation(SignSide::Right, tangent_rad, drive)? });
    }
    Ok(PlacementPlan { points, warning })
}

/// Rotation (radians, 0..2π) of a sign on `side` (left or right).
pub fn rotation(side: SignSide, tangent_rad: f64, drive: TrafficSide) -> Result<f64, PlacementError> {
    if side == SignSide::Both {
        return Err(PlacementError::RotationNeedsSingleSide);
    }
    // The traffic a sign serves travels up-station when the sign is on the side traffic keeps to.
    let up_station = (side == SignSide::Left) == (drive == TrafficSide::Left);
    let heading = if up_station { tangent_rad } else { tangent_rad + std::f64::consts::PI };
    Ok(normalize_rad(heading - std::f64::consts::PI / 2.0))
}

/// Brings an angle into 0..2π.
pub fn normalize_rad(a: f64) -> f64 {
    let mut r = a % (2.0 * std::f64::consts::PI);
    if r < 0.0 {
        r += 2.0 * std::f64::consts::PI;
    }
    r
}

/// One sign on site, ready to store: position in millimetres, rotation in degrees
/// counter-clockwise from +X, and the station and offset (m) it was taken from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedPosition {
    pub side: SignSide,
    pub x_mm: f64,
    pub y_mm: f64,
    /// Level in millimetres: the profile's centreline level at the station, plus the cross-fall
    /// adjustment when one was asked for and applied. 0 when the alignment has no profile.
    pub z_mm: f64,
    pub rotation_deg: f64,
    /// The **internal** station, whatever kind of station the location gave.
    pub station_m: f64,
    pub offset_m: f64,
    /// The cross-fall (percent, positive rising away from the centreline) that `z_mm` was
    /// adjusted by; `None` when no adjustment was made.
    pub cross_fall_pct: Option<f64>,
}

impl PlacedPosition {
    /// The kernel placement for this position, tied to the named alignment. `z` is `z_mm` (0
    /// without a profile) and `station_m` is the internal station.
    pub fn site_placement(&self, alignment_name: &str) -> SitePlacement {
        SitePlacement {
            x: self.x_mm,
            y: self.y_mm,
            z: self.z_mm,
            rotation_deg: self.rotation_deg,
            alignment: Some(alignment_name.to_string()),
            station_m: Some(self.station_m),
            offset_m: Some(self.offset_m),
        }
    }

    /// The row of the location report for this position (metres).
    pub fn placed_sign(&self, code: &str, alignment_name: &str) -> PlacedSign {
        PlacedSign {
            code: code.to_string(),
            alignment: alignment_name.to_string(),
            station: self.station_m,
            offset: self.offset_m,
            side: self.side,
            x: self.x_mm / 1000.0,
            y: self.y_mm / 1000.0,
            rotation_deg: self.rotation_deg,
        }
    }
}

/// What [`place`] returns: one position per sign (two for `side = both`, left first) and the
/// override warning of [`resolve`], if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub positions: Vec<PlacedPosition>,
    pub warning: Option<String>,
}

/// What kind of station a [`SignLocation`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StationReference {
    /// The alignment's continuous station (start station + distance along it).
    #[default]
    Internal,
    /// The station as displayed, to be taken through the alignment's station equations.
    /// `region` says which region it is in (0 before the first equation); with `None` the
    /// station must occur exactly once on the alignment, and one in an overlap is an error that
    /// lists the candidates. On an alignment without equations this is the internal station.
    Displayed { region: Option<usize> },
}

/// Whether the level of a placed sign is adjusted for the stored cross-fall.
///
/// The adjustment is `|offset| × cross-fall / 100`, added to the centreline level: it treats the
/// ground as one plane from the centreline to the sign at the cross-fall of that side.
/// **That is an approximation and holds only within the carriageway.** Beyond the carriageway
/// edge (shoulder, verge, side slope, where signs usually stand) the ground does not follow the
/// carriageway cross-fall and the result is not a ground level. It is off unless asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossFallAdjust {
    /// The centreline level is used as it is.
    #[default]
    Off,
    /// Adjust where the alignment has a cross-fall for that side at that station; elsewhere the
    /// centreline level is kept and [`PlacedPosition::cross_fall_pct`] is `None`.
    WhereKnown,
    /// Adjust, and fail where there is no profile or no stored cross-fall for that side there.
    Required,
}

/// The choices of [`place_with`]. The default is what [`place`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlaceOptions {
    pub station: StationReference,
    pub cross_fall: CrossFallAdjust,
}

/// Places a schedule location on an alignment: the point at its station, moved square to the
/// alignment by the resolved offset, with the rotation of [`resolve`] for the alignment's
/// direction there. The location's `alignment` name is not compared with the alignment's.
///
/// The station is an **internal** station. When the alignment has a profile the level is the
/// profile's (centreline) level at the station, and a station outside the profile is an error;
/// without a profile the level is 0. No cross-fall adjustment is made. See [`place_with`] for
/// displayed stations and the cross-fall option.
pub fn place(alignment: &Alignment, location: &SignLocation, drive: TrafficSide) -> Result<Placement, PlacementError> {
    place_with(alignment, location, drive, &PlaceOptions::default())
}

/// [`place`] with choices: `options.station` says whether the location's station is internal or
/// displayed (see [`StationReference`]), and `options.cross_fall` whether the level is adjusted
/// across the offset by the stored cross-fall (see [`CrossFallAdjust`] for why that is only an
/// approximation).
pub fn place_with(alignment: &Alignment, location: &SignLocation, drive: TrafficSide, options: &PlaceOptions) -> Result<Placement, PlacementError> {
    let name = || alignment.name.clone();
    let given = location.station.ok_or(PlacementError::NoStation)?;
    let station = match options.station {
        StationReference::Internal => given,
        StationReference::Displayed { region } => {
            alignment.internal_station(given, region).map_err(|source| PlacementError::Station { alignment: name(), source })?
        }
    };
    let at = alignment.point_at(station).ok_or_else(|| PlacementError::StationOffAlignment {
        alignment: name(),
        station: crate::format_station(station, 3),
        start: crate::format_station(alignment.start_station_m, 3),
        end: crate::format_station(alignment.end_station_m(), 3),
    })?;
    let plan = resolve(location, at.heading_rad, drive)?;
    let centreline_m = match &alignment.profile {
        Some(profile) => Some(profile.elevation_at(station).map_err(|source| PlacementError::Profile { alignment: name(), source })?),
        None => None,
    };
    if centreline_m.is_none() && options.cross_fall == CrossFallAdjust::Required {
        return Err(PlacementError::NoProfile { alignment: name() });
    }
    let fall = match (options.cross_fall, centreline_m, &alignment.superelevation) {
        (CrossFallAdjust::Off, _, _) | (_, None, _) | (_, _, None) => Default::default(),
        (_, Some(_), Some(s)) => s.cross_fall_at(station),
    };
    let mut positions = Vec::with_capacity(plan.points.len());
    for p in &plan.points {
        let (x, y) = at.offset(p.offset);
        let cross_fall_pct = if p.side == SignSide::Left { fall.left_pct } else { fall.right_pct };
        if cross_fall_pct.is_none() && options.cross_fall == CrossFallAdjust::Required {
            return Err(PlacementError::CrossFallUnknown { alignment: name(), station: alignment.station_label(station, 3), side: p.side.token() });
        }
        let z_m = centreline_m.unwrap_or(0.0) + cross_fall_pct.map_or(0.0, |pct| p.offset.abs() * pct / 100.0);
        positions.push(PlacedPosition {
            side: p.side,
            x_mm: x * 1000.0,
            y_mm: y * 1000.0,
            z_mm: z_m * 1000.0,
            rotation_deg: p.rotation_deg(),
            station_m: station,
            offset_m: p.offset,
            cross_fall_pct,
        });
    }
    Ok(Placement { positions, warning: plan.warning })
}

/// One placed sign for the sign location schedule. Station, offset, x and y are in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedSign {
    pub code: String,
    pub alignment: String,
    pub station: f64,
    pub offset: f64,
    pub side: SignSide,
    pub x: f64,
    pub y: f64,
    pub rotation_deg: f64,
}

/// The sign location schedule CSV.
pub mod location_report {
    use super::*;

    pub const HEADER: &str = "code,alignment,station,offset,side,x,y,rotation";

    /// Rows sorted by alignment (ignoring case) then station then offset, rows with equal keys in
    /// the order given; station, offset, x and y to 0.001, rotation in degrees to 0.0001; lines
    /// end with a line feed.
    pub fn write(signs: &[PlacedSign]) -> String {
        let mut rows: Vec<&PlacedSign> = signs.iter().collect();
        rows.sort_by(|a, b| {
            ordinal_ignore_case(&a.alignment, &b.alignment).then(compare_f64(a.station, b.station)).then(compare_f64(a.offset, b.offset))
        });
        let mut out = String::from(HEADER);
        out.push('\n');
        for s in rows {
            let cells = [
                quote(&s.code),
                quote(&s.alignment),
                dotnet_fixed(s.station, 3, 3),
                dotnet_fixed(s.offset, 3, 3),
                s.side.token().to_string(),
                dotnet_fixed(s.x, 3, 3),
                dotnet_fixed(s.y, 3, 3),
                dotnet_fixed(s.rotation_deg, 4, 4),
            ];
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        out
    }

    fn quote(v: &str) -> String {
        if v.contains([',', '"', '\n']) {
            format!("\"{}\"", v.replace('"', "\"\""))
        } else {
            v.to_string()
        }
    }
}

/// .NET `StringComparer.OrdinalIgnoreCase`: UTF-16 code units compared after mapping each
/// character to its single-character upper case (so `ß` stays `ß`).
fn ordinal_ignore_case(a: &str, b: &str) -> Ordering {
    fn key(s: &str) -> Vec<u16> {
        let mut out = Vec::with_capacity(s.len());
        let mut buf = [0u16; 2];
        for c in s.chars() {
            let mut upper = c.to_uppercase();
            let c = match (upper.next(), upper.next()) {
                (Some(u), None) => u,
                _ => c,
            };
            out.extend_from_slice(c.encode_utf16(&mut buf));
        }
        out
    }
    key(a).cmp(&key(b))
}

/// .NET `double.CompareTo`: NaN sorts before every number and equals itself.
fn compare_f64(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or_else(|| b.is_nan().cmp(&a.is_nan()))
}

/// A number as .NET writes it with a custom format of the shape `0.000` (`min_decimals` =
/// `max_decimals` = 3) or `0.###` (0 and 3) in the invariant culture.
///
/// .NET does not format the exact binary value: it first takes the 15 significant decimal digits
/// of the number and then rounds those half away from zero to the decimals asked for. So 2.0005
/// (stored as 2.000499999…) reads `2.001`, and digits past the fifteenth are zeros. A negative
/// number that rounds to zero keeps its sign (`-0.000`).
pub(crate) fn dotnet_fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    // Fifteen significant digits and the position of the decimal point after the first `point` of them.
    let (mut digits, mut point): (Vec<u8>, i64) = if v == 0.0 {
        (vec![0; 15], 1)
    } else {
        let text = format!("{:.14e}", v.abs());
        let (mantissa, exponent) = text.split_once('e').expect("exponent form");
        (mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect(), exponent.parse::<i64>().expect("exponent") + 1)
    };
    // Round half up to `max_decimals` places.
    let keep = point + max_decimals as i64;
    if keep < 0 {
        // Too small to leave a digit.
        digits = vec![0];
        point = 1;
    } else if (keep as usize) < digits.len() {
        let round_up = digits[keep as usize] >= 5;
        digits.truncate(keep as usize);
        if round_up {
            let mut i = digits.len();
            loop {
                if i == 0 {
                    digits.insert(0, 1);
                    point += 1;
                    break;
                }
                i -= 1;
                if digits[i] == 9 {
                    digits[i] = 0;
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
    }
    let digit = |i: i64| -> u8 {
        if i >= 0 && (i as usize) < digits.len() {
            digits[i as usize]
        } else {
            0
        }
    };
    let mut out = String::new();
    if v.is_sign_negative() {
        out.push('-');
    }
    if point <= 0 {
        out.push('0');
    } else {
        // Leading zeros can only come from the all-zero case.
        let whole: String = (0..point).map(|i| (b'0' + digit(i)) as char).collect();
        let trimmed = whole.trim_start_matches('0');
        out.push_str(if trimmed.is_empty() { "0" } else { trimmed });
    }
    let mut frac: Vec<u8> = (0..max_decimals as i64).map(|i| digit(point + i)).collect();
    while frac.len() > min_decimals && frac.last() == Some(&0) {
        frac.pop();
    }
    if !frac.is_empty() {
        out.push('.');
        out.extend(frac.iter().map(|d| (b'0' + d) as char));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_formatting_follows_dotnet() {
        assert_eq!(dotnet_fixed(0.0005, 3, 3), "0.001");
        assert_eq!(dotnet_fixed(2.0005, 3, 3), "2.001");
        assert_eq!(dotnet_fixed(-0.0004, 3, 3), "-0.000");
        assert_eq!(dotnet_fixed(-0.0004, 0, 3), "-0");
        assert_eq!(dotnet_fixed(2.5, 0, 3), "2.5");
        assert_eq!(dotnet_fixed(999.9996, 0, 3), "1000");
        assert_eq!(dotnet_fixed(359.99995, 4, 4), "360.0000");
        assert_eq!(dotnet_fixed(1e-300, 3, 3), "0.000");
        assert_eq!(dotnet_fixed(9007199254740993.0, 3, 3), "9007199254740990.000");
    }
}
