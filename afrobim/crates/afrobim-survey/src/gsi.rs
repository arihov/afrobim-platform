//! Reader for Leica GSI-8 and GSI-16 measurement files.
//!
//! SOURCE: the format description of the Total Open Station documentation, page "leica_gsi"
//! (https://totalopenstation.readthedocs.io/en/stable/input_formats/if_leica_gsi.html, retrieved
//! 2026-11-16), which lists the word layout, the word indexes and the unit codes; the Autodesk Map 3D
//! help page "Leica GSI formats" (survey file formats) gives example lines of both variants. No Leica
//! document was available, so everything not on those pages is not implemented.
//!
//! LAYOUT. ASCII, one line per record, words separated by blanks. A word is: word index (2 digits for
//! the indexes used here; the page lists a few 3 digit ones that are not read) and four information characters (the last is the unit code), a sign (`+` or `-`), then the data field
//! of 8 characters (GSI-8, word length 15) or 16 (GSI-16, word length 23). A leading `*` on the line
//! (GSI-16 files) is dropped. The data field is an integer without decimal point; the unit code gives
//! the implied decimals:
//!
//! | unit | meaning | decimals |
//! |---|---|---|
//! | 0 | metre | 3 |
//! | 1 | foot | 3 |
//! | 2 | gon | 5 |
//! | 3 | decimal degrees | 5 |
//! | 4 | sexagesimal DDD.MMSSs | 5 |
//! | 5 | mil (6400) | 4 |
//! | 6 | metre | 4 |
//! | 7 | foot | 4 |
//! | 8 | metre | 5 |
//!
//! Feet are converted only with `feet_to_m`, an input (no conversion factor is built in).
//!
//! WORD INDEXES USED: 11 point id (leading zeros removed), 21 horizontal circle reading, 22 vertical
//! angle, 31 slope distance, 32 horizontal distance, 33 height difference, 41 code block, 81 82 83 target
//! E N H, 84 85 86 station E N H, 87 reflector height, 88 instrument height. A line with 84 is a station
//! line (a setup and a control point are created); a line with any of 21 22 31 32 33 is a measurement
//! of a target from the current setup (a circle reading, so the kind is `Direction`); a line with 81 and
//! 82 and no measurement is a point with given coordinates. Word 25 and every other index are ignored
//! and counted in a warning. Instrument-computed coordinates (81 to 83) on a measurement line are
//! ignored (the observations are reduced by this crate instead).
//!
//! LIMITS. A GSI file does not say which target is the backsight, whether a line is face 1 or face 2,
//! or the orientation: `backsight` chooses how setups get a backsight (the first observation of the
//! setup, a named point or none). The vertical angle is a zenith angle unless `vertical_kind` says
//! otherwise. Code blocks (41) are attached to the previous record or the next one as chosen by
//! `code_follows_measurement` (the Autodesk help says the code lines must follow the measurement lines).
//! Word 51 (ppm and prism constant) holds two signed values and is not decoded.

use serde::{Deserialize, Serialize};

use crate::error::{parse_err, Result, SurveyError};
use crate::import::ParsedData;
use crate::model::*;
use crate::units::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GsiBacksight {
    None,
    /// The first observation of each setup is the backsight; its reading is the backsight reading.
    FirstObservation,
    /// The named point is the backsight of every setup that observes it.
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GsiOptions {
    /// Metres per foot, required only if the file contains feet.
    pub feet_to_m: Option<f64>,
    pub vertical_kind: VerticalKind,
    pub code_follows_measurement: bool,
    pub backsight: GsiBacksight,
}

fn strip_zeros(s: &str) -> String {
    let t = s.trim_start_matches('0');
    if t.is_empty() {
        "0".into()
    } else {
        t.to_string()
    }
}

struct Word<'a> {
    wi: &'a str,
    unit: u8,
    neg: bool,
    data: &'a str,
}

fn split_word<'a>(tok: &'a str, line: usize) -> Result<Word<'a>> {
    let n = tok.len();
    if (n != 15 && n != 23) || !tok.is_ascii() {
        return Err(parse_err(line, format!("word '{tok}' is neither GSI-8 (15 characters) nor GSI-16 (23 characters)")));
    }
    let sign = &tok[6..7];
    if sign != "+" && sign != "-" {
        return Err(parse_err(line, format!("word '{tok}': character 7 must be + or -")));
    }
    let unit_c = tok.as_bytes()[5];
    Ok(Word { wi: &tok[0..2], unit: if unit_c.is_ascii_digit() { unit_c - b'0' } else { 255 }, neg: sign == "-", data: &tok[7..] })
}

