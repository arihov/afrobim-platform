//! Delimited observation tables with a column mapping.
//!
//! One row is one observation. A new setup starts when the station value changes. The mapping names
//! the column index (from 0) of each field, or resolves names from the header row with
//! `CsvMapping::from_header`. Angles are in `angle_unit`, vertical angles of type `vertical_kind`,
//! distances in units of `distance_factor_m` metres. Horizontal values are circle readings
//! (`Direction`), as in a total station download. A field that is mapped but empty in a row is absent
//! for that row. Fields in double quotes may contain the delimiter; a doubled quote inside is one quote.

use serde::{Deserialize, Serialize};

use crate::error::{parse_err, Result, SurveyError};
use crate::import::ParsedData;
use crate::model::*;
use crate::units::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvRole {
    Station,
    InstrumentHeight,
    Backsight,
    BacksightReading,
    Target,
    Hz,
    Vertical,
    SlopeDistance,
    HorizontalDistance,
    VerticalDistance,
    TargetHeight,
    Description,
    /// 1 or 2.
    Face,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsvColumn {
    pub role: CsvRole,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsvMapping {
    pub delimiter: char,
    pub has_header: bool,
    pub columns: Vec<CsvColumn>,
    pub angle_unit: AngleUnit,
    pub vertical_kind: VerticalKind,
    pub distance_factor_m: f64,
}

impl CsvMapping {
    /// Map roles to the header names of the first line (case insensitive, trimmed). Every name must be
    /// found. `has_header` is true.
    pub fn from_header(header_line: &str, delimiter: char, names: &[(CsvRole, &str)], angle_unit: AngleUnit, vertical_kind: VerticalKind, distance_factor_m: f64) -> Result<CsvMapping> {
        let h = split_row(header_line, delimiter);
        let mut columns = Vec::new();
        for (role, name) in names {
            let i = h.iter().position(|c| c.trim().eq_ignore_ascii_case(name)).ok_or_else(|| parse_err(1, format!("header has no column '{name}'")))?;
            columns.push(CsvColumn { role: *role, index: i });
        }
        Ok(CsvMapping { delimiter, has_header: true, columns, angle_unit, vertical_kind, distance_factor_m })
    }
}

fn split_row(line: &str, d: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if c == '"' {
            if q && it.peek() == Some(&'"') {
                cur.push('"');
                it.next();
            } else {
                q = !q;
            }
        } else if c == d && !q {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

pub fn parse_csv(text: &str, m: &CsvMapping) -> Result<ParsedData> {
    if !(m.distance_factor_m > 0.0) {
        return Err(SurveyError::Invalid("distance_factor_m must be positive".into()));
    }
    let col = |r: CsvRole| m.columns.iter().find(|c| c.role == r).map(|c| c.index);
    let station_col = col(CsvRole::Station).ok_or_else(|| SurveyError::Invalid("the mapping needs a Station column".into()))?;
    let target_col = col(CsvRole::Target).ok_or_else(|| SurveyError::Invalid("the mapping needs a Target column".into()))?;
    let mut out = ParsedData::default();
    let mut cur: Option<(String, String)> = None; // station, backsight
    let mut warned_hi = false;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        if raw.trim().is_empty() || (i == 0 && m.has_header) {
            continue;
        }
        let f = split_row(raw, m.delimiter);
        let get = |r: CsvRole| -> Option<String> { col(r).and_then(|k| f.get(k)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) };
        let station = f.get(station_col).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).ok_or_else(|| parse_err(line, "empty station"))?;
        let target = f.get(target_col).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).ok_or_else(|| parse_err(line, "empty target"))?;
        let bs = get(CsvRole::Backsight).unwrap_or_default();
        let num = |r: CsvRole, scale: f64| -> Result<Option<f64>> {
            match get(r) {
                None => Ok(None),
                Some(s) => s.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| Some(v * scale)).ok_or_else(|| parse_err(line, format!("cannot read number '{s}' ({r:?})"))),
            }
        };
        let angle = |r: CsvRole| -> Result<Option<f64>> {
            match get(r) {
                None => Ok(None),
                Some(s) => parse_angle(&s, m.angle_unit).map(Some).map_err(|_| parse_err(line, format!("cannot read angle '{s}' ({r:?})"))),
            }
        };
        if cur.as_ref().map(|c| (&c.0, &c.1)) != Some((&station, &bs)) {
            let hi = match num(CsvRole::InstrumentHeight, m.distance_factor_m)? {
                Some(h) => h,
                None => {
                    if !warned_hi {
                        out.warnings.push(format!("line {line}: no instrument height in the table: 0 assumed for setups without one"));
                        warned_hi = true;
                    }
                    0.0
                }
            };
            let mut s = Setup::new(&station, hi);
            if !bs.is_empty() {
                s.backsight_point = Some(bs.clone());
            }
            if let Some(r) = angle(CsvRole::BacksightReading)? {
                s.backsight_reading_deg = r;
            }
            out.setups.push(s);
            cur = Some((station.clone(), bs));
        }
        let mut ob = Observation::new(&target);
        ob.source_line = Some(line);
        ob.hz_kind = HzKind::Direction;
        ob.hz_deg = angle(CsvRole::Hz)?;
        if let Some(v) = angle(CsvRole::Vertical)? {
            ob.zenith_deg = Some(to_zenith(v, m.vertical_kind));
        }
        ob.slope_distance = num(CsvRole::SlopeDistance, m.distance_factor_m)?;
        ob.horizontal_distance = num(CsvRole::HorizontalDistance, m.distance_factor_m)?;
        ob.vertical_distance = num(CsvRole::VerticalDistance, m.distance_factor_m)?;
        ob.target_height = num(CsvRole::TargetHeight, m.distance_factor_m)?.unwrap_or(0.0);
        ob.description = get(CsvRole::Description).unwrap_or_default();
        match get(CsvRole::Face).as_deref() {
            None => {}
            Some("1") => ob.face = Face::Face1,
            Some("2") => ob.face = Face::Face2,
            Some(o) => return Err(parse_err(line, format!("face must be 1 or 2, got '{o}'"))),
        }
        out.setups.last_mut().unwrap().observations.push(ob);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_by_header_names_and_rows() {
        let t = "From;BS;Hz (gon);Zen;SD;Dist;th;Code;HI\nS1;B1;0;100;;;1.8;\"EP1; B\";1.55\nS1;B1;50;99.5;45.0;;1.8;EP1;1.55\nS2;S1;0;100;30.0;;1.5;;1.6\n";
        let m = CsvMapping::from_header(
            t.lines().next().unwrap(),
            ';',
            &[
                (CsvRole::Station, "From"),
                (CsvRole::Backsight, "BS"),
                (CsvRole::Target, "Code"),
                (CsvRole::Hz, "Hz (gon)"),
                (CsvRole::Vertical, "Zen"),
                (CsvRole::SlopeDistance, "SD"),
                (CsvRole::TargetHeight, "th"),
                (CsvRole::InstrumentHeight, "HI"),
            ],
            AngleUnit::Gon,
            VerticalKind::Zenith,
            1.0,
        )
        .unwrap();
        // the Target column is "Code" only to test mapping; row 3 has an empty target
        let e = parse_csv(t, &m).unwrap_err();
        assert!(matches!(e, SurveyError::Parse { line: 4, .. }), "{e:?}");
        let d = parse_csv(&t.replace(";;1.5;;1.6", ";;1.5;X;1.6"), &m).unwrap();
        assert_eq!(d.setups.len(), 2);
        assert_eq!(d.setups[0].observations.len(), 2);
        assert_eq!(d.setups[0].observations[0].target, "EP1; B");
        assert!((d.setups[0].observations[1].hz_deg.unwrap() - 45.0).abs() < 1e-12);
        assert!((d.setups[0].instrument_height - 1.55).abs() < 1e-12);
        assert!((d.setups[1].observations[0].slope_distance.unwrap() - 30.0).abs() < 1e-12);
        assert!(CsvMapping::from_header("a;b", ';', &[(CsvRole::Station, "zz")], AngleUnit::Degrees, VerticalKind::Zenith, 1.0).is_err());
    }
}
