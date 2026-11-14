//! LandXML import of alignments: horizontal geometry, vertical profile, station equations and
//! superelevation.
//!
//! Reads `LandXML/Alignments/Alignment` as laid down by the LandXML 1.2 schema
//! (<http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd>; every element and attribute name
//! below was checked against it). Elements are matched by local name, so files in the 1.0, 1.1
//! and 2.0 namespaces read the same way.
//!
//! **Horizontal** (`CoordGeom`):
//!
//! - `Units/Metric@linearUnit` or `Units/Imperial@linearUnit`: lengths are converted to metres.
//! - `Alignment@name`, `Alignment@staStart`.
//! - `Line`: `Start`, `End`.
//! - `Curve`: `Start`, `Center`, `End` and `@rot` (`cw` | `ccw`). The radius is the distance from
//!   Start to Center, as the schema says; the length follows from the swept angle.
//! - `Spiral` with `@spiType="clothoid"`: `Start`, `End`, `@length`, `@radiusStart`, `@radiusEnd`
//!   (`INF` for the tangent end) and `@rot`. The start direction is the one that carries a
//!   clothoid of that length and those radii from Start to End.
//!
//! Points are text `northing easting [elevation]`, **northing first**, or a `pntRef` naming a
//! `CgPoint`. They become X = easting, Y = northing.
//!
//! The redundant attributes (`dir`, `dirStart`, `dirEnd`, `delta`, `chord`, `tangent`, `theta`,
//! `totalX`, …) are not read: they repeat what the points say, and the direction attributes
//! depend on unit and bearing conventions that differ between exporters. `length` and `radius`,
//! when present, are compared with the geometry and a note is made when they disagree.
//!
//! **Station equations** (`Alignment/StaEquation`): `@staInternal` (where it is: the station with
//! no equations applied), `@staAhead`, `@staBack` (optional) and `@staIncrement` (`increasing` |
//! `decreasing`, increasing when absent). All three stations are in the file's linear unit. A
//! `staBack` that disagrees by more than 1 mm with the stationing before the equation is noted
//! and the derived value used. If any equation cannot be read, none is imported and a note says so.
//!
//! **Profile** (`Alignment/Profile/ProfAlign`): its children in order, each with the text
//! `station elevation`: `PVI`, `ParaCurve@length`, `UnsymParaCurve@lengthIn @lengthOut`,
//! `CircCurve@radius` (`@length` is only compared with the curve the radius gives, since the
//! schema does not say whether it is the arc or its plan length). Stations and elevations are in
//! the file's linear unit (`Units@elevationUnit` is, by the schema, for elevation *attributes*
//! such as `ellipsoidHeight`, and is not applied). **PVI stations are taken to be internal
//! stations**; the schema does not say, and a note is made when the alignment also has station
//! equations. Only the first `ProfAlign` of an alignment is read; the others are named in a note.
//! `ProfSurf` (existing ground) is not read.
//!
//! **Superelevation** (`Alignment/Superelevation`, one element per curve): the stations
//! `BeginRunoutSta`, `BeginRunoffSta`, `FullSuperSta`, `RunoffSta`, `StartofRunoutSta`,
//! `EndofRunoutSta` (internal stations, by the schema's `station` type), the rate `FullSuperelev`
//! (percent) and `AdverseSE` (`adverse` | `non-adverse`). LandXML 1.2 gives **one number**, the
//! full rate; it gives no cross-fall at the other stations and no normal crown. So:
//!
//! - each `Superelevation` becomes one section holding the stations it states;
//! - `FullSuperSta` and `RunoffSta` get the full rate on both sides, as one plane: the side on
//!   the inside of the horizontal curve falls at the rate and the outside rises at it (the schema:
//!   "the road surface slopes toward the inside of the curve"), the other way round when
//!   `AdverseSE` is `adverse`. The inside is taken from the curvature of the imported horizontal
//!   geometry midway between the two stations. A note records each assignment;
//! - the other four stations are stored with **no cross-fall**, because the file states none. The
//!   cross-fall through the runoff and runout is therefore absent, not interpolated;
//! - when the hand cannot be told (no curvature there), the rate is negative (the schema does not
//!   define a sign), or `FullSuperelev` is missing, no side gets a value and a note says why.
//!
//! No rate is ever computed from speed or radius.
//!
//! A profile, set of station equations or superelevation that cannot be read is left off the
//! alignment with a note; the horizontal alignment is still imported.
//!
//! What is not supported. An alignment containing any of these is left out, with a note saying
//! why, and the rest of the file is still read:
//!
//! - spirals other than clothoids (`bloss`, `cubic`, `sinusoid`, …);
//! - `IrregularLine` and `Chain`;
//! - points given neither as coordinates nor as a resolvable `pntRef`.
//!
//! Ignored silently: `ProfSurf`, `CrossSects`, `Cant`, `AlignPIs`, `Feature`.
//!
//! None of this has been tested against a file exported by Civil 3D.