enum Val {
    Length(f64),
    Angle(f64),
}

fn decode(w: &Word, line: usize, feet: Option<f64>) -> Result<Val> {
    if w.data.is_empty() || !w.data.chars().all(|c| c.is_ascii_digit()) {
        return Err(parse_err(line, format!("word {}: data '{}' is not a number", w.wi, w.data)));
    }
    let digits: f64 = w.data.parse().map_err(|_| parse_err(line, "number too long"))?;
    let int: u128 = w.data.parse().map_err(|_| parse_err(line, "number too long"))?;
    let sgn = if w.neg { -1.0 } else { 1.0 };
    let no_feet = || parse_err(line, format!("word {}: feet need the feet_to_m option", w.wi));
    Ok(match w.unit {
        0 => Val::Length(sgn * digits / 1e3),
        6 => Val::Length(sgn * digits / 1e4),
        8 => Val::Length(sgn * digits / 1e5),
        1 => Val::Length(sgn * digits / 1e3 * feet.ok_or_else(no_feet)?),
        7 => Val::Length(sgn * digits / 1e4 * feet.ok_or_else(no_feet)?),
        2 => Val::Angle(sgn * digits / 1e5 * 0.9),
        3 => Val::Angle(sgn * digits / 1e5),
        5 => Val::Angle(sgn * digits / 1e4 * 360.0 / 6400.0),
        4 => {
            // DDD.MMSSs with five decimals: the last five digits are MM SS s
            let deg = (int / 100_000) as f64;
            let frac = (int % 100_000) as f64;
            let mm = (frac / 1000.0).floor();
            let ss = (frac - mm * 1000.0) / 10.0;
            if mm >= 60.0 || ss >= 60.0 {
                return Err(parse_err(line, format!("word {}: minutes or seconds out of range in '{}'", w.wi, w.data)));
            }
            Val::Angle(sgn * (deg + mm / 60.0 + ss / 3600.0))
        }
        u => return Err(parse_err(line, format!("word {}: unit code {u} is not in the documented table", w.wi))),
    })
}

fn length(v: Val, line: usize, wi: &str) -> Result<f64> {
    match v {
        Val::Length(x) => Ok(x),
        Val::Angle(_) => Err(parse_err(line, format!("word {wi} should be a length but has an angle unit"))),
    }
}
fn angle(v: Val, line: usize, wi: &str) -> Result<f64> {
    match v {
        Val::Angle(x) => Ok(x),
        Val::Length(_) => Err(parse_err(line, format!("word {wi} should be an angle but has a length unit"))),
    }
}

