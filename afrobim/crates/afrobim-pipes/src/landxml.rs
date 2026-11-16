//! LandXML 1.2 `PipeNetworks`: read and write.
//!
//! Worked from the schema <http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd> (fetched
//! 2026-11-16, 225 951 bytes); every element and attribute below is named in it. XML is read with
//! `roxmltree`, the library used by `afrobim-alignment`'s LandXML reader. Elements are matched by
//! local name, so any LandXML namespace version reads the same way.
//!
//! # Mapping
//!
//! | Here | LandXML 1.2 |
//! |---|---|
//! | network name, status, type, alignment name | `PipeNetwork@name`, `@state`, `@pipeNetType`, `@alignmentRef` |
//! | structure centre | `Struct/Center`, text `northing easting` (northing first) |
//! | rim, sump | `Struct@elevRim`, `@elevSump` |
//! | structure inner size | `CircStruct@diameter` or `RectStruct@length @width`; `thickness`, `material` |
//! | structure with no inner size | `InletStruct` (catch basin), `OutletStruct` (headwall, outfall), `Connection` (others) |
//! | pipe inverts | `Struct/Invert@elev @refPipe @flowDir` (`out` at the start structure, `in` at the end) |
//! | pipe shape | `CircPipe@diameter`, `RectPipe@width @height`, `ElliPipe` and `EggPipe@span @height`; `mannings`, `material`, `thickness` |
//! | design flow | `Pipe/PipeFlow@flowIn` |
//! | curved pipe through-point | `Pipe/Center` |
//! | length, slope | `Pipe@length` (plan length) and `@slope`, written, ignored on reading |
//!
//! The schema has no slot for the following, so they are written as `Feature name="afrobim"` with
//! `Property label value` children and read back from there: structure kind, id, part reference,
//! inflow, loss coefficient, wall thickness of structures without a `thickness` attribute;
//! pipe id, part reference, Colebrook-White ks, egg width profile, description; network source
//! and surface name.
//!
//! # Limits
//!
//! * Written in metres: `Units/Metric` with every unit `meter` (flow `cubicMeterSecond`). Read:
//!   metric and imperial linear units (`inch`, `foot`, `USSurveyFoot`, `mile`, `millimeter`,
//!   `centimeter`, `meter`, `kilometer`) are converted to metres; the unit of a diameter, width
//!   or height is that of `diameterUnit`, `widthUnit`, `heightUnit` and of everything else
//!   `linearUnit` (the schema does not say which unit `thickness` or a struct `length` uses; this
//!   crate takes `linearUnit`). Flow is converted from `cubicMeterSecond`, `literPerSecond`,
//!   `literPerMinute` and `cubicFeetSecond`; other flow units are skipped with a note.
//! * Coordinates are written with the shortest text that reads back to the same `f64`, so a
//!   round trip is exact. Files in other units are converted by multiplication, which is not exact.
//! * `Struct@elevRim` is required here; a file without it is refused.
//! * A `Pipe` that holds a `Channel` is skipped with a note (open channels are not pipes here).
//!   `StructFlow`, `PipeFlow` values other than `flowIn`, and `Units` inside `Pipes` or `Structs`
//!   are ignored.
//! * The schema needs at least two structures and one pipe per network and at least one `Invert`
//!   in each `Struct`; the writer refuses a network that would break this, and it refuses
//!   duplicate names, since LandXML refers to structures and pipes by name.
//! * Only plain-number `Center` text and `pntRef` into `CgPoints` are read.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::error::PipesError;
use crate::model::*;

const NS: &str = "http://www.landxml.org/schema/LandXML-1.2";
const FEATURE: &str = "afrobim";

/// Date and time for the `LandXML` root element, which the schema requires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteOptions {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM:SS`.
    pub time: String,
}

impl WriteOptions {
    pub fn new(date: &str, time: &str) -> WriteOptions {
        WriteOptions { date: date.to_string(), time: time.to_string() }
    }