use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::path::Path;

use crate::geometry::{format_station, integrate_heading, Alignment, AlignmentElement};
use crate::profile::{Profile, Pvi, VerticalCurve};
use crate::stationing::{StationDirection, StationEquation, StationEquations};
use crate::superelevation::{CriticalStation, CrossFallPoint, Superelevation, SuperelevationSection};

#[derive(Debug, thiserror::Error)]
pub enum LandXmlError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not well-formed XML: {0}")]
    Xml(String),
    #[error("not a LandXML file: the root element is <{0}>")]
    NotLandXml(String),
    #[error("unsupported linear unit '{0}'")]
    Unit(String),
}

/// The alignments read from a file, and what the reader has to say about it: alignments left out
/// and why, assumptions, and disagreements between attributes and geometry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandXmlImport {
    pub alignments: Vec<Alignment>,
    pub notes: Vec<String>,
}

/// Reads the alignments of a LandXML file, with their profile, station equations and superelevation.
pub fn read_landxml(path: &Path) -> Result<LandXmlImport, LandXmlError> {
    parse_landxml(&std::fs::read_to_string(path)?)
}

/// Reads the alignments of a LandXML document held in a string, with their profile, station
/// equations and superelevation.
pub fn parse_landxml(text: &str) -> Result<LandXmlImport, LandXmlError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| LandXmlError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "LandXML" {
        return Err(LandXmlError::NotLandXml(root.tag_name().name().to_string()));
    }
    let mut import = LandXmlImport::default();
    let scale = linear_unit(root, &mut import.notes)?;

    // Named points that Start/Center/End may refer to with pntRef.
    let mut points: HashMap<&str, (f64, f64)> = HashMap::new();
    for n in root.descendants().filter(|n| n.has_tag_name_local("CgPoint")) {
        if let (Some(name), Some(p)) = (n.attribute("name"), parse_point_text(n.text().unwrap_or(""))) {
            points.insert(name, p);
        }
    }

    let ctx = Context { scale, points };
    let mut unnamed = 0;
    for group in children(root, "Alignments") {
        for node in children(group, "Alignment") {
            let name = match node.attribute("name") {
                Some(n) if !n.trim().is_empty() => n.to_string(),
                _ => {
                    unnamed += 1;
                    let n = format!("Alignment {unnamed}");
                    import.notes.push(format!("An alignment has no name; called '{n}'."));
                    n
                }
            };
            match read_alignment(node, &name, &ctx, &mut import.notes) {
                Ok(a) => import.alignments.push(a),
                Err(why) => import.notes.push(format!("Alignment '{name}' was not imported: {why}")),
            }
        }
    }
    if import.alignments.is_empty() && import.notes.is_empty() {
        import.notes.push("The file contains no alignments.".to_string());
    }
    Ok(import)
}

struct Context<'a> {
    /// Metres per file unit.
    scale: f64,
    points: HashMap<&'a str, (f64, f64)>,
}