pub fn parse_gsi(text: &str, o: &GsiOptions) -> Result<ParsedData> {
    let mut out = ParsedData::default();
    let mut setup: Option<usize> = None;
    let mut th = 0.0;
    #[derive(Clone, Copy)]
    enum Last {
        None,
        Obs(usize),
        Point(usize),
    }
    let mut last = Last::None;
    let mut pending_code: Option<String> = None;
    let mut ignored: std::collections::BTreeMap<String, usize> = Default::default();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let t = t.strip_prefix('*').unwrap_or(t);
        let mut words = Vec::new();
        for tok in t.split_whitespace() {
            words.push(split_word(tok, line)?);
        }
        let get = |wi: &str| words.iter().find(|w| w.wi == wi);
        for w in &words {
            if !["11", "21", "22", "31", "32", "33", "41", "81", "82", "83", "84", "85", "86", "87", "88"].contains(&w.wi) {
                *ignored.entry(w.wi.to_string()).or_default() += 1;
            }
        }
        let id = get("11").map(|w| strip_zeros(w.data));
        if let Some(w41) = get("41") {
            let code = strip_zeros(w41.data);
            if o.code_follows_measurement {
                match last {
                    Last::Obs(si) => {
                        if let Some(ob) = out.setups[setup.unwrap()].observations.get_mut(si) {
                            ob.description = code;
                        }
                    }
                    Last::Point(pi) => out.points[pi].description = code,
                    Last::None => out.warnings.push(format!("line {line}: code block with nothing before it")),
                }
            } else {
                pending_code = Some(code);
            }
            if get("11").is_none() {
                continue;
            }
        }
        if let Some(w) = get("87") {
            th = length(decode(w, line, o.feet_to_m)?, line, "87")?;
        }
        if get("84").is_some() {
            let name = id.clone().ok_or_else(|| parse_err(line, "station line without word 11 (station id)"))?;
            let e = length(decode(get("84").unwrap(), line, o.feet_to_m)?, line, "84")?;
            let n = match get("85") {
                Some(w) => length(decode(w, line, o.feet_to_m)?, line, "85")?,
                None => return Err(parse_err(line, "station line has word 84 but not 85")),
            };
            let z = match get("86") {
                Some(w) => Some(length(decode(w, line, o.feet_to_m)?, line, "86")?),
                None => None,
            };
            let hi = match get("88") {
                Some(w) => length(decode(w, line, o.feet_to_m)?, line, "88")?,
                None => 0.0,
            };
            out.points.push(SurveyPoint::control(&name, e, n, z));
            out.setups.push(Setup::new(&name, hi));
            setup = Some(out.setups.len() - 1);
            last = Last::None;
            continue;
        }
        let has_meas = ["21", "22", "31", "32", "33"].iter().any(|k| get(k).is_some());
        if has_meas {
            let si = setup.ok_or_else(|| parse_err(line, "measurement line before any station line"))?;
            let target = id.clone().ok_or_else(|| parse_err(line, "measurement line without word 11 (point id)"))?;
            let mut ob = Observation::new(&target);
            ob.source_line = Some(line);
            ob.hz_kind = HzKind::Direction;
            ob.target_height = th;
            if let Some(w) = get("21") {
                ob.hz_deg = Some(angle(decode(w, line, o.feet_to_m)?, line, "21")?);
            }
            if let Some(w) = get("22") {
                ob.zenith_deg = Some(to_zenith(angle(decode(w, line, o.feet_to_m)?, line, "22")?, o.vertical_kind));
            }
            if let Some(w) = get("31") {
                ob.slope_distance = Some(length(decode(w, line, o.feet_to_m)?, line, "31")?);
            }
            if let Some(w) = get("32") {
                ob.horizontal_distance = Some(length(decode(w, line, o.feet_to_m)?, line, "32")?);
            }
            if let Some(w) = get("33") {
                ob.vertical_distance = Some(length(decode(w, line, o.feet_to_m)?, line, "33")?);
            }
            if let Some(c) = pending_code.take() {
                ob.description = c;
            }
            out.setups[si].observations.push(ob);
            last = Last::Obs(out.setups[si].observations.len() - 1);
            continue;
        }
        if get("81").is_some() && get("82").is_some() {
            let name = id.ok_or_else(|| parse_err(line, "coordinate line without word 11"))?;
            let e = length(decode(get("81").unwrap(), line, o.feet_to_m)?, line, "81")?;
            let n = length(decode(get("82").unwrap(), line, o.feet_to_m)?, line, "82")?;
            let z = match get("83") {
                Some(w) => Some(length(decode(w, line, o.feet_to_m)?, line, "83")?),
                None => None,
            };
            let mut p = SurveyPoint::given(&name, e, n, z);
            if let Some(c) = pending_code.take() {
                p.description = c;
            }
            out.points.push(p);
            last = Last::Point(out.points.len() - 1);
            continue;
        }
        if get("41").is_none() {
            out.warnings.push(format!("line {line}: no recognised record (words {})", words.iter().map(|w| w.wi).collect::<Vec<_>>().join(" ")));
        }
    }
    if !ignored.is_empty() {
        out.warnings.push(format!("word indexes ignored (count): {}", ignored.iter().map(|(k, v)| format!("{k} x{v}")).collect::<Vec<_>>().join(", ")));
    }
    for s in out.setups.iter_mut() {
        match &o.backsight {
            GsiBacksight::None => {}
            GsiBacksight::FirstObservation => {
                if let Some(first) = s.observations.first() {
                    s.backsight_point = Some(first.target.clone());
                    s.backsight_reading_deg = first.hz_deg.unwrap_or(0.0);
                }
            }
            GsiBacksight::Named(n) => {
                if let Some(ob) = s.observations.iter().find(|ob| &ob.target == n) {
                    s.backsight_point = Some(n.clone());
                    s.backsight_reading_deg = ob.hz_deg.unwrap_or(0.0);
                }
            }
        }
    }
    if text.trim().is_empty() {
        return Err(SurveyError::Parse { line: 1, message: "empty file".into() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> GsiOptions {
        GsiOptions { feet_to_m: None, vertical_kind: VerticalKind::Zenith, code_follows_measurement: true, backsight: GsiBacksight::FirstObservation }
    }

    #[test]
    fn documented_gsi8_coordinate_example() {
        // Total Open Station documentation sample (GSI8): 81 and 82 in metres with three decimals
        let t = "110001+0000A110 81..00+00005387 82..00-00000992\n110002+0000A111 81..00+00007586 82..00-00003031\n";
        let d = parse_gsi(t, &opts()).unwrap();
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.points[0].name, "A110");
        assert!((d.points[0].e.unwrap() - 5.387).abs() < 1e-12);
        assert!((d.points[0].n.unwrap() + 0.992).abs() < 1e-12);
        assert!((d.points[1].n.unwrap() + 3.031).abs() < 1e-12);
    }

    #[test]
    fn documented_gsi16_angle_example_in_gon() {
        // Total Open Station documentation sample: Hz = 133.84650 gon, V = 53.71500 gon (unit code 2)
        let t = "*110001+000000000PNC0055 21.002+0000000013384650 22.002+0000000005371500\n";
        let mut o = opts();
        o.backsight = GsiBacksight::None;
        // a measurement needs a station first
        let st = "*110000+0000000000000001 84..00+0000000500000000 85..00+0000009960000000 88..00+0000000000001550\n";
        let d = parse_gsi(&format!("{st}{t}"), &o).unwrap();
        let ob = &d.setups[0].observations[0];
        assert_eq!(ob.target, "PNC0055");
        assert!((ob.hz_deg.unwrap() - 133.84650 * 0.9).abs() < 1e-12);
        assert!((ob.zenith_deg.unwrap() - 53.71500 * 0.9).abs() < 1e-12);
        assert!((d.setups[0].instrument_height - 1.55).abs() < 1e-12);
        assert!((d.points[0].n.unwrap() - 9_960_000.0).abs() < 1e-9);
        assert!(matches!(parse_gsi(t, &opts()), Err(SurveyError::Parse { line: 1, .. })));
    }

    #[test]
    fn autodesk_gsi8_measurement_line_and_codes() {
        // Autodesk Map 3D help sample: 110002+00130021 21.102+19723700 22.102+10000000 31..00+00045179 51....+0000+000
        let st = "110001+00000001 84..00+00500000 85..00+00600000 88..00+00001500\n";
        let t = format!("{st}110002+00130021 21.102+19723700 22.102+10000000 31..00+00045179 51....+0000+000\n410003+00000012 42....+00001810 43....+00000013\n");
        let d = parse_gsi(&t, &opts()).unwrap();
        let ob = &d.setups[0].observations[0];
        assert_eq!(ob.target, "130021");
        assert!((ob.hz_deg.unwrap() - 197.237 * 0.9).abs() < 1e-12);
        assert!((ob.zenith_deg.unwrap() - 90.0).abs() < 1e-12);
        assert!((ob.slope_distance.unwrap() - 45.179).abs() < 1e-12);
        assert_eq!(ob.description, "12", "code block after the measurement");
        assert_eq!(d.setups[0].backsight_point.as_deref(), Some("130021"));
        assert!(d.warnings.iter().any(|w| w.contains("51") && w.contains("42")), "{:?}", d.warnings);
    }

    #[test]
    fn units_and_errors() {
        // sexagesimal 133.45123 -> 133 deg 45 min 12.3 s ; feet need a factor
        let st = "110001+00000001 84..00+00000000 85..00+00000000\n";
        let d = parse_gsi(&format!("{st}110002+00000002 21.004+13345123\n"), &opts()).unwrap();
        assert!((d.setups[0].observations[0].hz_deg.unwrap() - (133.0 + 45.0 / 60.0 + 12.3 / 3600.0)).abs() < 1e-12);
        let e = parse_gsi(&format!("{st}110002+00000002 31...1+00001000\n"), &opts());
        assert!(matches!(e, Err(SurveyError::Parse { line: 2, .. })));
        let mut o = opts();
        o.feet_to_m = Some(0.3048);
        let d = parse_gsi(&format!("{st}110002+00000002 31...1+00001000\n"), &o).unwrap();
        assert!((d.setups[0].observations[0].slope_distance.unwrap() - 0.3048).abs() < 1e-12);
        assert!(parse_gsi(&format!("{st}110002+000002 31..00+00001000\n"), &opts()).is_err());
        assert!(parse_gsi(&format!("{st}110002*00000002\n"), &opts()).is_err());
    }
}