    /// The current UTC date and time.
    pub fn now() -> WriteOptions {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
        // civil date from days since 1970-01-01 (H. Hinnant's algorithm)
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        WriteOptions { date: format!("{y:04}-{m:02}-{d:02}"), time: format!("{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60) }
    }
}

/// What a LandXML file held.
#[derive(Debug, Clone, PartialEq)]
pub struct LandXmlPipes {
    pub networks: Vec<PipeNetwork>,
    /// Things that were skipped or assumed.
    pub notes: Vec<String>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn num(v: f64, what: &str) -> Result<String, PipesError> {
    if v.is_finite() {
        Ok(v.to_string())
    } else {
        Err(PipesError::Xml(format!("{what} is not a finite number")))
    }
}

fn status_text(s: NetworkStatus) -> &'static str {
    match s {
        NetworkStatus::Existing => "existing",
        NetworkStatus::Proposed => "proposed",
        NetworkStatus::Abandoned => "abandoned",
        NetworkStatus::Destroyed => "destroyed",
    }
}

fn type_text(t: NetworkType) -> &'static str {
    match t {
        NetworkType::Storm => "storm",
        NetworkType::Sanitary => "sanitary",
        NetworkType::Water => "water",
        NetworkType::Other => "other",
    }
}

fn property(out: &mut String, indent: &str, label: &str, value: &str) {
    let _ = writeln!(out, "{indent}<Property label=\"{}\" value=\"{}\"/>", esc(label), esc(value));
}

/// Write networks as a LandXML 1.2 document.
pub fn write_landxml_pipes(networks: &[PipeNetwork], options: &WriteOptions) -> Result<String, PipesError> {
    let mut out = String::new();
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(out, "<LandXML xmlns=\"{NS}\" version=\"1.2\" date=\"{}\" time=\"{}\">", esc(&options.date), esc(&options.time));
    let _ = writeln!(
        out,
        "  <Units><Metric areaUnit=\"squareMeter\" linearUnit=\"meter\" volumeUnit=\"cubicMeter\" temperatureUnit=\"celsius\" pressureUnit=\"HPA\" diameterUnit=\"meter\" widthUnit=\"meter\" heightUnit=\"meter\" velocityUnit=\"metersPerSecond\" flowUnit=\"cubicMeterSecond\"/></Units>"
    );
    if networks.is_empty() {
        return Err(PipesError::Xml("the schema needs at least one PipeNetwork".to_string()));
    }
    let _ = writeln!(out, "  <PipeNetworks>");
    for net in networks {
        write_network(&mut out, net)?;
    }
    let _ = writeln!(out, "  </PipeNetworks>");
    let _ = writeln!(out, "</LandXML>");
    Ok(out)
}

fn write_network(out: &mut String, net: &PipeNetwork) -> Result<(), PipesError> {
    let err = |m: String| Err(PipesError::Xml(format!("network '{}': {m}", net.name)));
    if net.structure_count() < 2 || net.pipe_count() < 1 {
        return err("LandXML needs at least two structures and one pipe in a network".to_string());
    }
    for (i, s) in net.structures().enumerate() {
        if net.structures().take(i).any(|o| o.name == s.name) {
            return err(format!("the structure name '{}' is used twice", s.name));
        }
        if net.incoming(s.id).is_empty() && net.outgoing(s.id).is_empty() {
            return err(format!("structure '{}' has no pipe; LandXML needs an Invert in every Struct", s.name));
        }
    }
    for (i, p) in net.pipes().enumerate() {
        if net.pipes().take(i).any(|o| o.name == p.name) {
            return err(format!("the pipe name '{}' is used twice", p.name));
        }
    }
    let mut attrs = format!("name=\"{}\" pipeNetType=\"{}\" state=\"{}\"", esc(&net.name), type_text(net.network_type), status_text(net.status));
    if let Some(a) = &net.alignment_name {
        let _ = write!(attrs, " alignmentRef=\"{}\"", esc(a));
    }
    let _ = writeln!(out, "    <PipeNetwork {attrs}>");
    let _ = writeln!(out, "      <Structs>");
    for s in net.structures() {
        let mut a = format!("name=\"{}\" elevRim=\"{}\"", esc(&s.name), num(s.rim_level_m, "rim level")?);
        if let Some(v) = s.sump_level_m {
            let _ = write!(a, " elevSump=\"{}\"", num(v, "sump level")?);
        }
        if let Some(d) = &s.description {
            let _ = write!(a, " desc=\"{}\"", esc(d));
        }
        let _ = writeln!(out, "        <Struct {a}>");
        let _ = writeln!(out, "          <Center>{} {}</Center>", num(s.northing_m, "northing")?, num(s.easting_m, "easting")?);
        let mut wall_in_feature = s.wall_thickness_m;
        let mut extra = String::new();
        if let Some(t) = s.wall_thickness_m {
            let _ = write!(extra, " thickness=\"{}\"", num(t, "wall thickness")?);
        }
        if let Some(k) = s.loss_coefficient {
            num(k, "loss coefficient")?;
        }
        match s.inner {
            Some(StructureInner::Circular { diameter_m }) => {
                let _ = writeln!(out, "          <CircStruct diameter=\"{}\"{extra}/>", num(diameter_m, "diameter")?);
                wall_in_feature = None;
            }
            Some(StructureInner::Rectangular { length_m, width_m }) => {
                let _ = writeln!(out, "          <RectStruct length=\"{}\" width=\"{}\"{extra}/>", num(length_m, "length")?, num(width_m, "width")?);
                wall_in_feature = None;
            }
            None => {
                let tag = match s.kind {
                    StructureKind::CatchBasin => "InletStruct",
                    StructureKind::Headwall | StructureKind::Outfall => "OutletStruct",
                    _ => "Connection",
                };
                let _ = writeln!(out, "          <{tag}/>");
            }
        }
        for p in net.pipes().filter(|p| p.start == s.id) {
            let _ = writeln!(out, "          <Invert elev=\"{}\" flowDir=\"out\" refPipe=\"{}\"/>", num(p.start_invert_m, "invert")?, esc(&p.name));
        }
        for p in net.pipes().filter(|p| p.end == s.id) {
            let _ = writeln!(out, "          <Invert elev=\"{}\" flowDir=\"in\" refPipe=\"{}\"/>", num(p.end_invert_m, "invert")?, esc(&p.name));
        }
        let _ = writeln!(out, "          <Feature name=\"{FEATURE}\">");
        property(out, "            ", "id", &s.id.0.to_string());
        property(out, "            ", "kind", s.kind.key());
        if let Some(p) = &s.part {
            property(out, "            ", "part", p);
        }
        if s.inflow_m3_s != 0.0 {
            property(out, "            ", "inflow_m3_s", &num(s.inflow_m3_s, "inflow")?);
        }
        if let Some(k) = s.loss_coefficient {
            property(out, "            ", "loss_coefficient", &k.to_string());
        }
        if let Some(t) = wall_in_feature {
            property(out, "            ", "wall_thickness_m", &num(t, "wall thickness")?);
        }
        let _ = writeln!(out, "          </Feature>");
        let _ = writeln!(out, "        </Struct>");
    }
    let _ = writeln!(out, "      </Structs>");
    let _ = writeln!(out, "      <Pipes>");
    for p in net.pipes() {
        let (sa, sb) = (net.structure(p.start)?, net.structure(p.end)?);
        let l = net.plan_length_m(p.id)?;
        let mut a = format!("name=\"{}\" refStart=\"{}\" refEnd=\"{}\" length=\"{}\"", esc(&p.name), esc(&sa.name), esc(&sb.name), num(l, "length")?);
        if l > 0.0 {
            let _ = write!(a, " slope=\"{}\"", num((p.start_invert_m - p.end_invert_m) / l, "slope")?);
        }
        if let Some(d) = &p.description {
            let _ = write!(a, " desc=\"{}\"", esc(d));
        }
        let _ = writeln!(out, "        <Pipe {a}>");
        let mut common = String::new();
        if let Some(Roughness::Manning { n }) = p.roughness {
            let _ = write!(common, " mannings=\"{}\"", num(n, "Manning n")?);
        }
        if let Some(m) = &p.material {
            let _ = write!(common, " material=\"{}\"", esc(m));
        }
        if let Some(t) = p.wall_thickness_m {
            let _ = write!(common, " thickness=\"{}\"", num(t, "wall thickness")?);
        }
        match &p.shape {
            PipeShape::Circular { diameter_m } => {
                let _ = writeln!(out, "          <CircPipe diameter=\"{}\"{common}/>", num(*diameter_m, "diameter")?);
            }
            PipeShape::Box { width_m, height_m } => {
                let _ = writeln!(out, "          <RectPipe height=\"{}\" width=\"{}\"{common}/>", num(*height_m, "height")?, num(*width_m, "width")?);
            }
            PipeShape::Egg { width_m, height_m, .. } => {
                let _ = writeln!(out, "          <EggPipe height=\"{}\" span=\"{}\"{common}/>", num(*height_m, "height")?, num(*width_m, "width")?);
            }
            PipeShape::Elliptical { width_m, height_m } => {
                let _ = writeln!(out, "          <ElliPipe height=\"{}\" span=\"{}\"{common}/>", num(*height_m, "height")?, num(*width_m, "width")?);
            }
        }
        if let Some(q) = p.design_flow_m3_s {
            let _ = writeln!(out, "          <PipeFlow flowIn=\"{}\"/>", num(q, "design flow")?);
        }
        if let Some(v) = p.via {
            let _ = writeln!(out, "          <Center>{} {}</Center>", num(v.northing_m, "northing")?, num(v.easting_m, "easting")?);
        }
        let _ = writeln!(out, "          <Feature name=\"{FEATURE}\">");
        property(out, "            ", "id", &p.id.0.to_string());
        if let Some(part) = &p.part {
            property(out, "            ", "part", part);
        }
        if let Some(Roughness::ColebrookWhite { ks_m }) = p.roughness {
            property(out, "            ", "ks_m", &num(ks_m, "ks")?);
        }
        if let PipeShape::Egg { profile, .. } = &p.shape {
            if !profile.is_empty() {
                let text: Vec<String> = profile.iter().map(|q| format!("{} {}", q[0], q[1])).collect();
                property(out, "            ", "egg_profile", &text.join(" "));
            }
        }
        let _ = writeln!(out, "          </Feature>");
        let _ = writeln!(out, "        </Pipe>");
    }
    let _ = writeln!(out, "      </Pipes>");
    if net.source.is_some() || net.surface_name.is_some() {
        let _ = writeln!(out, "      <Feature name=\"{FEATURE}\">");
        if let Some(s) = &net.source {
            property(out, "        ", "source", s);
        }
        if let Some(s) = &net.surface_name {
            property(out, "        ", "surface_name", s);
        }
        let _ = writeln!(out, "      </Feature>");
    }
    let _ = writeln!(out, "    </PipeNetwork>");
    Ok(())
}

// ----------------------------------------------------------------------------- reading

type Node<'a, 'i> = roxmltree::Node<'a, 'i>;

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn kid<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> Option<Node<'a, 'i>> {
    kids(n, name).next()
}

fn attr_f64(n: Node, name: &str, what: &str) -> Result<Option<f64>, PipesError> {
    match n.attribute(name) {
        None => Ok(None),
        Some(t) => t.trim().parse::<f64>().map(Some).map_err(|_| PipesError::Xml(format!("{what}: attribute {name}=\"{t}\" is not a number"))),
    }
}

fn linear_factor(unit: &str) -> Option<f64> {
    Some(match unit {
        "millimeter" => 0.001,
        "centimeter" => 0.01,
        "meter" => 1.0,
        "kilometer" => 1000.0,
        "inch" => 0.0254,
        "foot" => 0.3048,
        "USSurveyFoot" => 1200.0 / 3937.0,
        "mile" => 1609.344,
        _ => return None,
    })
}

struct Units {
    linear: f64,
    diameter: f64,
    width: f64,
    height: f64,
    flow: Option<f64>,
}

fn read_units(root: Node, notes: &mut Vec<String>) -> Result<Units, PipesError> {
    let Some(units) = kid(root, "Units") else {
        notes.push("no Units element: lengths are taken as metres".to_string());
        return Ok(Units { linear: 1.0, diameter: 1.0, width: 1.0, height: 1.0, flow: Some(1.0) });
    };
    let Some(u) = units.children().find(|c| c.is_element() && matches!(c.tag_name().name(), "Metric" | "Imperial")) else {
        return Err(PipesError::Xml("Units holds neither Metric nor Imperial".to_string()));
    };
    let metric = u.tag_name().name() == "Metric";
    let lin = u.attribute("linearUnit").ok_or_else(|| PipesError::Xml("Units has no linearUnit".to_string()))?;
    let linear = linear_factor(lin).ok_or_else(|| PipesError::Xml(format!("unknown linearUnit '{lin}'")))?;
    let pick = |name: &str, notes: &mut Vec<String>| -> Result<f64, PipesError> {
        match u.attribute(name) {
            None => {
                notes.push(format!("{name} is not given: the linear unit is used"));
                Ok(linear)
            }
            Some(t) => linear_factor(t).ok_or_else(|| PipesError::Xml(format!("unknown {name} '{t}'"))),
        }
    };
    let (diameter, width, height) = (pick("diameterUnit", notes)?, pick("widthUnit", notes)?, pick("heightUnit", notes)?);
    let flow = match (metric, u.attribute("flowUnit")) {
        (_, None) => {
            notes.push("flowUnit is not given: flows are taken as cubic metres per second".to_string());
            if metric {
                Some(1.0)
            } else {
                None
            }
        }
        (true, Some("cubicMeterSecond")) => Some(1.0),
        (true, Some("literPerSecond")) => Some(0.001),
        (true, Some("literPerMinute")) => Some(1.0 / 60_000.0),
        (false, Some("cubicFeetSecond")) => Some(0.3048_f64.powi(3)),
        (_, Some(other)) => {
            notes.push(format!("flowUnit '{other}' is not converted: design flows are skipped"));
            None
        }
    };
    Ok(Units { linear, diameter, width, height, flow })
}

fn features(n: Node) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for f in kids(n, "Feature").filter(|f| f.attribute("name") == Some(FEATURE)) {
        for p in kids(f, "Property") {
            if let (Some(l), Some(v)) = (p.attribute("label"), p.attribute("value")) {
                m.insert(l.to_string(), v.to_string());
            }
        }
    }
    m
}