trait LocalName {
    fn has_tag_name_local(&self, name: &str) -> bool;
}

impl LocalName for roxmltree::Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name() == name
    }
}

fn children<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    node.children().filter(move |n| n.has_tag_name_local(name))
}

fn linear_unit(root: roxmltree::Node, notes: &mut Vec<String>) -> Result<f64, LandXmlError> {
    let unit = children(root, "Units")
        .flat_map(|u| u.children().filter(|n| n.has_tag_name_local("Metric") || n.has_tag_name_local("Imperial")))
        .find_map(|n| n.attribute("linearUnit"));
    match unit {
        None => {
            notes.push("The file does not state its linear unit; metres assumed.".to_string());
            Ok(1.0)
        }
        Some("meter") => Ok(1.0),
        Some("millimeter") => Ok(0.001),
        Some("centimeter") => Ok(0.01),
        Some("kilometer") => Ok(1000.0),
        Some("foot") => Ok(0.3048),
        Some("USSurveyFoot") => Ok(1200.0 / 3937.0),
        Some("inch") => Ok(0.0254),
        Some("mile") => Ok(1609.344),
        Some(other) => Err(LandXmlError::Unit(other.to_string())),
    }
}

/// `northing easting [elevation]` → (easting, northing), in file units.
fn parse_point_text(text: &str) -> Option<(f64, f64)> {
    let mut it = text.split_whitespace().map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()));
    let northing = it.next()??;
    let easting = it.next()??;
    Some((easting, northing))
}

fn point(parent: roxmltree::Node, name: &'static str, ctx: &Context) -> Result<(f64, f64), String> {
    let kind = parent.tag_name().name();
    let node = children(parent, name).next().ok_or_else(|| format!("a {kind} has no <{name}> point"))?;
    let text = node.text().unwrap_or("").trim();
    let p = if !text.is_empty() {
        parse_point_text(text).ok_or_else(|| format!("the <{name}> of a {kind} is not 'northing easting': '{text}'"))?
    } else if let Some(r) = node.attribute("pntRef") {
        *ctx.points.get(r).ok_or_else(|| format!("the <{name}> of a {kind} refers to point '{r}', which is not in the file"))?
    } else {
        return Err(format!("the <{name}> of a {kind} has no coordinates"));
    };
    Ok((p.0 * ctx.scale, p.1 * ctx.scale))
}

fn number(node: roxmltree::Node, attr: &str) -> Result<Option<f64>, String> {
    match node.attribute(attr) {
        None => Ok(None),
        Some(t) => t.trim().parse::<f64>().map(Some).map_err(|_| format!("{}@{attr} is not a number: '{t}'", node.tag_name().name())),
    }
}

/// A spiral radius: `INF` (any spelling of infinity) or 0 is the tangent end.
fn spiral_radius(node: roxmltree::Node, attr: &str, scale: f64) -> Result<Option<f64>, String> {
    let text = node.attribute(attr).ok_or_else(|| format!("a Spiral has no {attr}"))?.trim();
    let lower = text.to_ascii_lowercase();
    if matches!(lower.trim_start_matches(['+', '-']), "inf" | "infinity") {
        return Ok(None);
    }
    let r: f64 = text.parse().map_err(|_| format!("Spiral@{attr} is not a number or INF: '{text}'"))?;
    if r.is_nan() {
        return Err(format!("Spiral@{attr} is not a number or INF: '{text}'"));
    }
    Ok((r != 0.0 && r.is_finite()).then_some(r.abs() * scale))
}

/// +1 for `ccw` (left), −1 for `cw` (right).
fn rotation(node: roxmltree::Node) -> Result<f64, String> {
    match node.attribute("rot") {
        Some("ccw") => Ok(1.0),
        Some("cw") => Ok(-1.0),
        Some(other) => Err(format!("{}@rot is '{other}', expected cw or ccw", node.tag_name().name())),
        None => Err(format!("a {} has no rot attribute", node.tag_name().name())),
    }
}

