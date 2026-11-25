// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Reader for Topcon GTS-7 raw records, STRUCTURE ONLY.
//!
//! SOURCE: Maptek Vulcan help, "Survey, Appendix C, Data Recorder Interfaces, TOPCON GTS-7"
//! (https://help.maptek.com/vulcan/2026/topics/Appendices/Survey_Appendix_C/Appendices_TOPCON.HTM,
//! retrieved 2026-11-16). That page gives the record types and their field order, and the rules:
//! 1-8 record type, 9 onwards comma separated values;
//! STN ptno, inst ht, stn id; XYZ easting, northing, elevation (follows STN); BKB ptno, backsight
//! bearing, backsight angle; BS ptno [, target height]; FS and SS ptno, target height, pt code
//! [, string number]; CTL control code (optional, follows FS or SS); HV horiz angle, vertical angle;
//! SD horiz angle, vertical angle, slope distance; HD horiz angle, horiz distance, vertical distance
//! (HV, SD and HD follow a BS, FS or SS header); OFFSET radial, tangential, vertical offset;
//! Note comment; header records GTS-700, JOB, DATE, NAME, INST, UNITS, SCALE, ATMOS.
//!
//! WHAT THE PAGE DOES NOT SAY, and therefore is an input or an assumption here:
//! * the angle encoding and the values of the UNITS record (the options give the angle unit,
//!   vertical angle type and distance factor; UNITS, SCALE and ATMOS are read and ignored, with a warning);
//! * that BKB's "backsight angle" is the circle reading on the backsight and its "bearing" the grid
//!   azimuth (assumed from the field names);
//! * the vertical angle type (option);
//! * which of the HD fields is positive up (assumed: vertical distance is the height difference).
//! No real GTS-7 file was available, so this reader is tested only against records built from the
//! page's field list. OFFSET and CTL records are ignored with a warning.
//!
//! The record type is read as the text before the first comma when that is 1 to 8 characters, else
//! as the first 8 characters.

use serde::{Deserialize, Serialize};

use crate::error::{parse_err, Result};
use crate::import::ParsedData;
use crate::model::*;
use crate::units::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopconOptions {
    pub angle_unit: AngleUnit,
    pub vertical_kind: VerticalKind,
    pub distance_factor_m: f64,
}

