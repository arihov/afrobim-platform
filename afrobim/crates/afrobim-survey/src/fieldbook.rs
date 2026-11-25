// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The plain text field book format of this crate ("AFROFB").
//!
//! This is the crate's own documented format, not a copy of any vendor format. One record per line,
//! keyword first, then positional fields, then `KEY=value` options. Case of keywords and keys does
//! not matter, values are case sensitive. `#` starts a comment line. `DESC=` takes the rest of the line.
//! A strict parser: any unknown keyword, unknown key, missing field or unreadable number is an error
//! reported with its line number.
//!
//! ```text
//! AFROFB 1
//! UNITS DIST=m ANGLE=deg VERT=zenith           (DIST=other needs DISTFACTOR=<metres per unit>)
//! CONTROL A 500000.000 9960000.000 1000.000 FIX=ENZ DESC=nail
//! CONTROL B 500100.000 9960000.000 -           (Z given as "-": no height; FIX defaults to what is given)
//! POINT   C 500050 9960040 -                   (given coordinates, not control)
//! SETUP A 1.55 BS=B BSREAD=0 EQUIP=T1          (station, instrument height; BSAZ=<az> for an azimuth)
//! OBS P1 HZ=90.5 HZKIND=angle ZEN=88.2 SD=55.123 TH=1.8 FACE=1 DESC=EP1 B
//! DIRECTION A B 90.0 SIGMA=5                   (known azimuth; SIGMA in arc seconds makes it an observation)
//! LEVEL R1 EQUIP=L1
//! LEG A B DH=1.234 LEN=120                     (or BS=1.512 FS=0.278)
//! BASELINE A B 100.0 0.0 0.5 0.01 0.01 0.02    (dE dN dU and their sigmas)
//! FIGURE EP1 CLOSED BREAKLINE LAYER=V-EDGE STYLE=Edge
//! VERTEX P1 [LINE|CB|CO|CE]
//! ```
//! Angles are written in the unit of the UNITS line (`deg`, `gon`, `dms` as 123-45-30.5, `packed` as
//! DDD.MMSS), vertical angles in the VERT type. HZKIND is `angle` (turned from the backsight) or `dir`
//! (circle reading, the default). FACE is 1 or 2 (omitted: single face).

use crate::error::{parse_err, Result, SurveyError};
use crate::figures::{Figure, FigureVertex, VertexKind};
use crate::import::ParsedData;
use crate::model::*;
use crate::units::*;

pub(crate) struct Args {
    line: usize,
    pub pos: Vec<String>,
    keys: Vec<(String, String)>,
    pub desc: Option<String>,
}

impl Args {
    pub fn parse(line: usize, text: &str) -> Args {
        let upper = text.to_uppercase();
        let (head, desc) = match upper.find(" DESC=") {
            Some(i) => (&text[..i], Some(text[i + 6..].trim().to_string())),
            None => (text, None),
        };
        let mut pos = Vec::new();
        let mut keys = Vec::new();
        for t in head.split_whitespace() {
            match t.split_once('=') {
                Some((k, v)) => keys.push((k.to_uppercase(), v.to_string())),
                None => pos.push(t.to_string()),
            }
        }
        Args { line, pos, keys, desc }
    }
    pub fn take(&mut self, key: &str) -> Option<String> {
        let i = self.keys.iter().position(|(k, _)| k == key)?;
        Some(self.keys.remove(i).1)
    }
    pub fn flag(&mut self, word: &str) -> bool {
        match self.pos.iter().position(|p| p.eq_ignore_ascii_case(word)) {
            Some(i) => {
                self.pos.remove(i);
                true
            }
            None => false,
        }
    }
    pub fn finish(&self, kw: &str) -> Result<()> {
        if let Some((k, _)) = self.keys.first() {
            return Err(parse_err(self.line, format!("{kw}: unknown key {k}=")));
        }
        Ok(())
    }
    pub fn num(&self, s: &str, what: &str) -> Result<f64> {
        s.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| parse_err(self.line, format!("cannot read {what} from '{s}'")))
    }
    pub fn need(&self, i: usize, kw: &str, what: &str) -> Result<String> {
        self.pos.get(i).cloned().ok_or_else(|| parse_err(self.line, format!("{kw}: missing {what}")))
    }
    pub fn exact(&self, n: usize, kw: &str) -> Result<()> {
        if self.pos.len() > n {
            return Err(parse_err(self.line, format!("{kw}: unexpected extra field '{}'", self.pos[n])));
        }
        Ok(())
    }
}