fn read_alignment(node: roxmltree::Node, name: &str, ctx: &Context, notes: &mut Vec<String>) -> Result<Alignment, String> {
    let scale = ctx.scale;
    let start_station_m = match number(node, "staStart")? {
        Some(s) => s * scale,
        None => {
            notes.push(format!("Alignment '{name}' has no staStart; it starts at station 0."));
            0.0
        }
    };
    let mut elements: Vec<AlignmentElement> = Vec::new();
    let mut local_notes: Vec<String> = Vec::new();
    let mut found_geometry = false;
    for geom in children(node, "CoordGeom") {
        found_geometry = true;
        for item in geom.children().filter(|n| n.is_element()) {
            let n = elements.len() + 1;
            let before = elements.len();
            match item.tag_name().name() {
                "Line" => {
                    let (s, e) = (point(item, "Start", ctx)?, point(item, "End", ctx)?);
                    let length = (e.0 - s.0).hypot(e.1 - s.1);
                    if length > 0.0 {
                        elements.push(AlignmentElement::line(s.0, s.1, (e.1 - s.1).atan2(e.0 - s.0), length));
                    }
                }
                "Curve" => {
                    let (s, c, e) = (point(item, "Start", ctx)?, point(item, "Center", ctx)?, point(item, "End", ctx)?);
                    let turn = rotation(item)?;
                    let radius = (s.0 - c.0).hypot(s.1 - c.1);
                    let radius_end = (e.0 - c.0).hypot(e.1 - c.1);
                    if radius <= 0.0 {
                        return Err(format!("element {n} is a Curve whose Start is its Center"));
                    }
                    if (radius - radius_end).abs() > 1e-3 {
                        local_notes.push(format!(
                            "element {n} (Curve): Start is {radius:.4} m from the Center and End {radius_end:.4} m; the Start radius is used."
                        ));
                    }
                    if let Some(r) = number(item, "radius")? {
                        if (r.abs() * scale - radius).abs() > 1e-3 {
                            local_notes.push(format!(
                                "element {n} (Curve): radius=\"{:.4}\" m but Start is {radius:.4} m from the Center; the points are used.",
                                r.abs() * scale
                            ));
                        }
                    }
                    let a0 = (s.1 - c.1).atan2(s.0 - c.0);
                    let a1 = (e.1 - c.1).atan2(e.0 - c.0);
                    let mut sweep = (turn * (a1 - a0)).rem_euclid(TAU);
                    let declared = number(item, "length")?.map(|l| l * scale);
                    if sweep * radius < 1e-9 {
                        // Start and End coincide: nothing, or a full circle if the file says so.
                        sweep = if declared.is_some_and(|l| (l - TAU * radius).abs() < 1e-3 * radius) { TAU } else { 0.0 };
                    }
                    if sweep > 0.0 {
                        let mut arc = AlignmentElement::line(s.0, s.1, a0 + turn * FRAC_PI_2, sweep * radius);
                        arc.curvature_start = turn / radius;
                        arc.curvature_end = turn / radius;
                        elements.push(arc);
                    }
                }
                "Spiral" => {
                    let kind = item.attribute("spiType").unwrap_or("");
                    if kind != "clothoid" {
                        return Err(format!(
                            "element {n} is a spiral of type '{kind}'; only clothoid spirals are supported"
                        ));
                    }
                    let (s, e) = (point(item, "Start", ctx)?, point(item, "End", ctx)?);
                    let turn = rotation(item)?;
                    let length = number(item, "length")?.ok_or_else(|| format!("element {n} is a Spiral with no length"))? * scale;
                    if !(length.is_finite() && length > 0.0) {
                        return Err(format!("element {n} is a Spiral with length {length}"));
                    }
                    let k0 = spiral_radius(item, "radiusStart", scale)?.map_or(0.0, |r| turn / r);
                    let k1 = spiral_radius(item, "radiusEnd", scale)?.map_or(0.0, |r| turn / r);
                    if k0 == k1 {
                        return Err(format!("element {n} is a Spiral with the same radius at both ends"));
                    }
                    // The clothoid in its own frame fixes the chord; the points fix where the chord lies.
                    let (lx, ly) = integrate_heading(0.0, k0, (k1 - k0) / length, length);
                    let chord = (e.0 - s.0).hypot(e.1 - s.1);
                    let heading = (e.1 - s.1).atan2(e.0 - s.0) - ly.atan2(lx);
                    if (chord - lx.hypot(ly)).abs() > 1e-3 {
                        local_notes.push(format!(
                            "element {n} (Spiral): Start to End is {chord:.4} m, but a clothoid of this length and these radii has a chord of {:.4} m; the stated length and radii are used, so it ends {:.4} m from End.",
                            lx.hypot(ly),
                            (chord - lx.hypot(ly)).abs()
                        ));
                    }
                    elements.push(AlignmentElement { x: s.0, y: s.1, heading_rad: heading, length, curvature_start: k0, curvature_end: k1 });
                }
                "IrregularLine" | "Chain" => {
                    return Err(format!("element {n} is a {}, which is not supported", item.tag_name().name()));
                }
                "Feature" => {}
                other => return Err(format!("element {n} is a <{other}>, which is not part of CoordGeom")),
            }
            // The stated length is redundant; say so when it disagrees with the geometry.
            if let (Some(e), true, "Line" | "Curve") = (elements.last(), elements.len() > before, item.tag_name().name()) {
                if let Some(declared) = number(item, "length")? {
                    if (declared * scale - e.length).abs() > 1e-3 {
                        local_notes.push(format!(
                            "element {n} ({}): length=\"{:.4}\" m but its points give {:.4} m; the points are used.",
                            item.tag_name().name(),
                            declared * scale,
                            e.length
                        ));
                    }
                }
            }
        }
    }
    if !found_geometry {
        return Err("it has no CoordGeom".to_string());
    }
    if elements.is_empty() {
        return Err("its CoordGeom has no lines, curves or spirals".to_string());
    }
    let mut alignment = Alignment::from_elements(name, start_station_m, elements).map_err(|e| e.to_string())?;
    alignment.station_equations = read_station_equations(node, scale, &alignment, &mut local_notes);
    alignment.profile = read_profile(node, scale, &alignment, &mut local_notes);
    alignment.superelevation = read_superelevation(node, scale, &alignment, &mut local_notes);

    if let Some(declared) = number(node, "length")? {
        if (declared * scale - alignment.length()).abs() > 1e-3 {
            local_notes.push(format!("length=\"{:.4}\" m but its elements add up to {:.4} m.", declared * scale, alignment.length()));
        }
    }
    for j in alignment.discontinuities(1e-3, 1e-4) {
        local_notes.push(format!(
            "at station {} element {} starts {:.4} m from the end of the one before and turns {:.4}° from it.",
            crate::format_station(j.station_m, 3),
            j.index + 1,
            j.gap_m,
            j.heading_change_rad.to_degrees()
        ));
    }
    notes.extend(local_notes.into_iter().map(|n| format!("Alignment '{name}': {n}")));
    Ok(alignment)
}