fn parse_point(text: &str, what: &str) -> Result<(f64, f64), PipesError> {
    let v: Vec<f64> = text.split_whitespace().map(|t| t.parse::<f64>()).collect::<Result<_, _>>().map_err(|_| PipesError::Xml(format!("{what}: '{}' is not 'northing easting'", text.trim())))?;
    if v.len() < 2 {
        return Err(PipesError::Xml(format!("{what}: '{}' needs a northing and an easting", text.trim())));
    }
    Ok((v[0], v[1]))
}

/// A `Center` or other point element as `(northing, easting)` in the file's unit.
fn point_of(n: Node, cg: &HashMap<String, (f64, f64)>, what: &str) -> Result<(f64, f64), PipesError> {
    let text: String = n.children().filter(|c| c.is_text()).map(|c| c.text().unwrap_or("")).collect();
    if !text.trim().is_empty() {
        return parse_point(&text, what);
    }
    if let Some(r) = n.attribute("pntRef") {
        return cg.get(r).copied().ok_or_else(|| PipesError::Xml(format!("{what}: pntRef '{r}' is not a CgPoint")));
    }
    Err(PipesError::Xml(format!("{what}: no coordinates")))
}

/// Read every `PipeNetwork` of a LandXML document.
pub fn parse_landxml_pipes(text: &str) -> Result<LandXmlPipes, PipesError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| PipesError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "LandXML" {
        return Err(PipesError::Xml(format!("the root element is '{}', not LandXML", root.tag_name().name())));
    }
    let mut notes = Vec::new();
    let units = read_units(root, &mut notes)?;
    let mut cg: HashMap<String, (f64, f64)> = HashMap::new();
    for p in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "CgPoint") {
        if let Some(name) = p.attribute("name") {
            let t: String = p.children().filter(|c| c.is_text()).map(|c| c.text().unwrap_or("")).collect();
            if let Ok(pt) = parse_point(&t, "CgPoint") {
                cg.insert(name.to_string(), pt);
            }
        }
    }
    let mut networks = Vec::new();
    for pn in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "PipeNetworks") {
        for net in kids(pn, "PipeNetwork") {
            networks.push(read_network(net, &units, &cg, &mut notes)?);
        }
    }
    if networks.is_empty() {
        notes.push("the file has no PipeNetwork".to_string());
    }
    Ok(LandXmlPipes { networks, notes })
}