struct Units {
    dist: f64,
    angle: AngleUnit,
    vert: VerticalKind,
}

pub fn parse_fieldbook(text: &str) -> Result<ParsedData> {
    let mut out = ParsedData::default();
    let mut units: Option<Units> = None;
    let mut seen_header = false;
    let mut cur_setup: Option<usize> = None;
    let mut cur_run: Option<usize> = None;
    let mut cur_fig: Option<usize> = None;
    let mut run_id = 0u64;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let kw_end = t.find(char::is_whitespace).unwrap_or(t.len());
        let kw = t[..kw_end].to_uppercase();
        let rest = &t[kw_end..];
        let mut a = Args::parse(line, rest);
        if !seen_header {
            if kw != "AFROFB" {
                return Err(parse_err(line, "the first record must be 'AFROFB 1'"));
            }
            let v = a.need(0, "AFROFB", "version")?;
            if v != "1" {
                return Err(parse_err(line, format!("AFROFB version {v} is not supported")));
            }
            seen_header = true;
            continue;
        }
        if kw == "UNITS" {
            let dist = a.take("DIST").ok_or_else(|| parse_err(line, "UNITS: DIST= is required"))?;
            let factor = a.take("DISTFACTOR");
            let dist_f = if dist.eq_ignore_ascii_case("m") {
                if factor.is_some() {
                    return Err(parse_err(line, "UNITS: DISTFACTOR is only for units other than m"));
                }
                1.0
            } else {
                let f = factor.ok_or_else(|| parse_err(line, format!("UNITS: DIST={dist} needs DISTFACTOR=<metres per unit>")))?;
                let f = a.num(&f, "DISTFACTOR")?;
                if !(f > 0.0) {
                    return Err(parse_err(line, "UNITS: DISTFACTOR must be positive"));
                }
                f
            };
            let angle = match a.take("ANGLE").ok_or_else(|| parse_err(line, "UNITS: ANGLE= is required"))?.to_lowercase().as_str() {
                "deg" => AngleUnit::Degrees,
                "gon" => AngleUnit::Gon,
                "dms" => AngleUnit::DmsText,
                "packed" => AngleUnit::DmsPacked,
                o => return Err(parse_err(line, format!("UNITS: ANGLE={o} is not one of deg, gon, dms, packed"))),
            };
            let vert = match a.take("VERT").unwrap_or_else(|| "zenith".into()).to_lowercase().as_str() {
                "zenith" => VerticalKind::Zenith,
                "elevation" => VerticalKind::Elevation,
                "nadir" => VerticalKind::Nadir,
                o => return Err(parse_err(line, format!("UNITS: VERT={o} is not one of zenith, elevation, nadir"))),
            };
            a.finish("UNITS")?;
            units = Some(Units { dist: dist_f, angle, vert });
            continue;
        }
        let u = units.as_ref().ok_or_else(|| parse_err(line, format!("{kw}: a UNITS record must come first")))?;
        let ang = |s: &str| -> Result<f64> { parse_angle(s, u.angle).map_err(|_| parse_err(line, format!("cannot read angle '{s}'"))) };
        let dist = |a: &Args, s: &str, w: &str| -> Result<f64> { Ok(a.num(s, w)? * u.dist) };
        match kw.as_str() {
            "CONTROL" | "POINT" => {
                let name = a.need(0, &kw, "point name")?;
                let comp = |a: &Args, idx: usize, w: &str| -> Result<Option<f64>> {
                    let s = a.need(idx, &kw, w)?;
                    if s == "-" {
                        Ok(None)
                    } else {
                        Ok(Some(dist(a, &s, w)?))
                    }
                };
                let e = comp(&a, 1, "easting")?.ok_or_else(|| parse_err(line, format!("{kw}: easting cannot be '-'")))?;
                let n = comp(&a, 2, "northing")?.ok_or_else(|| parse_err(line, format!("{kw}: northing cannot be '-'")))?;
                let z = comp(&a, 3, "height (or '-')")?;
                a.exact(4, &kw)?;
                let mut p = if kw == "CONTROL" { SurveyPoint::control(&name, e, n, z) } else { SurveyPoint::given(&name, e, n, z) };
                if let Some(f) = a.take("FIX") {
                    if kw == "POINT" {
                        return Err(parse_err(line, "POINT: FIX is only for CONTROL"));
                    }
                    let f = f.to_uppercase();
                    if f != "NONE" && !f.chars().all(|c| "ENZ".contains(c)) {
                        return Err(parse_err(line, format!("CONTROL: FIX={f} must be NONE or letters from E, N, Z")));
                    }
                    p.fix_e = f.contains('E');
                    p.fix_n = f.contains('N');
                    p.fix_z = f.contains('Z') && z.is_some();
                    if f.contains('Z') && z.is_none() {
                        return Err(parse_err(line, "CONTROL: FIX contains Z but no height is given"));
                    }
                }
                for (key, slot) in [("SE", &mut p.sigma_e), ("SN", &mut p.sigma_n), ("SZ", &mut p.sigma_z)] {
                    if let Some(v) = a.take(key) {
                        *slot = Some(dist(&a, &v, key)?);
                    }
                }
                p.description = a.desc.clone().unwrap_or_default();
                a.finish(&kw)?;
                out.points.push(p);
            }
            "SETUP" => {
                let st = a.need(0, "SETUP", "station")?;
                let hi = dist(&a, &a.need(1, "SETUP", "instrument height")?, "instrument height")?;
                a.exact(2, "SETUP")?;
                let mut s = Setup::new(&st, hi);
                s.backsight_point = a.take("BS");
                if let Some(v) = a.take("BSAZ") {
                    s.backsight_azimuth_deg = Some(ang(&v)?);
                }
                if let Some(v) = a.take("BSREAD") {
                    s.backsight_reading_deg = ang(&v)?;
                }
                s.equipment = a.take("EQUIP");
                a.finish("SETUP")?;
                out.setups.push(s);
                cur_setup = Some(out.setups.len() - 1);
                cur_run = None;
                cur_fig = None;
            }
            "OBS" => {
                let si = cur_setup.ok_or_else(|| parse_err(line, "OBS before any SETUP"))?;
                let target = a.need(0, "OBS", "target")?;
                a.exact(1, "OBS")?;
                let mut o = Observation::new(&target);
                o.source_line = Some(line);
                if let Some(v) = a.take("HZ") {
                    o.hz_deg = Some(ang(&v)?);
                }
                match a.take("HZKIND").map(|s| s.to_lowercase()).as_deref() {
                    None | Some("dir") => o.hz_kind = HzKind::Direction,
                    Some("angle") => o.hz_kind = HzKind::Angle,
                    Some(other) => return Err(parse_err(line, format!("OBS: HZKIND={other} is not 'angle' or 'dir'"))),
                }
                if let Some(v) = a.take("ZEN") {
                    o.zenith_deg = Some(to_zenith(ang(&v)?, u.vert));
                }
                for (key, slot) in [("SD", &mut o.slope_distance), ("HD", &mut o.horizontal_distance), ("VD", &mut o.vertical_distance)] {
                    if let Some(v) = a.take(key) {
                        let x = dist(&a, &v, key)?;
                        if key != "VD" && x < 0.0 {
                            return Err(parse_err(line, format!("OBS: {key} cannot be negative")));
                        }
                        *slot = Some(x);
                    }
                }
                if let Some(v) = a.take("TH") {
                    o.target_height = dist(&a, &v, "TH")?;
                }
                match a.take("FACE").as_deref() {
                    None => {}
                    Some("1") => o.face = Face::Face1,
                    Some("2") => o.face = Face::Face2,
                    Some(other) => return Err(parse_err(line, format!("OBS: FACE={other} is not 1 or 2"))),
                }
                o.description = a.desc.clone().unwrap_or_default();
                a.finish("OBS")?;
                if o.hz_deg.is_none() && o.zenith_deg.is_none() && o.slope_distance.is_none() && o.horizontal_distance.is_none() && o.vertical_distance.is_none() {
                    return Err(parse_err(line, "OBS: no measurement given"));
                }
                out.setups[si].observations.push(o);
            }
            "DIRECTION" => {
                let from = a.need(0, "DIRECTION", "from point")?;
                let to = a.need(1, "DIRECTION", "to point")?;
                let az = ang(&a.need(2, "DIRECTION", "azimuth")?)?;
                a.exact(3, "DIRECTION")?;
                let sigma = match a.take("SIGMA") {
                    Some(v) => Some(a.num(&v, "SIGMA")?),
                    None => None,
                };
                a.finish("DIRECTION")?;
                out.directions.push(KnownDirection { from, to, azimuth_deg: az, sigma_arcsec: sigma, event: None });
            }
            "LEVEL" => {
                let name = a.need(0, "LEVEL", "run name")?;
                a.exact(1, "LEVEL")?;
                let eq = a.take("EQUIP");
                a.finish("LEVEL")?;
                run_id += 1;
                out.level_runs.push(LevelRun { id: run_id, name, legs: Vec::new(), equipment: eq, event: None });
                cur_run = Some(out.level_runs.len() - 1);
                cur_setup = None;
                cur_fig = None;
            }
            "LEG" => {
                let ri = cur_run.ok_or_else(|| parse_err(line, "LEG before any LEVEL"))?;
                let from = a.need(0, "LEG", "from point")?;
                let to = a.need(1, "LEG", "to point")?;
                a.exact(2, "LEG")?;
                let len = match a.take("LEN") {
                    Some(v) => dist(&a, &v, "LEN")?,
                    None => return Err(parse_err(line, "LEG: LEN= (length) is required")),
                };
                let dh = match (a.take("DH"), a.take("BS"), a.take("FS")) {
                    (Some(d), None, None) => dist(&a, &d, "DH")?,
                    (None, Some(b), Some(f)) => dist(&a, &b, "BS")? - dist(&a, &f, "FS")?,
                    _ => return Err(parse_err(line, "LEG: give DH= or both BS= and FS=")),
                };
                a.finish("LEG")?;
                out.level_runs[ri].legs.push(LevelLeg { from, to, dh, length_m: len });
            }
            "BASELINE" => {
                let from = a.need(0, "BASELINE", "from point")?;
                let to = a.need(1, "BASELINE", "to point")?;
                let mut v = [0.0; 6];
                for (k, w) in ["dE", "dN", "dU", "sigma E", "sigma N", "sigma U"].iter().enumerate() {
                    v[k] = dist(&a, &a.need(2 + k, "BASELINE", w)?, w)?;
                }
                a.exact(8, "BASELINE")?;
                a.finish("BASELINE")?;
                if v[3] <= 0.0 || v[4] <= 0.0 || v[5] <= 0.0 {
                    return Err(parse_err(line, "BASELINE: sigmas must be positive"));
                }
                out.baselines.push(Baseline { from, to, de: v[0], dn: v[1], du: v[2], sigma_e: v[3], sigma_n: v[4], sigma_u: v[5], event: None });
            }
            "FIGURE" => {
                let name = a.need(0, "FIGURE", "name")?;
                let closed = a.flag("CLOSED");
                let bl = a.flag("BREAKLINE");
                a.exact(1, "FIGURE")?;
                let layer = a.take("LAYER").unwrap_or_default();
                let style = a.take("STYLE").unwrap_or_default();
                a.finish("FIGURE")?;
                out.figures.push(Figure { name, vertices: Vec::new(), closed, breakline: bl, style, layer, from_linework: false, event: None });
                cur_fig = Some(out.figures.len() - 1);
                cur_setup = None;
                cur_run = None;
            }
            "VERTEX" => {
                let fi = cur_fig.ok_or_else(|| parse_err(line, "VERTEX before any FIGURE"))?;
                let p = a.need(0, "VERTEX", "point")?;
                let kind = match a.pos.get(1).map(|s| s.to_uppercase()).as_deref() {
                    None | Some("LINE") => VertexKind::Line,
                    Some("CB") => VertexKind::CurveBegin,
                    Some("CO") => VertexKind::CurveOn,
                    Some("CE") => VertexKind::CurveEnd,
                    Some(o) => return Err(parse_err(line, format!("VERTEX: kind {o} is not LINE, CB, CO or CE"))),
                };
                a.exact(2, "VERTEX")?;
                a.finish("VERTEX")?;
                out.figures[fi].vertices.push(FigureVertex { point: p, kind });
            }
            other => return Err(parse_err(line, format!("unknown record '{other}'"))),
        }
    }
    if !seen_header {
        return Err(SurveyError::Parse { line: 1, message: "empty file: the first record must be 'AFROFB 1'".into() });
    }
    Ok(out)
}

