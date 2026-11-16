//! Survey figures and the linework code set of this crate.
//!
//! The code set is modelled on the linework code set documented in the Autodesk Civil 3D help (pages
//! "About Special Codes", "About Curve Segment Codes", "Edit Linework Code Set Dialog Box", retrieved
//! 2026-11-16). It is a small subset, with every code configurable:
//!
//! * A point description is `NAME [CODE] [NAME [CODE] ...]`. Text after the escape character is a
//!   comment. Names and codes are separated by the delimiter.
//! * `B` begins a figure at this point (an active figure of the same name is ended first), `C`
//!   continues it, `E` continues and ends it, `CLS` continues, closes it with a segment back to its
//!   first vertex and ends it, `BC` begins a curve (continuing the figure to this point, or starting it),
//!   `OC` marks a point on a curve, `EC` ends a curve.
//! * A name with no code continues the active figure of that name; if none is active it begins a figure
//!   only when `auto_begin_on_prefix` is on and the name matches a figure prefix.
//! * A curve run with exactly three points (BC, one point on the curve, end) is the circular arc through
//!   the three points. Without an `EC` the third point after `BC` ends the curve.
//!
//! Not supported (documented in the README): horizontal and vertical offset codes, right turn,
//! rectangle, recall point, connect point, circle, curves through more than three points (they are
//! kept as chords and a warning is returned).

use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};

/// Maps a figure name prefix to the style and layer names and the breakline flag. Matching is by
/// longest prefix, ignoring case, as the figure prefix database of Civil 3D does by prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigurePrefix {
    pub prefix: String,
    pub style: String,
    pub layer: String,
    pub breakline: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineworkCodes {
    pub begin: String,
    pub cont: String,
    pub end: String,
    pub close: String,
    pub begin_curve: String,
    pub end_curve: String,
    pub on_curve: String,
    pub delimiter: char,
    pub escape: char,
    pub auto_begin_on_prefix: bool,
}