/// Agreement asked of redundant values, and how far a profile may fall short of the alignment's
/// ends before it is worth a note: 1 mm.
const TOLERANCE_M: f64 = 1e-3;

fn required(node: roxmltree::Node, attr: &str) -> Result<f64, String> {
    number(node, attr)?.filter(|v| v.is_finite()).ok_or_else(|| format!("a {} has no {attr}", node.tag_name().name()))
}

/// `Alignment/StaEquation`. All or nothing: one unreadable equation and the alignment keeps its
/// internal stations.
fn read_station_equations(node: roxmltree::Node, scale: f64, alignment: &Alignment, notes: &mut Vec<String>) -> StationEquations {
    let read = || -> Result<Vec<StationEquation>, String> {
        let mut out = Vec::new();
        for e in children(node, "StaEquation") {
            let direction = match e.attribute("staIncrement") {
                None | Some("increasing") => StationDirection::Increasing,
                Some("decreasing") => StationDirection::Decreasing,
                Some(other) => return Err(format!("StaEquation@staIncrement is '{other}', expected increasing or decreasing")),
            };
            out.push(StationEquation {
                internal_m: required(e, "staInternal")? * scale,
                back_m: number(e, "staBack")?.map(|b| b * scale),
                ahead_m: required(e, "staAhead")? * scale,
                direction,
            });
        }
        out.sort_by(|a, b| a.internal_m.total_cmp(&b.internal_m));
        Ok(out)
    };
    let mut equations = match read() {
        Ok(equations) => StationEquations { equations },
        Err(why) => {
            notes.push(format!("station equations were not imported: {why}. Stations run continuously from staStart."));
            return StationEquations::default();
        }
    };
    for index in 0..equations.equations.len() {
        let e = equations.equations[index];
        let expected = equations.back_station(index).unwrap_or(e.internal_m);
        if let Some(back) = e.back_m {
            if (back - expected).abs() > TOLERANCE_M {
                notes.push(format!(
                    "the station equation at internal station {} gives staBack {} m, but the stationing before it arrives at {} m; the latter is used.",
                    format_station(e.internal_m, 3),
                    back,
                    expected
                ));
                equations.equations[index].back_m = None;
            }
        }
        if e.internal_m < alignment.start_station_m - TOLERANCE_M || e.internal_m > alignment.end_station_m() + TOLERANCE_M {
            notes.push(format!("the station equation at internal station {} is off the alignment and has no effect.", format_station(e.internal_m, 3)));
        }
    }
    if let Err(why) = equations.validate() {
        notes.push(format!("station equations were not imported: {why}. Stations run continuously from staStart."));
        return StationEquations::default();
    }
    equations
}

