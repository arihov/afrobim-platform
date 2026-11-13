//! LandXML import of horizontal alignments.
//!
//! Reads `LandXML/Alignments/Alignment/CoordGeom` as laid down by the LandXML 1.2 schema
//! (<http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd>; element and attribute names
//! below are taken from it). Elements are matched by local name, so files in the 1.0, 1.1 and 2.0
//! namespaces read the same way.
//!
//! What is used:
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
//! What is not supported. An alignment containing any of these is left out, with a note saying
//! why, and the rest of the file is still read:
//!
//! - spirals other than clothoids (`bloss`, `cubic`, `sinusoid`, …);
//! - `IrregularLine` and `Chain`;
//! - points given neither as coordinates nor as a resolvable `pntRef`.
//!
//! Ignored, with a note: `StaEquation` (stations then run continuously from `staStart`).
//! Ignored silently: `Profile`, `CrossSects`, `Superelevation`, `Cant`, `AlignPIs`, `Feature`.

use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::path::Path;

use crate::geometry::{integrate_heading, Alignment, AlignmentElement};

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

/// Reads the horizontal alignments of a LandXML file.
pub fn read_landxml(path: &Path) -> Result<LandXmlImport, LandXmlError> {
    parse_landxml(&std::fs::read_to_string(path)?)
}

/// Reads the horizontal alignments of a LandXML document held in a string.
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
    let alignment = Alignment::from_elements(name, start_station_m, elements).map_err(|e| e.to_string())?;

    if children(node, "StaEquation").next().is_some() {
        local_notes.push("station equations are ignored: stations run continuously from staStart.".to_string());
    }
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