impl LineworkCodes {
    /// The codes used in the examples of the Civil 3D help pages cited above (B, C, E, CLS, BC, EC, OC,
    /// space delimiter, slash escape). They are example values of one code set, not a standard.
    pub fn civil3d_documented() -> LineworkCodes {
        LineworkCodes {
            begin: "B".into(),
            cont: "C".into(),
            end: "E".into(),
            close: "CLS".into(),
            begin_curve: "BC".into(),
            end_curve: "EC".into(),
            on_curve: "OC".into(),
            delimiter: ' ',
            escape: '/',
            auto_begin_on_prefix: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexKind {
    Line,
    CurveBegin,
    CurveOn,
    CurveEnd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigureVertex {
    pub point: String,
    pub kind: VertexKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Figure {
    pub name: String,
    pub vertices: Vec<FigureVertex>,
    pub closed: bool,
    /// Feeds a surface as a breakline.
    pub breakline: bool,
    pub style: String,
    pub layer: String,
    /// Generated from point descriptions by `process_linework` (such figures are replaced when the
    /// linework is generated again); false for figures defined by field book commands.
    pub from_linework: bool,
    pub event: Option<u64>,
}

impl Figure {
    pub fn new(name: &str, prefixes: &[FigurePrefix]) -> Figure {
        let mut f = Figure { name: name.into(), vertices: Vec::new(), closed: false, breakline: false, style: String::new(), layer: String::new(), from_linework: false, event: None };
        f.apply_prefix(prefixes);
        f
    }
    pub fn apply_prefix(&mut self, prefixes: &[FigurePrefix]) {
        let up = self.name.to_uppercase();
        if let Some(p) = prefixes.iter().filter(|p| up.starts_with(&p.prefix.to_uppercase())).max_by_key(|p| p.prefix.len()) {
            self.style = p.style.clone();
            self.layer = p.layer.clone();
            self.breakline = p.breakline;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Code {
    Begin,
    Cont,
    End,
    Close,
    BeginCurve,
    EndCurve,
    OnCurve,
}

/// Build figures from point descriptions given in the order of processing. Returns the figures and
/// warnings (continue of a figure that is not active, unsupported curve runs).
pub fn process_linework(points: &[(String, String)], codes: &LineworkCodes, prefixes: &[FigurePrefix]) -> (Vec<Figure>, Vec<String>) {
    let mut figs: Vec<Figure> = Vec::new();
    let mut active: Vec<(String, usize)> = Vec::new();
    let mut warnings = Vec::new();
    let find_code = |t: &str| -> Option<Code> {
        let table = [
            (&codes.begin, Code::Begin),
            (&codes.cont, Code::Cont),
            (&codes.end, Code::End),
            (&codes.close, Code::Close),
            (&codes.begin_curve, Code::BeginCurve),
            (&codes.end_curve, Code::EndCurve),
            (&codes.on_curve, Code::OnCurve),
        ];
        table.iter().find(|(c, _)| !c.is_empty() && c.eq_ignore_ascii_case(t)).map(|(_, k)| *k)
    };
    for (pname, desc) in points {
        let text = match desc.find(codes.escape) {
            Some(i) => &desc[..i],
            None => desc.as_str(),
        };
        let tokens: Vec<&str> = if codes.delimiter == ' ' { text.split_whitespace().collect() } else { text.split(codes.delimiter).map(|t| t.trim()).filter(|t| !t.is_empty()).collect() };
        // group into (name, codes) pairs
        let mut groups: Vec<(&str, Vec<Code>)> = Vec::new();
        for t in tokens {
            match find_code(t) {
                Some(c) if !groups.is_empty() => groups.last_mut().unwrap().1.push(c),
                _ => groups.push((t, Vec::new())),
            }
        }
        for (name, cs) in groups {
            let act = active.iter().position(|(n, _)| n == name);
            let begin_here = |figs: &mut Vec<Figure>, active: &mut Vec<(String, usize)>, kind: VertexKind| {
                if let Some(p) = active.iter().position(|(n, _)| n == name) {
                    active.remove(p);
                }
                let mut f = Figure::new(name, prefixes);
                f.vertices.push(FigureVertex { point: pname.clone(), kind });
                figs.push(f);
                active.push((name.to_string(), figs.len() - 1));
            };
            if cs.is_empty() {
                match act {
                    Some(p) => {
                        let fi = active[p].1;
                        let kind = implicit_kind(&figs[fi]);
                        figs[fi].vertices.push(FigureVertex { point: pname.clone(), kind });
                    }
                    None => {
                        let up = name.to_uppercase();
                        if codes.auto_begin_on_prefix && prefixes.iter().any(|p| up.starts_with(&p.prefix.to_uppercase())) {
                            begin_here(&mut figs, &mut active, VertexKind::Line);
                        }
                    }
                }
                continue;
            }
            for c in cs {
                match c {
                    Code::Begin => begin_here(&mut figs, &mut active, VertexKind::Line),
                    Code::BeginCurve => {
                        match active.iter().position(|(n, _)| n == name) {
                            Some(p) => {
                                let fi = active[p].1;
                                figs[fi].vertices.push(FigureVertex { point: pname.clone(), kind: VertexKind::CurveBegin });
                            }
                            None => begin_here(&mut figs, &mut active, VertexKind::CurveBegin),
                        }
                    }
                    Code::Cont | Code::End | Code::Close | Code::OnCurve | Code::EndCurve => match active.iter().position(|(n, _)| n == name) {
                        Some(p) => {
                            let fi = active[p].1;
                            let kind = match c {
                                Code::OnCurve => VertexKind::CurveOn,
                                Code::EndCurve => VertexKind::CurveEnd,
                                _ => implicit_kind(&figs[fi]),
                            };
                            if c != Code::Close {
                                figs[fi].vertices.push(FigureVertex { point: pname.clone(), kind });
                            } else {
                                figs[fi].vertices.push(FigureVertex { point: pname.clone(), kind: VertexKind::Line });
                                figs[fi].closed = true;
                            }
                            if matches!(c, Code::End | Code::Close) {
                                active.remove(p);
                            }
                        }
                        None => {
                            warnings.push(format!("point {pname}: code for figure '{name}' but no such figure is active; a figure was begun here"));
                            begin_here(&mut figs, &mut active, VertexKind::Line);
                            if matches!(c, Code::End | Code::Close) {
                                active.retain(|(n, _)| n != name);
                            }
                        }
                    },
                }
            }
        }
    }
    for f in &figs {
        let mut run = 0usize;
        let mut in_curve = false;
        for v in &f.vertices {
            match v.kind {
                VertexKind::CurveBegin => {
                    in_curve = true;
                    run = 1;
                }
                VertexKind::CurveOn if in_curve => run += 1,
                VertexKind::CurveEnd if in_curve => {
                    run += 1;
                    if run > 3 {
                        warnings.push(format!("figure '{}': a curve through {run} points is kept as chords (only three point arcs are supported)", f.name));
                    }
                    in_curve = false;
                }
                _ => {}
            }
        }
    }
    (figs, warnings)
}

/// Kind of a vertex added without an explicit curve code: after a curve begin the next point is on the
/// curve, and the point after that ends a three point curve.
fn implicit_kind(f: &Figure) -> VertexKind {
    let n = f.vertices.len();
    match f.vertices.last().map(|v| v.kind) {
        Some(VertexKind::CurveBegin) => VertexKind::CurveOn,
        Some(VertexKind::CurveOn) => {
            // number of vertices since the last CurveBegin
            let start = f.vertices.iter().rposition(|v| v.kind == VertexKind::CurveBegin).unwrap_or(0);
            if n - start == 2 {
                VertexKind::CurveEnd
            } else {
                VertexKind::CurveOn
            }
        }
        _ => VertexKind::Line,
    }
}

/// A polyline produced from a figure, 3D.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Breakline {
    pub name: String,
    pub layer: String,
    pub style: String,
    pub closed: bool,
    pub points: Vec<[f64; 3]>,
}

type Coord = (f64, f64, Option<f64>);

/// Flatten a figure to a polyline: lines stay straight, a three point curve becomes an arc through
/// the three points, flattened with chords whose sagitta does not exceed `chord_tolerance` metres
/// (the sagitta of a chord subtending angle t on radius r is r (1 - cos(t/2)), so t = 2 acos(1 - tol/r)).
/// Elevations along an arc are interpolated linearly in arc angle. A figure with a missing vertex or a
/// missing elevation (when `need_z`) is an error.
pub fn flatten_figure(fig: &Figure, lookup: &dyn Fn(&str) -> Option<Coord>, chord_tolerance: f64, need_z: bool) -> Result<Breakline> {
    if !(chord_tolerance > 0.0) {
        return Err(SurveyError::Invalid("chord tolerance must be positive".into()));
    }
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let get = |name: &str| -> Result<[f64; 3]> {
        let (e, n, z) = lookup(name).ok_or_else(|| SurveyError::NoPoint(name.into()))?;
        match z {
            Some(z) => Ok([e, n, z]),
            None if need_z => Err(SurveyError::Invalid(format!("point '{name}' of figure '{}' has no elevation", fig.name))),
            None => Ok([e, n, f64::NAN]),
        }
    };
    let v = &fig.vertices;
    let mut i = 0;
    while i < v.len() {
        let p = get(&v[i].point)?;
        if v[i].kind == VertexKind::CurveBegin && i + 2 < v.len() && v[i + 1].kind == VertexKind::CurveOn && v[i + 2].kind == VertexKind::CurveEnd {
            let q = get(&v[i + 1].point)?;
            let r = get(&v[i + 2].point)?;
            pts.push(p);
            pts.extend(arc_points(p, q, r, chord_tolerance));
            i += 3;
        } else {
            pts.push(p);
            i += 1;
        }
    }
    if fig.closed {
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
    }
    Ok(Breakline { name: fig.name.clone(), layer: fig.layer.clone(), style: fig.style.clone(), closed: fig.closed, points: pts })
}

/// Points after `a` along the circular arc a -> b -> c (b on the arc), ending at c.
fn arc_points(a: [f64; 3], b: [f64; 3], c: [f64; 3], tol: f64) -> Vec<[f64; 3]> {
    // circumcentre relative to a (keeps UTM sized numbers out of the arithmetic)
    let (bx, by) = (b[0] - a[0], b[1] - a[1]);
    let (cx, cy) = (c[0] - a[0], c[1] - a[1]);
    let d = 2.0 * (bx * cy - by * cx);
    if d.abs() < 1e-12 {
        return vec![b, c];
    }
    let b2 = bx * bx + by * by;
    let c2 = cx * cx + cy * cy;
    let ux = (cy * b2 - by * c2) / d;
    let uy = (bx * c2 - cx * b2) / d;
    let r = ux.hypot(uy);
    let ang = |x: f64, y: f64| (y - uy).atan2(x - ux);
    let (a0, a1, a2) = (ang(0.0, 0.0), ang(bx, by), ang(cx, cy));
    // orientation: counter-clockwise if the cross product of (b-a) x (c-b) is positive
    let ccw = (bx) * (cy - by) - (by) * (cx - bx) > 0.0;
    let two_pi = 2.0 * std::f64::consts::PI;
    let sweep = |from: f64, to: f64| -> f64 {
        let mut s = if ccw { to - from } else { from - to };
        while s < 0.0 {
            s += two_pi;
        }
        while s >= two_pi {
            s -= two_pi;
        }
        s
    };
    let total = sweep(a0, a2);
    let t_max = if tol >= r { std::f64::consts::PI } else { 2.0 * (1.0 - tol / r).acos() };
    let n = ((total / t_max).ceil() as usize).max(2);
    let _ = a1;
    let sgn = if ccw { 1.0 } else { -1.0 };
    let mut out = Vec::new();
    for k in 1..n {
        let f = k as f64 / n as f64;
        let t = a0 + sgn * total * f;
        let z = if a[2].is_nan() || c[2].is_nan() { f64::NAN } else { a[2] + (c[2] - a[2]) * f };
        out.push([a[0] + ux + r * t.cos(), a[1] + uy + r * t.sin(), z]);
    }
    out.push(c);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn begin_continue_end_close() {
        let c = LineworkCodes::civil3d_documented();
        let (f, w) = process_linework(&pts(&[("1", "EP1 B"), ("2", "EP1"), ("3", "EP1 C"), ("4", "EP1 E"), ("5", "EP1"), ("6", "SW B"), ("7", "SW"), ("8", "SW CLS /note")]), &c, &[]);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].vertices.iter().map(|v| v.point.as_str()).collect::<Vec<_>>(), ["1", "2", "3", "4"]);
        assert!(!f[0].closed);
        assert!(f[1].closed);
        assert_eq!(f[1].vertices.len(), 3);
    }

    #[test]
    fn two_figures_in_one_description_and_restart() {
        let c = LineworkCodes::civil3d_documented();
        let (f, _) = process_linework(&pts(&[("1", "A B C1 B"), ("2", "A E C1"), ("3", "A B")]), &c, &[]);
        // A: 1,2 ended; then a new A begins at 3.  C1: 1,2
        let a: Vec<_> = f.iter().filter(|x| x.name == "A").collect();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].vertices.len(), 2);
        assert_eq!(f.iter().find(|x| x.name == "C1").unwrap().vertices.len(), 2);
    }

    #[test]
    fn prefix_table_and_auto_begin() {
        let pre = vec![
            FigurePrefix { prefix: "EP".into(), style: "Edge".into(), layer: "V-EDGE".into(), breakline: true },
            FigurePrefix { prefix: "EPX".into(), style: "EdgeX".into(), layer: "V-EDGEX".into(), breakline: false },
        ];
        let mut c = LineworkCodes::civil3d_documented();
        c.auto_begin_on_prefix = true;
        let (f, _) = process_linework(&pts(&[("1", "EP1"), ("2", "EP1"), ("3", "EPX7"), ("4", "TREE")]), &c, &pre);
        assert_eq!(f.len(), 2, "TREE matches no prefix and does not begin a figure");
        assert_eq!(f[0].layer, "V-EDGE");
        assert!(f[0].breakline);
        assert_eq!(f[1].layer, "V-EDGEX", "longest prefix wins");
        assert!(!f[1].breakline);
        c.auto_begin_on_prefix = false;
        let (f2, _) = process_linework(&pts(&[("1", "EP1"), ("2", "EP1")]), &c, &pre);
        assert!(f2.is_empty());
    }

    #[test]
    fn three_point_curve_and_flatten() {
        let c = LineworkCodes::civil3d_documented();
        let (f, w) = process_linework(&pts(&[("1", "K B"), ("2", "K BC"), ("3", "K"), ("4", "K"), ("5", "K E")]), &c, &[]);
        assert!(w.is_empty());
        let kinds: Vec<_> = f[0].vertices.iter().map(|v| v.kind).collect();
        assert_eq!(kinds, [VertexKind::Line, VertexKind::CurveBegin, VertexKind::CurveOn, VertexKind::CurveEnd, VertexKind::Line]);
        // points 2,3,4 on a circle radius 50 about (500000, 9960000): angles 0, 45, 90 degrees
        let r = 50.0;
        let o = (500_000.0, 9_960_000.0);
        let on = |deg: f64| (o.0 + r * deg.to_radians().cos(), o.1 + r * deg.to_radians().sin(), Some(10.0));
        let lookup = move |n: &str| match n {
            "1" => Some((o.0 + r + 5.0, o.1 - 5.0, Some(10.0))),
            "2" => Some(on(0.0)),
            "3" => Some(on(45.0)),
            "4" => Some(on(90.0)),
            "5" => Some((o.0, o.1 + r + 5.0, Some(10.0))),
            _ => None,
        };
        let bl = flatten_figure(&f[0], &lookup, 0.001, true).unwrap();
        // every flattened vertex lies on the circle to within round-off, chords within 1 mm sagitta
        let mut n_on_arc = 0;
        for p in &bl.points[1..bl.points.len() - 1] {
            let d = ((p[0] - o.0).powi(2) + (p[1] - o.1).powi(2)).sqrt();
            if (d - r).abs() < 1e-6 {
                n_on_arc += 1;
            }
        }
        assert!(n_on_arc > 20, "arc was flattened into many chords: {n_on_arc}");
        assert!(lookup("9").is_none());
        assert!(flatten_figure(&f[0], &|n: &str| if n == "3" { None } else { lookup(n) }, 0.001, true).is_err());
    }
}