/// `station elevation` → (station, elevation), in file units.
fn station_elevation(node: roxmltree::Node) -> Result<(f64, f64), String> {
    let text = node.text().unwrap_or("").trim();
    let mut it = text.split_whitespace().map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()));
    match (it.next().flatten(), it.next().flatten()) {
        (Some(station), Some(elevation)) => Ok((station, elevation)),
        _ => Err(format!("the text of a {} is not 'station elevation': '{text}'", node.tag_name().name())),
    }
}

/// The first `Alignment/Profile/ProfAlign`.
fn read_profile(node: roxmltree::Node, scale: f64, alignment: &Alignment, notes: &mut Vec<String>) -> Option<Profile> {
    let all: Vec<roxmltree::Node> = children(node, "Profile").flat_map(|p| children(p, "ProfAlign")).collect();
    let first = *all.first()?;
    let name = first.attribute("name").unwrap_or("").to_string();
    if all.len() > 1 {
        let others: Vec<String> = all[1..].iter().map(|n| format!("'{}'", n.attribute("name").unwrap_or(""))).collect();
        notes.push(format!("it has {} design profiles (ProfAlign); '{name}' is used and {} ignored.", all.len(), others.join(", ")));
    }
    let read = || -> Result<(Profile, Vec<(usize, f64)>), String> {
        let mut pvis = Vec::new();
        // Stated CircCurve lengths, to compare with what the radius gives: (PVI index, length in metres).
        let mut circular_lengths = Vec::new();
        for item in first.children().filter(|n| n.is_element()) {
            let curve = match item.tag_name().name() {
                "PVI" => VerticalCurve::None,
                "ParaCurve" => VerticalCurve::Parabola { length_m: required(item, "length")? * scale },
                "UnsymParaCurve" => {
                    VerticalCurve::AsymmetricParabola { length_in_m: required(item, "lengthIn")? * scale, length_out_m: required(item, "lengthOut")? * scale }
                }
                "CircCurve" => {
                    if let Some(l) = number(item, "length")? {
                        circular_lengths.push((pvis.len(), l * scale));
                    }
                    VerticalCurve::Circular { radius_m: required(item, "radius")?.abs() * scale }
                }
                "Feature" => continue,
                other => return Err(format!("it contains a <{other}>, which is not part of ProfAlign")),
            };
            let (station, elevation) = station_elevation(item)?;
            pvis.push(Pvi { station_m: station * scale, elevation_m: elevation * scale, curve });
        }
        Ok((Profile::new(&name, pvis).map_err(|e| e.to_string())?, circular_lengths))
    };
    let (profile, circular_lengths) = match read() {
        Ok(read) => read,
        Err(why) => {
            notes.push(format!("profile '{name}' was not imported: {why}"));
            return None;
        }
    };
    let extents = profile.curve_extents().unwrap_or_default();
    for (pvi, declared) in circular_lengths {
        let (Some(extent), VerticalCurve::Circular { radius_m }) = (extents.iter().find(|e| e.pvi == pvi), profile.pvis[pvi].curve) else { continue };
        let plan = extent.end_station_m - extent.start_station_m;
        let turn = match (profile.grade_between(pvi - 1), profile.grade_between(pvi)) {
            (Some(g1), Some(g2)) => (g2.atan() - g1.atan()).abs(),
            _ => continue,
        };
        let arc = radius_m * turn;
        if (declared - plan).abs() > TOLERANCE_M && (declared - arc).abs() > TOLERANCE_M {
            notes.push(format!(
                "profile '{name}': the CircCurve at station {} has length=\"{declared:.4}\" m, but its radius gives a curve {plan:.4} m long on plan ({arc:.4} m along the arc); the radius is used.",
                format_station(profile.pvis[pvi].station_m, 3)
            ));
        }
    }
    if let Some((from, to)) = profile.range() {
        if from > alignment.start_station_m + TOLERANCE_M || to < alignment.end_station_m() - TOLERANCE_M {
            notes.push(format!(
                "profile '{name}' runs from station {} to {}, short of the alignment ({} to {}); there is no level outside the profile.",
                format_station(from, 3),
                format_station(to, 3),
                format_station(alignment.start_station_m, 3),
                format_station(alignment.end_station_m(), 3)
            ));
        }
    }
    if !alignment.station_equations.is_empty() {
        notes.push(format!("profile '{name}': the alignment has station equations; the profile's stations are taken to be internal stations (no equations applied)."));
    }
    Some(profile)
}