/// Write a network as a field book: metres, decimal degrees, zenith angles. Reading it back returns
/// the same data (numbers are written with full precision).
pub fn write_fieldbook(net: &Network) -> String {
    let mut s = String::new();
    s.push_str("AFROFB 1\nUNITS DIST=m ANGLE=deg VERT=zenith\n");
    let f = |v: f64| format!("{v}");
    for p in &net.points {
        let (e, n) = match (p.e, p.n) {
            (Some(e), Some(n)) => (e, n),
            _ => continue,
        };
        let z = p.z.map(f).unwrap_or_else(|| "-".into());
        if p.control {
            let fix: String = [(p.fix_e, 'E'), (p.fix_n, 'N'), (p.fix_z, 'Z')].iter().filter(|x| x.0).map(|x| x.1).collect();
            s.push_str(&format!("CONTROL {} {} {} {} FIX={}", p.name, f(e), f(n), z, if fix.is_empty() { "NONE".to_string() } else { fix }));
            for (k, v) in [("SE", p.sigma_e), ("SN", p.sigma_n), ("SZ", p.sigma_z)] {
                if let Some(v) = v {
                    s.push_str(&format!(" {k}={}", f(v)));
                }
            }
        } else {
            s.push_str(&format!("POINT {} {} {} {}", p.name, f(e), f(n), z));
        }
        if !p.description.is_empty() {
            s.push_str(&format!(" DESC={}", p.description));
        }
        s.push('\n');
    }
    for d in &net.directions {
        s.push_str(&format!("DIRECTION {} {} {}", d.from, d.to, f(d.azimuth_deg)));
        if let Some(sg) = d.sigma_arcsec {
            s.push_str(&format!(" SIGMA={}", f(sg)));
        }
        s.push('\n');
    }
    for su in &net.setups {
        s.push_str(&format!("SETUP {} {}", su.station, f(su.instrument_height)));
        if let Some(b) = &su.backsight_point {
            s.push_str(&format!(" BS={b}"));
        }
        if let Some(a) = su.backsight_azimuth_deg {
            s.push_str(&format!(" BSAZ={}", f(a)));
        }
        if su.backsight_reading_deg != 0.0 {
            s.push_str(&format!(" BSREAD={}", f(su.backsight_reading_deg)));
        }
        if let Some(e) = &su.equipment {
            s.push_str(&format!(" EQUIP={e}"));
        }
        s.push('\n');
        for o in &su.observations {
            s.push_str(&format!("OBS {}", o.target));
            if let Some(h) = o.hz_deg {
                s.push_str(&format!(" HZ={} HZKIND={}", f(h), if o.hz_kind == HzKind::Angle { "angle" } else { "dir" }));
            }
            if let Some(z) = o.zenith_deg {
                s.push_str(&format!(" ZEN={}", f(z)));
            }
            for (k, v) in [("SD", o.slope_distance), ("HD", o.horizontal_distance), ("VD", o.vertical_distance)] {
                if let Some(v) = v {
                    s.push_str(&format!(" {k}={}", f(v)));
                }
            }
            if o.target_height != 0.0 {
                s.push_str(&format!(" TH={}", f(o.target_height)));
            }
            match o.face {
                Face::Face1 => s.push_str(" FACE=1"),
                Face::Face2 => s.push_str(" FACE=2"),
                Face::Single => {}
            }
            if !o.description.is_empty() {
                s.push_str(&format!(" DESC={}", o.description));
            }
            s.push('\n');
        }
    }
    for r in &net.level_runs {
        s.push_str(&format!("LEVEL {}", r.name));
        if let Some(e) = &r.equipment {
            s.push_str(&format!(" EQUIP={e}"));
        }
        s.push('\n');
        for l in &r.legs {
            s.push_str(&format!("LEG {} {} DH={} LEN={}\n", l.from, l.to, f(l.dh), f(l.length_m)));
        }
    }
    for b in &net.baselines {
        s.push_str(&format!("BASELINE {} {} {} {} {} {} {} {}\n", b.from, b.to, f(b.de), f(b.dn), f(b.du), f(b.sigma_e), f(b.sigma_n), f(b.sigma_u)));
    }
    for fg in &net.figures {
        s.push_str(&format!("FIGURE {}{}{}", fg.name, if fg.closed { " CLOSED" } else { "" }, if fg.breakline { " BREAKLINE" } else { "" }));
        if !fg.layer.is_empty() {
            s.push_str(&format!(" LAYER={}", fg.layer));
        }
        if !fg.style.is_empty() {
            s.push_str(&format!(" STYLE={}", fg.style));
        }
        s.push('\n');
        for v in &fg.vertices {
            let k = match v.kind {
                VertexKind::Line => "LINE",
                VertexKind::CurveBegin => "CB",
                VertexKind::CurveOn => "CO",
                VertexKind::CurveEnd => "CE",
            };
            s.push_str(&format!("VERTEX {} {k}\n", v.point));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# job\nAFROFB 1\nUNITS DIST=m ANGLE=dms VERT=zenith\nCONTROL A 500000.000 9960000.000 1000.000 FIX=ENZ DESC=nail head\nCONTROL B 500100 9960000 -\nSETUP A 1.55 BS=B BSREAD=0-00-00 EQUIP=T1\nOBS P1 HZ=90-30-15.5 HZKIND=angle ZEN=88-12-00 SD=55.123 TH=1.8 FACE=1 DESC=EP1 B\nLEVEL R1 EQUIP=L1\nLEG A B BS=1.512 FS=0.278 LEN=120\n";

    #[test]
    fn parses_sample() {
        let d = parse_fieldbook(SAMPLE).unwrap();
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.points[0].description, "nail head");
        assert_eq!(d.points[1].z, None);
        assert!(!d.points[1].fix_z && d.points[1].fix_e);
        let o = &d.setups[0].observations[0];
        assert!((o.hz_deg.unwrap() - (90.0 + 30.0 / 60.0 + 15.5 / 3600.0)).abs() < 1e-12);
        assert_eq!(o.hz_kind, HzKind::Angle);
        assert_eq!(o.face, Face::Face1);
        assert_eq!(o.description, "EP1 B");
        assert!((d.level_runs[0].legs[0].dh - 1.234).abs() < 1e-12);
    }

    #[test]
    fn strict_errors_carry_line_numbers() {
        let cases: [(&str, usize); 8] = [
            ("OBS P1 HZ=1\n", 1),
            ("AFROFB 1\nOBS P1 HZ=1\n", 2),
            ("AFROFB 1\nUNITS DIST=m ANGLE=deg\nSETUP A\n", 3),
            ("AFROFB 1\nUNITS DIST=m ANGLE=deg\nSETUP A 1.5\nOBS P HZ=abc\n", 4),
            ("AFROFB 1\nUNITS DIST=m ANGLE=deg\nFOO x\n", 3),
            ("AFROFB 1\nUNITS DIST=ft ANGLE=deg\n", 2),
            ("AFROFB 1\nUNITS DIST=m ANGLE=deg\nCONTROL A 1 2 3 WHAT=1\n", 3),
            ("AFROFB 1\nUNITS DIST=m ANGLE=deg\nSETUP A 1.5\nOBS P\n", 4),
        ];
        for (t, l) in cases {
            match parse_fieldbook(t) {
                Err(SurveyError::Parse { line, .. }) => assert_eq!(line, l, "{t}"),
                other => panic!("expected parse error for {t:?}, got {other:?}"),
            }
        }
        let e = parse_fieldbook("AFROFB 1\nUNITS DIST=m ANGLE=deg\nFOO x\n").unwrap_err();
        assert!(e.to_string().starts_with("line 3:"));
    }

    #[test]
    fn explicit_distance_factor_and_round_trip() {
        let d = parse_fieldbook("AFROFB 1\nUNITS DIST=ft DISTFACTOR=0.3048 ANGLE=gon VERT=elevation\nCONTROL A 100 200 10\nSETUP A 5\nOBS P HZ=100 ZEN=0 SD=100\n").unwrap();
        assert!((d.points[0].e.unwrap() - 30.48).abs() < 1e-12);
        assert!((d.setups[0].observations[0].hz_deg.unwrap() - 90.0).abs() < 1e-12);
        assert!((d.setups[0].observations[0].zenith_deg.unwrap() - 90.0).abs() < 1e-12);
        assert!((d.setups[0].observations[0].slope_distance.unwrap() - 30.48).abs() < 1e-12);

        let mut net = Network::new("N");
        net.points = parse_fieldbook(SAMPLE).unwrap().points;
        net.setups = parse_fieldbook(SAMPLE).unwrap().setups;
        net.level_runs = parse_fieldbook(SAMPLE).unwrap().level_runs;
        let text = write_fieldbook(&net);
        let mut again = parse_fieldbook(&text).unwrap();
        // source line numbers legitimately differ in the rewritten file
        for s in again.setups.iter_mut().chain(net.setups.iter_mut()) {
            s.observations.iter_mut().for_each(|o| o.source_line = None);
        }
        assert_eq!(again.points, net.points);
        assert_eq!(again.setups, net.setups);
        assert_eq!(again.level_runs[0].legs, net.level_runs[0].legs);
    }
}