/// Read a LandXML file's pipe networks from disk.
pub fn read_landxml_pipes(path: &std::path::Path) -> Result<LandXmlPipes, PipesError> {
    let text = std::fs::read_to_string(path).map_err(|e| PipesError::Xml(format!("{}: {e}", path.display())))?;
    parse_landxml_pipes(&text)
}

fn read_network(n: Node, u: &Units, cg: &HashMap<String, (f64, f64)>, notes: &mut Vec<String>) -> Result<PipeNetwork, PipesError> {
    let name = n.attribute("name").ok_or_else(|| PipesError::Xml("a PipeNetwork has no name".to_string()))?;
    let mut net = PipeNetwork::new(name);
    net.status = match n.attribute("state") {
        Some("existing") => NetworkStatus::Existing,
        Some("abandoned") => NetworkStatus::Abandoned,
        Some("destroyed") => NetworkStatus::Destroyed,
        _ => NetworkStatus::Proposed,
    };
    net.network_type = match n.attribute("pipeNetType") {
        Some("storm") => NetworkType::Storm,
        Some("sanitary") => NetworkType::Sanitary,
        Some("water") => NetworkType::Water,
        _ => NetworkType::Other,
    };
    net.alignment_name = n.attribute("alignmentRef").map(str::to_string);
    let nf = features(n);
    net.source = nf.get("source").cloned();
    net.surface_name = nf.get("surface_name").cloned();
    let ctx = |m: String| PipesError::Xml(format!("network '{name}': {m}"));
    // structures
    struct Inv {
        elev: f64,
        dir: String,
        pipe: String,
    }
    let mut by_name: HashMap<String, StructureId> = HashMap::new();
    let mut inverts: HashMap<StructureId, Vec<Inv>> = HashMap::new();
    let mut next_auto = 1u32;
    let structs: Vec<Node> = kid(n, "Structs").map(|s| kids(s, "Struct").collect()).unwrap_or_default();
    let mut used_ids: Vec<u32> = Vec::new();
    for s in &structs {
        if let Some(i) = features(*s).get("id").and_then(|t| t.parse::<u32>().ok()) {
            used_ids.push(i);
        }
    }
    for s in structs {
        let sname = s.attribute("name").ok_or_else(|| ctx("a Struct has no name".to_string()))?;
        let f = features(s);
        let id = match f.get("id").and_then(|t| t.parse::<u32>().ok()) {
            Some(i) => i,
            None => {
                while used_ids.contains(&next_auto) {
                    next_auto += 1;
                }
                used_ids.push(next_auto);
                next_auto
            }
        };
        let rim = attr_f64(s, "elevRim", &format!("Struct {sname}"))?.ok_or_else(|| ctx(format!("Struct {sname} has no elevRim")))? * u.linear;
        let sump = attr_f64(s, "elevSump", sname)?.map(|v| v * u.linear);
        let center = kid(s, "Center").ok_or_else(|| ctx(format!("Struct {sname} has no Center")))?;
        let (north, east) = point_of(center, cg, &format!("Struct {sname}"))?;
        let mut kind = StructureKind::Junction;
        let mut inner = None;
        let mut wall = None;
        if let Some(c) = kid(s, "CircStruct") {
            kind = StructureKind::Manhole;
            let d = attr_f64(c, "diameter", sname)?.ok_or_else(|| ctx(format!("CircStruct of {sname} has no diameter")))?;
            inner = Some(StructureInner::Circular { diameter_m: d * u.diameter });
            wall = attr_f64(c, "thickness", sname)?.map(|v| v * u.linear);
        } else if let Some(c) = kid(s, "RectStruct") {
            kind = StructureKind::Manhole;
            let l = attr_f64(c, "length", sname)?.ok_or_else(|| ctx(format!("RectStruct of {sname} has no length")))?;
            let w = attr_f64(c, "width", sname)?.ok_or_else(|| ctx(format!("RectStruct of {sname} has no width")))?;
            inner = Some(StructureInner::Rectangular { length_m: l * u.linear, width_m: w * u.width });
            wall = attr_f64(c, "thickness", sname)?.map(|v| v * u.linear);
        } else if kid(s, "InletStruct").is_some() {
            kind = StructureKind::CatchBasin;
        } else if kid(s, "OutletStruct").is_some() {
            kind = StructureKind::Outfall;
        }
        if let Some(k) = f.get("kind").and_then(|k| StructureKind::from_key(k)) {
            kind = k;
        }
        if wall.is_none() {
            wall = f.get("wall_thickness_m").and_then(|t| t.parse::<f64>().ok());
        }
        let sid = StructureId(id);
        net.insert_structure(Structure {
            id: sid,
            name: sname.to_string(),
            kind,
            easting_m: east * u.linear,
            northing_m: north * u.linear,
            rim_level_m: rim,
            sump_level_m: sump,
            inner,
            wall_thickness_m: wall,
            part: f.get("part").cloned(),
            inflow_m3_s: f.get("inflow_m3_s").and_then(|t| t.parse().ok()).unwrap_or(0.0),
            loss_coefficient: f.get("loss_coefficient").and_then(|t| t.parse().ok()),
            description: s.attribute("desc").map(str::to_string),
        })?;
        by_name.insert(sname.to_string(), sid);
        let mut list = Vec::new();
        for i in kids(s, "Invert") {
            list.push(Inv {
                elev: attr_f64(i, "elev", sname)?.ok_or_else(|| ctx(format!("an Invert of {sname} has no elev")))? * u.linear,
                dir: i.attribute("flowDir").unwrap_or("both").to_string(),
                pipe: i.attribute("refPipe").unwrap_or("").to_string(),
            });
        }
        inverts.insert(sid, list);
    }
    // pipes
    let pipes: Vec<Node> = kid(n, "Pipes").map(|s| kids(s, "Pipe").collect()).unwrap_or_default();
    let mut used_pipe_ids: Vec<u32> = pipes.iter().filter_map(|p| features(*p).get("id").and_then(|t| t.parse::<u32>().ok())).collect();
    let mut next_pipe_auto = 1u32;
    for p in pipes {
        let pname = p.attribute("name").ok_or_else(|| ctx("a Pipe has no name".to_string()))?;
        let f = features(p);
        let find = |attr: &str| -> Result<StructureId, PipesError> {
            let r = p.attribute(attr).ok_or_else(|| ctx(format!("Pipe {pname} has no {attr}")))?;
            by_name.get(r).copied().ok_or_else(|| ctx(format!("Pipe {pname}: {attr} '{r}' is not a Struct")))
        };
        if kid(p, "Channel").is_some() {
            notes.push(format!("network '{name}': pipe {pname} is a Channel and was skipped"));
            continue;
        }
        let (start, end) = (find("refStart")?, find("refEnd")?);
        let invert_of = |sid: StructureId, dirs: [&str; 2]| -> Result<f64, PipesError> {
            inverts
                .get(&sid)
                .and_then(|l| l.iter().find(|i| i.pipe == pname && (i.dir == dirs[0] || i.dir == dirs[1])))
                .map(|i| i.elev)
                .ok_or_else(|| ctx(format!("pipe {pname} has no Invert (flowDir {}) in its structure", dirs[0])))
        };
        let (si, ei) = (invert_of(start, ["out", "both"])?, invert_of(end, ["in", "both"])?);
        let mut shape = None;
        let mut common: Option<Node> = None;
        let mut wall = None;
        let w = |n: Node, a: &str, k: f64| attr_f64(n, a, pname).map(|v| v.map(|v| v * k));
        if let Some(c) = kid(p, "CircPipe") {
            shape = Some(PipeShape::Circular { diameter_m: w(c, "diameter", u.diameter)?.ok_or_else(|| ctx(format!("CircPipe of {pname} has no diameter")))? });
            common = Some(c);
        } else if let Some(c) = kid(p, "RectPipe") {
            shape = Some(PipeShape::Box { width_m: w(c, "width", u.width)?.ok_or_else(|| ctx(format!("RectPipe of {pname} has no width")))?, height_m: w(c, "height", u.height)?.ok_or_else(|| ctx(format!("RectPipe of {pname} has no height")))? });
            common = Some(c);
        } else if let Some(c) = kid(p, "ElliPipe") {
            shape = Some(PipeShape::Elliptical { width_m: w(c, "span", u.width)?.ok_or_else(|| ctx(format!("ElliPipe of {pname} has no span")))?, height_m: w(c, "height", u.height)?.ok_or_else(|| ctx(format!("ElliPipe of {pname} has no height")))? });
            common = Some(c);
        } else if let Some(c) = kid(p, "EggPipe") {
            let profile = f
                .get("egg_profile")
                .map(|t| {
                    let v: Vec<f64> = t.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    v.chunks(2).filter(|c| c.len() == 2).map(|c| [c[0], c[1]]).collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if profile.is_empty() {
                notes.push(format!("network '{name}': egg pipe {pname} has no width profile in the file, so its hydraulics will not be computed"));
            }
            shape = Some(PipeShape::Egg { width_m: w(c, "span", u.width)?.ok_or_else(|| ctx(format!("EggPipe of {pname} has no span")))?, height_m: w(c, "height", u.height)?.ok_or_else(|| ctx(format!("EggPipe of {pname} has no height")))?, profile });
            common = Some(c);
        }
        let shape = shape.ok_or_else(|| ctx(format!("pipe {pname} has no CircPipe, RectPipe, ElliPipe or EggPipe")))?;
        let mut roughness = f.get("ks_m").and_then(|t| t.parse::<f64>().ok()).map(|ks_m| Roughness::ColebrookWhite { ks_m });
        let mut material = None;
        if let Some(c) = common {
            if let Some(n) = attr_f64(c, "mannings", pname)? {
                roughness = Some(Roughness::Manning { n });
            }
            material = c.attribute("material").map(str::to_string);
            wall = w(c, "thickness", u.linear)?;
            if c.attribute("hazenWilliams").is_some() {
                notes.push(format!("network '{name}': pipe {pname} has a Hazen-Williams coefficient, which is not used"));
            }
        }
        let design_flow = match (kid(p, "PipeFlow"), u.flow) {
            (Some(pf), Some(k)) => attr_f64(pf, "flowIn", pname)?.map(|q| q * k),
            _ => None,
        };
        let via = match kid(p, "Center") {
            Some(c) => {
                let (north, east) = point_of(c, cg, &format!("Pipe {pname}"))?;
                Some(PipeVia { easting_m: east * u.linear, northing_m: north * u.linear })
            }
            None => None,
        };
        let id = match f.get("id").and_then(|t| t.parse::<u32>().ok()) {
            Some(i) => i,
            None => {
                while used_pipe_ids.contains(&next_pipe_auto) {
                    next_pipe_auto += 1;
                }
                used_pipe_ids.push(next_pipe_auto);
                next_pipe_auto
            }
        };
        net.insert_pipe(Pipe {
            id: PipeId(id),
            name: pname.to_string(),
            start,
            end,
            start_invert_m: si,
            end_invert_m: ei,
            shape,
            wall_thickness_m: wall,
            material,
            roughness,
            part: f.get("part").cloned(),
            design_flow_m3_s: design_flow,
            via,
            description: p.attribute("desc").map(str::to_string),
        })?;
    }
    Ok(net)
}