pub fn parse_topcon(text: &str, o: &TopconOptions) -> Result<ParsedData> {
    let mut out = ParsedData::default();
    let mut setup: Option<usize> = None;
    let mut pending: Option<Observation> = None;
    let mut warned: std::collections::BTreeSet<String> = Default::default();
    fn flush(out: &mut ParsedData, setup: Option<usize>, pending: &mut Option<Observation>, warn: &mut Vec<String>) {
        if let Some(ob) = pending.take() {
            if ob.hz_deg.is_none() && ob.slope_distance.is_none() && ob.horizontal_distance.is_none() {
                warn.push(format!("header of {} has no HV, SD or HD record and was dropped", ob.target));
            } else if let Some(si) = setup {
                out.setups[si].observations.push(ob);
            }
        }
    }
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let (rec, rest) = match raw.find(',') {
            Some(c) if c <= 8 => (raw[..c].trim().to_uppercase(), &raw[c + 1..]),
            _ => {
                let cut = raw.len().min(8);
                (raw[..cut].trim().to_uppercase(), raw.get(cut..).unwrap_or("").trim_start_matches(','))
            }
        };
        let f: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        let num = |k: usize, w: &str| -> Result<f64> {
            f.get(k).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()).ok_or_else(|| parse_err(line, format!("{rec}: cannot read {w}")))
        };
        let dnum = |k: usize, w: &str| -> Result<f64> { Ok(num(k, w)? * o.distance_factor_m) };
        let ang = |k: usize, w: &str| -> Result<f64> {
            let s = f.get(k).ok_or_else(|| parse_err(line, format!("{rec}: missing {w}")))?;
            parse_angle(s, o.angle_unit).map_err(|_| parse_err(line, format!("{rec}: cannot read {w} from '{s}'")))
        };
        let name = |k: usize, w: &str| -> Result<String> {
            let s = f.get(k).copied().unwrap_or("");
            if s.is_empty() {
                Err(parse_err(line, format!("{rec}: missing {w}")))
            } else {
                Ok(s.to_string())
            }
        };
        let mut w = Vec::new();
        match rec.as_str() {
            "STN" => {
                flush(&mut out, setup, &mut pending, &mut w);
                let st = name(0, "point number")?;
                let hi = dnum(1, "instrument height")?;
                out.setups.push(Setup::new(&st, hi));
                setup = Some(out.setups.len() - 1);
            }
            "XYZ" => {
                let si = setup.ok_or_else(|| parse_err(line, "XYZ before any STN"))?;
                let st = out.setups[si].station.clone();
                out.points.push(SurveyPoint::control(&st, dnum(0, "easting")?, dnum(1, "northing")?, Some(dnum(2, "elevation")?)));
            }
            "BKB" => {
                let si = setup.ok_or_else(|| parse_err(line, "BKB before any STN"))?;
                out.setups[si].backsight_point = Some(name(0, "backsight point")?);
                out.setups[si].backsight_azimuth_deg = Some(ang(1, "backsight bearing")?);
                out.setups[si].backsight_reading_deg = ang(2, "backsight angle")?;
            }
            "BS" | "FS" | "SS" => {
                flush(&mut out, setup, &mut pending, &mut w);
                setup.ok_or_else(|| parse_err(line, format!("{rec} before any STN")))?;
                let mut ob = Observation::new(&name(0, "point number")?);
                ob.source_line = Some(line);
                ob.hz_kind = HzKind::Direction;
                if rec == "BS" {
                    if f.get(1).map(|s| !s.is_empty()).unwrap_or(false) {
                        ob.target_height = dnum(1, "target height")?;
                    }
                } else {
                    ob.target_height = dnum(1, "target height")?;
                    ob.description = f.get(2).copied().unwrap_or("").to_string();
                }
                pending = Some(ob);
            }
            "HV" | "SD" | "HD" => {
                let ob = pending.as_mut().ok_or_else(|| parse_err(line, format!("{rec} without a BS, FS or SS header")))?;
                ob.hz_deg = Some(ang(0, "horizontal angle")?);
                match rec.as_str() {
                    "HV" => ob.zenith_deg = Some(to_zenith(ang(1, "vertical angle")?, o.vertical_kind)),
                    "SD" => {
                        ob.zenith_deg = Some(to_zenith(ang(1, "vertical angle")?, o.vertical_kind));
                        ob.slope_distance = Some(dnum(2, "slope distance")?);
                    }
                    _ => {
                        ob.horizontal_distance = Some(dnum(1, "horizontal distance")?);
                        ob.vertical_distance = Some(dnum(2, "vertical distance")?);
                    }
                }
            }
            "CTL" | "OFFSET" | "UNITS" | "SCALE" | "ATMOS" | "GTS-700" | "JOB" | "DATE" | "NAME" | "INST" | "NOTE" => {
                if matches!(rec.as_str(), "CTL" | "OFFSET" | "UNITS" | "SCALE" | "ATMOS") && warned.insert(rec.clone()) {
                    out.warnings.push(format!("{rec} records are read and ignored (first at line {line})"));
                }
            }
            other => return Err(parse_err(line, format!("unknown record type '{other}'"))),
        }
        out.warnings.extend(w);
    }
    let mut w = Vec::new();
    flush(&mut out, setup, &mut pending, &mut w);
    out.warnings.extend(w);
    // a station XYZ makes the station control; backsight points keep what BKB said
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SurveyError;

    fn o() -> TopconOptions {
        TopconOptions { angle_unit: AngleUnit::Degrees, vertical_kind: VerticalKind::Zenith, distance_factor_m: 1.0 }
    }

    #[test]
    fn records_from_the_documented_field_list() {
        let t = "GTS-700,1.0\nJOB,test,x\nUNITS,Metres,Degrees\nSTN,S1,1.55,S1\nXYZ,500000,9960000,100\nBKB,B1,90,0\nBS,B1,1.8\nHV,0.0005,90\nFS,P1,1.8,EP1\nSD,45.5,88.25,60.123\nSS,P2,1.5,TREE\nHD,100.2,30.5,-0.25\nNote,hello\nOFFSET,0,0,0\n";
        let d = parse_topcon(t, &o()).unwrap();
        assert_eq!(d.points[0].name, "S1");
        let s = &d.setups[0];
        assert_eq!(s.backsight_point.as_deref(), Some("B1"));
        assert_eq!(s.backsight_azimuth_deg, Some(90.0));
        assert_eq!(s.observations.len(), 3);
        assert!((s.observations[1].slope_distance.unwrap() - 60.123).abs() < 1e-12);
        assert_eq!(s.observations[1].description, "EP1");
        assert!((s.observations[2].vertical_distance.unwrap() + 0.25).abs() < 1e-12);
        assert!(d.warnings.iter().any(|w| w.contains("UNITS")));
    }

    #[test]
    fn errors_have_line_numbers() {
        assert!(matches!(parse_topcon("SD,1,2,3\n", &o()), Err(SurveyError::Parse { line: 1, .. })));
        assert!(matches!(parse_topcon("STN,S,1.5,S\nZZZ,1\n", &o()), Err(SurveyError::Parse { line: 2, .. })));
        assert!(matches!(parse_topcon("STN,S,abc,S\n", &o()), Err(SurveyError::Parse { line: 1, .. })));
    }
}
