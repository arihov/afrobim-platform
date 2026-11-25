// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Reader for a subset of the Autodesk Civil 3D field book (.fbk) survey command language.
//!
//! SOURCES (Autodesk Civil 3D 2027 help, retrieved 2026-11-16 through the Autodesk Product Help
//! search; page titles): "About Point Creation Commands", "About Point Location Commands",
//! "To Create a Survey Observation by Angle and Distance", "To Create a Survey Observation Using a
//! Face1 or Face2 Angle", "To Create a New Survey Instrument Setup", "About Figure Commands",
//! "To Continue a Survey Figure", "To End a Survey Figure", "About Field Book Files" and "Is there a
//! file specification document available for the field book .FBK file format in Civil 3D?" (the
//! answer of which is: no specification document, only the help pages).
//!
//! IMPLEMENTED (everything below is as written on those pages):
//!   NE [SS] name n e [elev] [desc]   control point (NE), non control point (NE SS)
//!   NEZ name n e z [desc]            control point with elevation
//!   AZ p1 p2 azimuth                 known azimuth from p1 to p2
//!   STN|STA name [instrument height] [desc]    new setup
//!   BS name [orientation]            backsight of the current setup; orientation is the circle
//!                                    reading on it (angle observations are readings: azimuth =
//!                                    backsight azimuth + (angle - orientation))
//!   PRISM h                          target height for the following observations
//!   AD [VA|VD] pt angle dist [vertical] [desc]
//!   F1|F2 [VA|VD] pt angle [dist [vertical]] [desc]
//!   BEG name, CONT name, PT id, CLOSE, END   figures; while a figure is active every point created by
//!                                    NE, NEZ, AD, F1, F2 and PT becomes a vertex (the draw mode of the
//!                                    help examples)
//! with the meaning of the parameters given on the pages: the angle is clockwise from the backsight,
//! with VA the distance is a slope distance and the next value is the vertical (zenith) angle, with VD
//! the distance is horizontal and the next value a height difference from the instrument to the prism,
//! without either the distance is horizontal. Lines starting with `!` are comments (the help shows
//! `!` for echoed output). Descriptions may be quoted; a description that looks like a number must be
//! quoted.
//!
//! NOT IMPLEMENTED: every other command (ZD, BD, DD, STADIA, PT OFFSET, CL ROD, HI, AP ON, SKIP, LAT LONG,
//! CRV, XC, FIG, offsets, curves, ...). In strict mode they are errors with the line number, otherwise
//! warnings. The help pages give no unit rules for a file: the angle unit and vertical angle type of
//! the survey database are options here. Because no specification and no real file could be obtained,
//! the reader is checked against the examples printed on the help pages and nothing else; treat
//! it as a reader for simple field books, not as a complete FBK reader. The tutorial file excerpt
//! `FC1 VA 105 19.192302 57.714 88.440647 "TC1"` printed on the help page is not recognised (the
//! command `FC1` is not documented anywhere that could be fetched).

use serde::{Deserialize, Serialize};

use crate::error::{parse_err, Result};
use crate::figures::{Figure, FigureVertex, VertexKind};
use crate::import::ParsedData;
use crate::model::*;
use crate::units::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FbkOptions {
    pub angle_unit: AngleUnit,
    pub vertical_kind: VerticalKind,
    /// Metres per distance unit of the file (1 for metres).
    pub distance_factor_m: f64,
    /// Unknown or unsupported commands are errors (true) or warnings (false).
    pub strict: bool,
}

fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut had = false;
    for c in line.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                had = true;
            }
            c if c.is_whitespace() && !in_q => {
                if !cur.is_empty() || had {
                    out.push(std::mem::take(&mut cur));
                    had = false;
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() || had {
        out.push(cur);
    }
    out
}

pub fn parse_fbk(text: &str, o: &FbkOptions) -> Result<ParsedData> {
    if !(o.distance_factor_m > 0.0) {
        return Err(crate::error::SurveyError::Invalid("distance_factor_m must be positive".into()));
    }
    let mut out = ParsedData::default();
    let mut setup: Option<usize> = None;
    let mut prism = 0.0;
    let mut active: Option<usize> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let t = raw.trim();
        if t.is_empty() || t.starts_with('!') {
            continue;
        }
        let tok = tokenize(t);
        let cmd = tok[0].to_uppercase();
        let num = |s: &str, w: &str| -> Result<f64> { s.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| parse_err(line, format!("{cmd}: cannot read {w} from '{s}'"))) };
        let dnum = |s: &str, w: &str| -> Result<f64> { Ok(num(s, w)? * o.distance_factor_m) };
        let ang = |s: &str, w: &str| -> Result<f64> { parse_angle(s, o.angle_unit).map_err(|_| parse_err(line, format!("{cmd}: cannot read {w} from '{s}'"))) };
        let need = |k: usize, w: &str| -> Result<&String> { tok.get(k).ok_or_else(|| parse_err(line, format!("{cmd}: missing {w}"))) };
        let add_vertex = |out: &mut ParsedData, active: Option<usize>, name: &str| {
            if let Some(f) = active {
                out.figures[f].vertices.push(FigureVertex { point: name.to_string(), kind: VertexKind::Line });
            }
        };
        match cmd.as_str() {
            "NE" | "NEZ" => {
                let ss = cmd == "NE" && tok.get(1).map(|s| s.eq_ignore_ascii_case("SS")).unwrap_or(false);
                let b = if ss { 2 } else { 1 };
                let name = need(b, "point")?.clone();
                let n = dnum(need(b + 1, "northing")?, "northing")?;
                let e = dnum(need(b + 2, "easting")?, "easting")?;
                let mut k = b + 3;
                let mut z = None;
                if cmd == "NEZ" {
                    z = Some(dnum(need(k, "elevation")?, "elevation")?);
                    k += 1;
                } else if ss {
                    if let Some(s) = tok.get(k) {
                        if let Ok(v) = s.parse::<f64>() {
                            z = Some(v * o.distance_factor_m);
                            k += 1;
                        }
                    }
                }
                let desc = tok[k.min(tok.len())..].join(" ");
                let mut p = if ss { SurveyPoint::given(&name, e, n, z) } else { SurveyPoint::control(&name, e, n, z) };
                p.description = desc;
                out.points.push(p);
                add_vertex(&mut out, active, &name);
            }
            "AZ" => {
                let from = need(1, "first point")?.clone();
                let to = need(2, "second point")?.clone();
                let az = ang(need(3, "azimuth")?, "azimuth")?;
                out.directions.push(KnownDirection { from, to, azimuth_deg: az, sigma_arcsec: None, event: None });
            }
            "STN" | "STA" => {
                let name = need(1, "station point")?.clone();
                let hi = match tok.get(2) {
                    Some(s) => match s.parse::<f64>() {
                        Ok(v) => v * o.distance_factor_m,
                        Err(_) => 0.0,
                    },
                    None => {
                        out.warnings.push(format!("line {line}: {cmd} without instrument height: 0 assumed"));
                        0.0
                    }
                };
                out.setups.push(Setup::new(&name, hi));
                setup = Some(out.setups.len() - 1);
            }
            "BS" => {
                let si = setup.ok_or_else(|| parse_err(line, "BS before any STN"))?;
                let p = need(1, "backsight point")?.clone();
                out.setups[si].backsight_point = Some(p);
                if let Some(s) = tok.get(2) {
                    out.setups[si].backsight_reading_deg = ang(s, "orientation")?;
                }
            }
            "PRISM" => {
                prism = dnum(need(1, "height")?, "height")?;
            }
            "AD" | "F1" | "F2" => {
                let si = setup.ok_or_else(|| parse_err(line, format!("{cmd} before any STN")))?;
                let mut k = 1;
                let mode = match tok.get(1).map(|s| s.to_uppercase()).as_deref() {
                    Some("VA") => {
                        k = 2;
                        Some("VA")
                    }
                    Some("VD") => {
                        k = 2;
                        Some("VD")
                    }
                    _ => None,
                };
                let name = need(k, "point")?.clone();
                let angle = ang(need(k + 1, "angle")?, "angle")?;
                let mut ob = Observation::new(&name);
                ob.source_line = Some(line);
                ob.hz_kind = HzKind::Direction;
                ob.hz_deg = Some(angle);
                ob.target_height = prism;
                ob.face = match cmd.as_str() {
                    "F1" => Face::Face1,
                    "F2" => Face::Face2,
                    _ => Face::Single,
                };
                let mut next = k + 2;
                let has_dist = tok.get(next).map(|s| s.parse::<f64>().is_ok()).unwrap_or(false);
                if !has_dist && cmd == "AD" {
                    return Err(parse_err(line, "AD: missing distance"));
                }
                if has_dist {
                    let d = dnum(&tok[next], "distance")?;
                    next += 1;
                    match mode {
                        Some("VA") => {
                            let v = ang(need(next, "vertical angle")?, "vertical angle")?;
                            next += 1;
                            ob.slope_distance = Some(d);
                            ob.zenith_deg = Some(to_zenith(v, o.vertical_kind));
                        }
                        Some("VD") => {
                            let v = dnum(need(next, "vertical distance")?, "vertical distance")?;
                            next += 1;
                            ob.horizontal_distance = Some(d);
                            ob.vertical_distance = Some(v);
                        }
                        _ => ob.horizontal_distance = Some(d),
                    }
                }
                ob.description = tok[next.min(tok.len())..].join(" ");
                out.setups[si].observations.push(ob);
                add_vertex(&mut out, active, &name);
            }
            "BEG" | "BEGIN" => {
                let name = need(1, "figure name")?.clone();
                out.figures.push(Figure { name, vertices: Vec::new(), closed: false, breakline: false, style: String::new(), layer: String::new(), from_linework: false, event: None });
                active = Some(out.figures.len() - 1);
            }
            "CONT" | "CONTINUE" => {
                let name = need(1, "figure name")?;
                match out.figures.iter().rposition(|f| &f.name == name) {
                    Some(f) => active = Some(f),
                    None => return Err(parse_err(line, format!("CONT: figure '{name}' has not been started in this file"))),
                }
            }
            "PT" if tok.get(1).map(|s| !s.eq_ignore_ascii_case("OFFSET")).unwrap_or(false) => {
                if active.is_none() {
                    return Err(parse_err(line, "PT outside a figure (no BEG or CONT)"));
                }
                let p = tok[1].clone();
                add_vertex(&mut out, active, &p);
            }
            "CLOSE" if tok.len() == 1 => {
                let f = active.ok_or_else(|| parse_err(line, "CLOSE outside a figure"))?;
                out.figures[f].closed = true;
                active = None;
            }
            "END" | "E" if tok.len() == 1 => {
                if active.is_none() {
                    out.warnings.push(format!("line {line}: END with no active figure"));
                }
                active = None;
            }
            _ => {
                let msg = format!("command '{}' is not supported by this reader", tok.join(" "));
                if o.strict {
                    return Err(parse_err(line, msg));
                }
                out.warnings.push(format!("line {line}: {msg}"));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SurveyError;

    fn opts(strict: bool) -> FbkOptions {
        FbkOptions { angle_unit: AngleUnit::Gon, vertical_kind: VerticalKind::Zenith, distance_factor_m: 1.0, strict }
    }

    #[test]
    fn help_page_example_face1() {
        // from "To Create a Survey Observation Using a Face1 or Face2 Angle"
        let t = "NE 2 200 200\nAZ 2 1 100\nSTN 2 1.585\nBS 1\nF1 VA 101 0 30.480 100 BS\n! AZIMUTH: 101.6126 DISTANCE: 30.4800\nF1 VA 102 89.5370 15.304 101.2052 IP\n";
        let d = parse_fbk(t, &opts(true)).unwrap();
        assert_eq!(d.points[0].name, "2");
        assert!(d.points[0].control);
        assert_eq!(d.points[0].n, Some(200.0));
        assert_eq!(d.directions[0].azimuth_deg, 90.0);
        let s = &d.setups[0];
        assert_eq!(s.station, "2");
        assert!((s.instrument_height - 1.585).abs() < 1e-12);
        assert_eq!(s.backsight_point.as_deref(), Some("1"));
        assert_eq!(s.observations.len(), 2);
        assert_eq!(s.observations[0].face, Face::Face1);
        assert_eq!(s.observations[0].description, "BS");
        assert!((s.observations[1].slope_distance.unwrap() - 15.304).abs() < 1e-12);
        assert!((s.observations[1].zenith_deg.unwrap() - 101.2052 * 0.9).abs() < 1e-12);
        assert!((s.observations[1].hz_deg.unwrap() - 89.5370 * 0.9).abs() < 1e-12);
    }

    #[test]
    fn help_page_example_face2_backsight_without_distance() {
        let t = "NE 2 200 200\nAZ 2 1 100\nSTN 2 1.585\nF2 1 200\nF2 VA 101 311.4954 15.30 301.0355 IP\n";
        let d = parse_fbk(t, &opts(true)).unwrap();
        let o = &d.setups[0].observations;
        assert_eq!(o[0].face, Face::Face2);
        assert!(o[0].slope_distance.is_none() && o[0].horizontal_distance.is_none());
        assert!((o[0].hz_deg.unwrap() - 180.0).abs() < 1e-12);
        assert_eq!(o[1].description, "IP");
    }

    #[test]
    fn figures_draw_mode() {
        let t = "NE 1 1000 1000\nNE 2 1000 2000\nSTN 1\nBS 2\nBEG CURBA\nAD 4 50 60\nBEG CURBB\nAD 5 60 80\nAD 6 90 40\nCONT CURBA\nAD 7 20 60\nEND\nBEG CONYROAD\nPT 1\nPT 2\nCLOSE\n";
        let d = parse_fbk(t, &opts(true)).unwrap();
        let names: Vec<_> = d.figures.iter().map(|f| (f.name.as_str(), f.vertices.iter().map(|v| v.point.as_str()).collect::<Vec<_>>())).collect();
        assert_eq!(names[0], ("CURBA", vec!["4", "7"]));
        assert_eq!(names[1], ("CURBB", vec!["5", "6"]));
        assert!(d.figures[2].closed);
    }

    #[test]
    fn unsupported_commands_and_errors_have_line_numbers() {
        let t = "NE 1 10 10\nSTN 1 1.5\nZD 5 90 10\n";
        match parse_fbk(t, &opts(true)) {
            Err(SurveyError::Parse { line, .. }) => assert_eq!(line, 3),
            o => panic!("{o:?}"),
        }
        let d = parse_fbk(t, &opts(false)).unwrap();
        assert_eq!(d.warnings.len(), 1);
        assert!(parse_fbk("AD 1 10 10\n", &opts(true)).is_err());
        assert!(matches!(parse_fbk("NE 1 10 abc\n", &opts(true)), Err(SurveyError::Parse { line: 1, .. })));
    }

    #[test]
    fn ne_ss_and_quoted_description() {
        let t = "NE SS 10 100.5 200.25 5.5 \"TREE 12\"\nNEZ 11 1 2 3 CP\n";
        let d = parse_fbk(t, &opts(true)).unwrap();
        assert!(!d.points[0].control);
        assert_eq!((d.points[0].n, d.points[0].e, d.points[0].z), (Some(100.5), Some(200.25), Some(5.5)));
        assert_eq!(d.points[0].description, "TREE 12");
        assert!(d.points[1].control && d.points[1].fix_z);
    }
}