/// `Alignment/Superelevation`: one section per element. See the module documentation for what a
/// LandXML superelevation does and does not say.
fn read_superelevation(node: roxmltree::Node, scale: f64, alignment: &Alignment, notes: &mut Vec<String>) -> Option<Superelevation> {
    const STATIONS: [(&str, CriticalStation); 6] = [
        ("BeginRunoutSta", CriticalStation::BeginRunout),
        ("BeginRunoffSta", CriticalStation::BeginRunoff),
        ("FullSuperSta", CriticalStation::FullSuper),
        ("RunoffSta", CriticalStation::Runoff),
        ("StartofRunoutSta", CriticalStation::StartOfRunout),
        ("EndofRunoutSta", CriticalStation::EndOfRunout),
    ];
    /// The one value of a child element; `None` when absent or empty (`xsi:nil`).
    fn single<'a>(parent: roxmltree::Node<'a, '_>, name: &'static str) -> Result<Option<&'a str>, String> {
        let mut values = children(parent, name).filter_map(|n| n.text().map(str::trim)).filter(|t| !t.is_empty());
        let first = values.next();
        if values.next().is_some() {
            return Err(format!("it has more than one {name}"));
        }
        Ok(first)
    }
    fn value(parent: roxmltree::Node, name: &'static str) -> Result<Option<f64>, String> {
        match single(parent, name)? {
            None => Ok(None),
            Some(t) => t.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| format!("its {name} is not a number: '{t}'")),
        }
    }

    let mut sections: Vec<SuperelevationSection> = Vec::new();
    for (n, element) in children(node, "Superelevation").enumerate() {
        let n = n + 1;
        let read = |notes: &mut Vec<String>| -> Result<SuperelevationSection, String> {
            let mut points = Vec::new();
            let mut names = Vec::new();
            for (name, kind) in STATIONS {
                if let Some(station) = value(element, name)? {
                    points.push(CrossFallPoint { station_m: station * scale, left_pct: None, right_pct: None, kind: Some(kind) });
                    names.push(name);
                }
            }
            if points.is_empty() {
                return Err("it states no stations".to_string());
            }
            if let Some(i) = (1..points.len()).find(|&i| points[i].station_m <= points[i - 1].station_m) {
                return Err(format!("its stations are not in order ({} is not after {})", names[i], names[i - 1]));
            }
            let rate = value(element, "FullSuperelev")?;
            let adverse = match single(element, "AdverseSE")? {
                None | Some("non-adverse") => false,
                Some("adverse") => true,
                Some(other) => return Err(format!("its AdverseSE is '{other}', expected adverse or non-adverse")),
            };
            let full: Vec<f64> = points.iter().filter(|p| matches!(p.kind, Some(CriticalStation::FullSuper | CriticalStation::Runoff))).map(|p| p.station_m).collect();
            let unassigned = |why: &str| format!("superelevation {n}: no cross-fall was assigned, because {why}.");
            match (rate, full.as_slice()) {
                (None, _) => notes.push(unassigned("it has no FullSuperelev")),
                (_, []) => notes.push(unassigned("it has neither FullSuperSta nor RunoffSta")),
                (Some(rate), _) if rate < 0.0 => notes.push(unassigned(&format!("its FullSuperelev is negative ({rate} %) and the schema does not say what a sign means"))),
                (Some(rate), stations) => {
                    let middle = (stations[0] + stations[stations.len() - 1]) / 2.0;
                    match alignment.curvature_at(middle) {
                        Some(k) if k != 0.0 => {
                            // The inside of the curve falls, unless the superelevation is adverse.
                            let left_falls = (k > 0.0) != adverse;
                            let (left, right) = if left_falls { (-rate, rate) } else { (rate, -rate) };
                            for p in points.iter_mut().filter(|p| matches!(p.kind, Some(CriticalStation::FullSuper | CriticalStation::Runoff))) {
                                p.left_pct = Some(left);
                                p.right_pct = Some(right);
                            }
                            notes.push(format!(
                                "superelevation {n}: FullSuperelev {rate} % was applied from station {} to {} as left {left:+} %, right {right:+} % (the horizontal curve there turns {}{}); the file gives no cross-fall at its other stations.",
                                format_station(stations[0], 3),
                                format_station(stations[stations.len() - 1], 3),
                                if k > 0.0 { "left" } else { "right" },
                                if adverse { ", and AdverseSE is adverse" } else { "" }
                            ));
                        }
                        Some(_) => notes.push(unassigned(&format!("the horizontal alignment is straight at station {}, so the inside of the curve cannot be told", format_station(middle, 3)))),
                        None => notes.push(unassigned(&format!("station {} is off the alignment", format_station(middle, 3)))),
                    }
                }
            }
            Ok(SuperelevationSection { points })
        };
        match read(notes) {
            Ok(section) => sections.push(section),
            Err(why) => notes.push(format!("superelevation {n} was not imported: {why}.")),
        }
    }
    if sections.is_empty() {
        return None;
    }
    sections.sort_by(|a, b| a.points[0].station_m.total_cmp(&b.points[0].station_m));
    match Superelevation::new(sections) {
        Ok(s) => Some(s),
        Err(why) => {
            notes.push(format!("superelevation was not imported: {why}."));
            None
        }
    }
}
