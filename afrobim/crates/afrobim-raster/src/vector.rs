//! Vector GIS exchange for the CAD/BIM application.
//!
//! Reads and writes GeoJSON and KML, reads GPX and geographic CSV point files.
//! Every coordinate in this module is WGS84 geographic: longitude and latitude
//! in degrees, with an optional height in metres. The order is always
//! `[lon, lat, h]`. This module never reprojects; the caller does that before
//! export and after import.

use std::path::Path;

use roxmltree::{Document, Node};
use serde_json::Value;

/// Largest file `read_any` will load, in bytes (64 MB).
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// A geometry in WGS84, with positions as `[lon, lat, h]`.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// A single position.
    Point([f64; 3]),
    /// An open polyline of at least two positions.
    LineString(Vec<[f64; 3]>),
    /// Rings of a polygon: the outer ring first, then any holes. Every ring is closed.
    Polygon(Vec<Vec<[f64; 3]>>),
}

/// One named geometry on a layer, with free-form text properties.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    /// Display name; empty when the source had none.
    pub name: String,
    /// Layer (folder) the feature belongs to.
    pub layer: String,
    /// The geometry, in WGS84.
    pub geometry: Geometry,
    /// Extra attributes, in source order.
    pub properties: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Formats a number with a fixed count of decimals, never producing "-0.000".
fn fmt_fixed(v: f64, dp: usize) -> String {
    let s = format!("{v:.dp$}");
    if let Some(rest) = s.strip_prefix('-') {
        if rest.chars().all(|c| c == '0' || c == '.') {
            return rest.to_string();
        }
    }
    s
}

fn in_wgs84_range(lon: f64, lat: f64) -> bool {
    (-180.0..=180.0).contains(&lon) && (-90.0..=90.0).contains(&lat)
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Closes a ring if it is open and checks it has at least four positions.
fn close_ring(mut ring: Vec<[f64; 3]>) -> Result<Vec<[f64; 3]>, String> {
    if let (Some(first), Some(last)) = (ring.first().copied(), ring.last().copied()) {
        if first != last {
            ring.push(first);
        }
    }
    if ring.len() < 4 {
        return Err("a polygon ring needs at least three distinct positions".to_string());
    }
    Ok(ring)
}

fn check_line(line: Vec<[f64; 3]>) -> Result<Vec<[f64; 3]>, String> {
    if line.len() < 2 {
        return Err("a line needs at least two positions".to_string());
    }
    Ok(line)
}

fn json_string(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// GeoJSON writer
// ---------------------------------------------------------------------------

fn geojson_pos(p: &[f64; 3], with_h: bool) -> String {
    if with_h {
        format!("[{},{},{}]", fmt_fixed(p[0], 8), fmt_fixed(p[1], 8), fmt_fixed(p[2], 3))
    } else {
        format!("[{},{}]", fmt_fixed(p[0], 8), fmt_fixed(p[1], 8))
    }
}

fn geojson_list(points: &[[f64; 3]], with_h: bool) -> String {
    let parts: Vec<String> = points.iter().map(|p| geojson_pos(p, with_h)).collect();
    format!("[{}]", parts.join(","))
}

fn geometry_has_height(g: &Geometry) -> bool {
    let nonzero = |p: &[f64; 3]| (p[2] * 1000.0).round() != 0.0;
    match g {
        Geometry::Point(p) => nonzero(p),
        Geometry::LineString(l) => l.iter().any(nonzero),
        Geometry::Polygon(rings) => rings.iter().any(|r| r.iter().any(nonzero)),
    }
}

/// Writes features as an RFC 7946 FeatureCollection.
///
/// Coordinates are `[lon, lat]` with 8 decimal places; a third element (height,
/// 3 decimal places) is added to every position only if any height in the
/// collection is non-zero. Properties hold `name`, `layer` and then the extra
/// properties (extras called `name` or `layer` are dropped). Coordinates must be
/// finite.
pub fn to_geojson(features: &[Feature]) -> String {
    let with_h = features.iter().any(|f| geometry_has_height(&f.geometry));
    let mut out = String::from("{\"type\":\"FeatureCollection\",\"features\":[");
    for (i, f) in features.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('\n');
        let geom = match &f.geometry {
            Geometry::Point(p) => format!(
                "{{\"type\":\"Point\",\"coordinates\":{}}}",
                geojson_pos(p, with_h)
            ),
            Geometry::LineString(l) => format!(
                "{{\"type\":\"LineString\",\"coordinates\":{}}}",
                geojson_list(l, with_h)
            ),
            Geometry::Polygon(rings) => {
                let parts: Vec<String> = rings.iter().map(|r| geojson_list(r, with_h)).collect();
                format!(
                    "{{\"type\":\"Polygon\",\"coordinates\":[{}]}}",
                    parts.join(",")
                )
            }
        };
        let mut props = format!(
            "\"name\":{},\"layer\":{}",
            json_string(&f.name),
            json_string(&f.layer)
        );
        for (k, v) in &f.properties {
            if k == "name" || k == "layer" {
                continue;
            }
            props.push_str(&format!(",{}:{}", json_string(k), json_string(v)));
        }
        out.push_str(&format!(
            "{{\"type\":\"Feature\",\"properties\":{{{props}}},\"geometry\":{geom}}}"
        ));
    }
    if !features.is_empty() {
        out.push('\n');
    }
    out.push_str("]}\n");
    out
}

// ---------------------------------------------------------------------------
// KML writer
// ---------------------------------------------------------------------------

/// Distinct line colours (RGB), cycled per layer.
const PALETTE: [(u8, u8, u8); 10] = [
    (0xe6, 0x19, 0x4b),
    (0x3c, 0xb4, 0x4b),
    (0x43, 0x63, 0xd8),
    (0xf5, 0x82, 0x31),
    (0x91, 0x1e, 0xb4),
    (0x42, 0xd4, 0xf4),
    (0xf0, 0x32, 0xe6),
    (0xbf, 0xef, 0x45),
    (0x46, 0x99, 0x90),
    (0x9a, 0x63, 0x24),
];

/// KML colour is `aabbggrr`.
fn kml_colour(alpha: u8, rgb: (u8, u8, u8)) -> String {
    format!("{:02x}{:02x}{:02x}{:02x}", alpha, rgb.2, rgb.1, rgb.0)
}

fn kml_pos(p: &[f64; 3]) -> String {
    format!("{},{},{}", fmt_fixed(p[0], 8), fmt_fixed(p[1], 8), fmt_fixed(p[2], 3))
}

fn kml_list(points: &[[f64; 3]]) -> String {
    let parts: Vec<String> = points.iter().map(kml_pos).collect();
    parts.join(" ")
}

fn kml_geometry(g: &Geometry) -> String {
    match g {
        Geometry::Point(p) => format!(
            "<Point><altitudeMode>clampToGround</altitudeMode><coordinates>{}</coordinates></Point>",
            kml_pos(p)
        ),
        Geometry::LineString(l) => format!(
            "<LineString><tessellate>1</tessellate><altitudeMode>clampToGround</altitudeMode><coordinates>{}</coordinates></LineString>",
            kml_list(l)
        ),
        Geometry::Polygon(rings) => {
            let mut s = String::from(
                "<Polygon><tessellate>1</tessellate><altitudeMode>clampToGround</altitudeMode>",
            );
            for (i, ring) in rings.iter().enumerate() {
                let tag = if i == 0 { "outerBoundaryIs" } else { "innerBoundaryIs" };
                s.push_str(&format!(
                    "<{tag}><LinearRing><coordinates>{}</coordinates></LinearRing></{tag}>",
                    kml_list(ring)
                ));
            }
            s.push_str("</Polygon>");
            s
        }
    }
}

/// Writes features as a KML 2.2 document.
///
/// One `<Folder>` per layer in first-seen order, and one `<Style>` per layer with
/// a distinct line colour. Properties go to `<ExtendedData>`. All text is
/// XML-escaped. Heights are written but ignored by viewers because the altitude
/// mode is `clampToGround`.
pub fn to_kml(document_name: &str, features: &[Feature]) -> String {
    let mut layers: Vec<&str> = Vec::new();
    for f in features {
        if !layers.contains(&f.layer.as_str()) {
            layers.push(&f.layer);
        }
    }
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n");
    out.push_str(&format!("<name>{}</name>\n", xml_escape(document_name)));
    for (i, _) in layers.iter().enumerate() {
        let rgb = PALETTE[i % PALETTE.len()];
        out.push_str(&format!(
            "<Style id=\"layer-{i}\"><IconStyle><color>{}</color></IconStyle><LineStyle><color>{}</color><width>2</width></LineStyle><PolyStyle><color>{}</color></PolyStyle></Style>\n",
            kml_colour(0xff, rgb),
            kml_colour(0xff, rgb),
            kml_colour(0x40, rgb)
        ));
    }
    for (i, layer) in layers.iter().enumerate() {
        out.push_str(&format!("<Folder>\n<name>{}</name>\n", xml_escape(layer)));
        for f in features.iter().filter(|f| f.layer == *layer) {
            out.push_str("<Placemark>");
            out.push_str(&format!("<name>{}</name>", xml_escape(&f.name)));
            out.push_str(&format!("<styleUrl>#layer-{i}</styleUrl>"));
            if !f.properties.is_empty() {
                out.push_str("<ExtendedData>");
                for (k, v) in &f.properties {
                    out.push_str(&format!(
                        "<Data name=\"{}\"><value>{}</value></Data>",
                        xml_escape(k),
                        xml_escape(v)
                    ));
                }
                out.push_str("</ExtendedData>");
            }
            out.push_str(&kml_geometry(&f.geometry));
            out.push_str("</Placemark>\n");
        }
        out.push_str("</Folder>\n");
    }
    out.push_str("</Document>\n</kml>\n");
    out
}

// ---------------------------------------------------------------------------
// GeoJSON reader
// ---------------------------------------------------------------------------

fn crs_is_wgs84(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    u.contains("CRS84") || u.ends_with(":4326")
}

fn check_crs(obj: &serde_json::Map<String, Value>) -> Result<(), String> {
    let crs = match obj.get("crs") {
        None | Some(Value::Null) => return Ok(()),
        Some(c) => c,
    };
    let name = crs
        .get("properties")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| crs.to_string());
    if crs_is_wgs84(&name) {
        Ok(())
    } else {
        Err(format!(
            "GeoJSON declares the coordinate reference system \"{name}\", but GeoJSON must be WGS84 (longitude, latitude in degrees). Reproject the file to WGS84 and import it again."
        ))
    }
}

fn parse_position(v: &Value) -> Result<[f64; 3], String> {
    let arr = v.as_array().ok_or("a position must be an array of numbers")?;
    if arr.len() < 2 {
        return Err("a position needs at least longitude and latitude".to_string());
    }
    let num = |i: usize| -> Result<f64, String> {
        arr[i]
            .as_f64()
            .ok_or_else(|| "a position holds a non-numeric value".to_string())
    };
    let lon = num(0)?;
    let lat = num(1)?;
    let h = if arr.len() > 2 { num(2)? } else { 0.0 };
    if !in_wgs84_range(lon, lat) {
        return Err(format!(
            "coordinate [{lon}, {lat}] is outside the WGS84 range (longitude -180 to 180, latitude -90 to 90): the file is probably not in WGS84 (GeoJSON must be WGS84), so reproject it first"
        ));
    }
    Ok([lon, lat, h])
}

fn parse_positions(v: &Value) -> Result<Vec<[f64; 3]>, String> {
    v.as_array()
        .ok_or("coordinates must be an array of positions")?
        .iter()
        .map(parse_position)
        .collect()
}

fn parse_polygon(v: &Value) -> Result<Vec<Vec<[f64; 3]>>, String> {
    let rings = v.as_array().ok_or("polygon coordinates must be an array of rings")?;
    if rings.is_empty() {
        return Err("a polygon needs an outer ring".to_string());
    }
    rings
        .iter()
        .map(|r| close_ring(parse_positions(r)?))
        .collect()
}

fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

struct Meta {
    name: String,
    layer: String,
    props: Vec<(String, String)>,
}

impl Meta {
    fn push(&self, out: &mut Vec<Feature>, geometry: Geometry) {
        out.push(Feature {
            name: self.name.clone(),
            layer: self.layer.clone(),
            geometry,
            properties: self.props.clone(),
        });
    }
}

fn read_geometry(g: &Value, meta: &Meta, out: &mut Vec<Feature>) -> Result<(), String> {
    let kind = g
        .get("type")
        .and_then(Value::as_str)
        .ok_or("geometry has no \"type\" member")?;
    if kind == "GeometryCollection" {
        let geoms = g
            .get("geometries")
            .and_then(Value::as_array)
            .ok_or("GeometryCollection has no \"geometries\" array")?;
        for sub in geoms {
            read_geometry(sub, meta, out)?;
        }
        return Ok(());
    }
    let coords = g
        .get("coordinates")
        .ok_or_else(|| format!("{kind} has no \"coordinates\" member"))?;
    let each = |c: &Value| -> Result<Vec<Value>, String> {
        c.as_array()
            .cloned()
            .ok_or_else(|| format!("{kind} coordinates must be an array"))
    };
    match kind {
        "Point" => meta.push(out, Geometry::Point(parse_position(coords)?)),
        "MultiPoint" => {
            for p in each(coords)? {
                meta.push(out, Geometry::Point(parse_position(&p)?));
            }
        }
        "LineString" => meta.push(out, Geometry::LineString(check_line(parse_positions(coords)?)?)),
        "MultiLineString" => {
            for l in each(coords)? {
                meta.push(out, Geometry::LineString(check_line(parse_positions(&l)?)?));
            }
        }
        "Polygon" => meta.push(out, Geometry::Polygon(parse_polygon(coords)?)),
        "MultiPolygon" => {
            for p in each(coords)? {
                meta.push(out, Geometry::Polygon(parse_polygon(&p)?));
            }
        }
        other => return Err(format!("unsupported GeoJSON geometry type \"{other}\"")),
    }
    Ok(())
}

fn read_feature(f: &Value, out: &mut Vec<Feature>) -> Result<(), String> {
    let geometry = match f.get("geometry") {
        None | Some(Value::Null) => return Ok(()),
        Some(g) => g,
    };
    let mut name = String::new();
    let mut layer = String::from("GeoJSON");
    let mut props = Vec::new();
    if let Some(map) = f.get("properties").and_then(Value::as_object) {
        let mut name_done = false;
        let mut layer_done = false;
        for (k, v) in map {
            let text = match scalar_text(v) {
                Some(t) => t,
                None => continue,
            };
            if !name_done && (k == "name" || k == "Name") {
                name = text;
                name_done = true;
            } else if !layer_done && (k == "layer" || k == "Layer") {
                layer = text;
                layer_done = true;
            } else {
                props.push((k.clone(), text));
            }
        }
    }
    read_geometry(geometry, &Meta { name, layer, props }, out)
}

/// Reads GeoJSON text: a FeatureCollection, a Feature or a bare geometry.
///
/// Multi* geometries are split into one feature each. The name comes from
/// `properties.name` or `Name`, the layer from `properties.layer` or `"GeoJSON"`,
/// and other scalar properties are kept as strings. Coordinates outside the
/// WGS84 range are rejected with a hint that the file is probably projected. A
/// top-level legacy `crs` member that is not WGS84 is rejected with an error
/// naming it (a WGS84 `crs` such as `EPSG:4326` or `CRS84` is accepted).
pub fn from_geojson(text: &str) -> Result<Vec<Feature>, String> {
    let root: Value =
        serde_json::from_str(strip_bom(text)).map_err(|e| format!("GeoJSON parse error: {e}"))?;
    let obj = root.as_object().ok_or("the GeoJSON root must be a JSON object")?;
    check_crs(obj)?;
    let kind = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or("the GeoJSON object has no \"type\" member")?;
    let mut out = Vec::new();
    match kind {
        "FeatureCollection" => {
            let feats = obj
                .get("features")
                .and_then(Value::as_array)
                .ok_or("the FeatureCollection has no \"features\" array")?;
            for f in feats {
                read_feature(f, &mut out)?;
            }
        }
        "Feature" => read_feature(&root, &mut out)?,
        _ => {
            let meta = Meta {
                name: String::new(),
                layer: "GeoJSON".to_string(),
                props: Vec::new(),
            };
            read_geometry(&root, &meta, &mut out)?;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// KML reader
// ---------------------------------------------------------------------------

fn local<'a>(n: Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && local(*c) == name)
}

fn node_text(n: Node<'_, '_>) -> String {
    let s: String = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect();
    s.trim().to_string()
}

fn child_text(n: Node<'_, '_>, name: &str) -> Option<String> {
    child(n, name).map(node_text)
}

fn kml_layer(pm: Node<'_, '_>) -> String {
    let mut doc_name: Option<String> = None;
    for anc in pm.ancestors().skip(1).filter(|a| a.is_element()) {
        match local(anc) {
            "Folder" => {
                if let Some(n) = child_text(anc, "name") {
                    if !n.is_empty() {
                        return n;
                    }
                }
            }
            "Document" if doc_name.is_none() => {
                doc_name = child_text(anc, "name").filter(|n| !n.is_empty());
            }
            _ => {}
        }
    }
    doc_name.unwrap_or_else(|| "KML".to_string())
}

fn kml_properties(pm: Node<'_, '_>) -> Vec<(String, String)> {
    let mut props = Vec::new();
    for ext in pm.children().filter(|c| c.is_element() && local(*c) == "ExtendedData") {
        for d in ext.descendants().filter(|d| d.is_element()) {
            match local(d) {
                "Data" => {
                    if let Some(key) = d.attribute("name") {
                        let value = child_text(d, "value").unwrap_or_default();
                        props.push((key.to_string(), value));
                    }
                }
                "SimpleData" => {
                    if let Some(key) = d.attribute("name") {
                        props.push((key.to_string(), node_text(d)));
                    }
                }
                _ => {}
            }
        }
    }
    props
}

/// Parses whitespace-separated `lon,lat[,h]` tuples. Spaces around the commas
/// inside a tuple are tolerated.
fn parse_kml_coords(text: &str) -> Result<Vec<[f64; 3]>, String> {
    let mut tuples: Vec<String> = Vec::new();
    for tok in text.split_whitespace() {
        match tuples.last_mut() {
            Some(last) if last.ends_with(',') || tok.starts_with(',') => last.push_str(tok),
            _ => tuples.push(tok.to_string()),
        }
    }
    let mut out = Vec::with_capacity(tuples.len());
    for t in &tuples {
        let parts: Vec<&str> = t.split(',').collect();
        if parts.len() < 2 {
            return Err(format!("coordinate \"{t}\" needs at least lon,lat"));
        }
        let num = |s: &str| -> Result<f64, String> {
            s.trim()
                .parse::<f64>()
                .map_err(|_| format!("coordinate \"{t}\" is not numeric"))
        };
        let lon = num(parts[0])?;
        let lat = num(parts[1])?;
        let h = match parts.get(2) {
            Some(s) if !s.trim().is_empty() => num(s)?,
            _ => 0.0,
        };
        if !in_wgs84_range(lon, lat) {
            return Err(format!(
                "coordinate {lon},{lat} is outside the WGS84 range: KML must be WGS84, so the file is probably projected"
            ));
        }
        out.push([lon, lat, h]);
    }
    Ok(out)
}

fn kml_coords_of(n: Node<'_, '_>) -> Result<Vec<[f64; 3]>, String> {
    let c = child(n, "coordinates")
        .ok_or_else(|| format!("<{}> has no <coordinates>", local(n)))?;
    parse_kml_coords(&node_text(c))
}

fn collect_kml_geometries(node: Node<'_, '_>, out: &mut Vec<Geometry>) -> Result<(), String> {
    for c in node.children().filter(|c| c.is_element()) {
        match local(c) {
            "Point" => {
                let p = kml_coords_of(c)?
                    .into_iter()
                    .next()
                    .ok_or("a Point has empty coordinates")?;
                out.push(Geometry::Point(p));
            }
            "LineString" | "LinearRing" => {
                out.push(Geometry::LineString(check_line(kml_coords_of(c)?)?));
            }
            "Polygon" => {
                let ring_of = |boundary: Node<'_, '_>| -> Result<Vec<[f64; 3]>, String> {
                    let ring = child(boundary, "LinearRing")
                        .ok_or("a polygon boundary has no <LinearRing>")?;
                    close_ring(kml_coords_of(ring)?)
                };
                let outer = child(c, "outerBoundaryIs").ok_or("a Polygon has no outerBoundaryIs")?;
                let mut rings = vec![ring_of(outer)?];
                for inner in c
                    .children()
                    .filter(|i| i.is_element() && local(*i) == "innerBoundaryIs")
                {
                    rings.push(ring_of(inner)?);
                }
                out.push(Geometry::Polygon(rings));
            }
            "MultiGeometry" => collect_kml_geometries(c, out)?,
            _ => {}
        }
    }
    Ok(())
}

/// Reads KML text. Tag matching uses local names only, so any namespace works.
///
/// The layer is the name of the nearest enclosing named Folder, else the
/// Document name, else `"KML"`. A MultiGeometry is split into one feature per
/// member. `ExtendedData` `Data/value` and `SimpleData` become properties
/// (values are trimmed). Placemarks with no supported geometry are skipped.
pub fn from_kml(text: &str) -> Result<Vec<Feature>, String> {
    let doc =
        Document::parse(strip_bom(text)).map_err(|e| format!("KML parse error: {e}"))?;
    let root = doc.root_element();
    if !matches!(local(root), "kml" | "Document" | "Folder" | "Placemark") {
        return Err(format!(
            "this is not a KML document (root element is <{}>)",
            local(root)
        ));
    }
    let mut out = Vec::new();
    for pm in doc
        .descendants()
        .filter(|n| n.is_element() && local(*n) == "Placemark")
    {
        let name = child_text(pm, "name").unwrap_or_default();
        let layer = kml_layer(pm);
        let props = kml_properties(pm);
        let mut geoms = Vec::new();
        collect_kml_geometries(pm, &mut geoms)
            .map_err(|e| format!("Placemark \"{name}\": {e}"))?;
        for g in geoms {
            out.push(Feature {
                name: name.clone(),
                layer: layer.clone(),
                geometry: g,
                properties: props.clone(),
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// GPX reader
// ---------------------------------------------------------------------------

fn gpx_point(n: Node<'_, '_>) -> Result<[f64; 3], String> {
    let attr = |key: &str| -> Result<f64, String> {
        n.attribute(key)
            .ok_or_else(|| format!("<{}> has no {key} attribute", local(n)))?
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("<{}> has a non-numeric {key} attribute", local(n)))
    };
    let lat = attr("lat")?;
    let lon = attr("lon")?;
    if !in_wgs84_range(lon, lat) {
        return Err(format!(
            "GPX position lat {lat}, lon {lon} is outside the WGS84 range"
        ));
    }
    let h = match child_text(n, "ele") {
        Some(s) if !s.is_empty() => s
            .parse::<f64>()
            .map_err(|_| format!("<ele> \"{s}\" is not numeric"))?,
        _ => 0.0,
    };
    Ok([lon, lat, h])
}

fn gpx_points(parent: Node<'_, '_>, tag: &str) -> Result<Vec<[f64; 3]>, String> {
    parent
        .children()
        .filter(|c| c.is_element() && local(*c) == tag)
        .map(gpx_point)
        .collect()
}

/// Reads GPX text: waypoints (layer "Waypoints"), one LineString per track
/// segment (layer "Tracks") and routes (layer "Routes"). Segments and routes
/// with fewer than two points are skipped.
pub fn from_gpx(text: &str) -> Result<Vec<Feature>, String> {
    let doc = Document::parse(strip_bom(text)).map_err(|e| format!("GPX parse error: {e}"))?;
    let root = doc.root_element();
    if local(root) != "gpx" {
        return Err(format!(
            "this is not a GPX document (root element is <{}>)",
            local(root)
        ));
    }
    let mut out = Vec::new();
    let mut add = |name: &str, layer: &str, geometry: Geometry| {
        out.push(Feature {
            name: name.to_string(),
            layer: layer.to_string(),
            geometry,
            properties: Vec::new(),
        });
    };
    for n in root.children().filter(|c| c.is_element()) {
        let name = child_text(n, "name").unwrap_or_default();
        match local(n) {
            "wpt" => add(&name, "Waypoints", Geometry::Point(gpx_point(n)?)),
            "trk" => {
                for seg in n
                    .children()
                    .filter(|c| c.is_element() && local(*c) == "trkseg")
                {
                    let pts = gpx_points(seg, "trkpt")?;
                    if pts.len() >= 2 {
                        add(&name, "Tracks", Geometry::LineString(pts));
                    }
                }
            }
            "rte" => {
                let pts = gpx_points(n, "rtept")?;
                if pts.len() >= 2 {
                    add(&name, "Routes", Geometry::LineString(pts));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// CSV reader
// ---------------------------------------------------------------------------

/// Picks the delimiter (comma, semicolon or tab) by counting them outside
/// quotes on the first non-blank line.
fn detect_delimiter(text: &str) -> char {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let (mut comma, mut semi, mut tab) = (0usize, 0usize, 0usize);
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => comma += 1,
            ';' if !quoted => semi += 1,
            '\t' if !quoted => tab += 1,
            _ => {}
        }
    }
    if comma >= semi && comma >= tab {
        ','
    } else if semi >= tab {
        ';'
    } else {
        '\t'
    }
}

/// Splits text into records of fields with the 1-based line each record starts
/// on. Quoted fields may hold delimiters, newlines and `""` escapes. Blank
/// records are dropped.
fn parse_csv_records(text: &str, delim: char) -> Result<Vec<(usize, Vec<String>)>, String> {
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1usize;
    let mut record_line = 1usize;
    let mut quote_line = 1usize;
    let mut chars = text.chars().peekable();

    let mut finish = |record: &mut Vec<String>, field: &mut String, record_line: usize| {
        record.push(std::mem::take(field));
        let done = std::mem::take(record);
        if done.iter().any(|f| !f.trim().is_empty()) {
            records.push((record_line, done));
        }
    };

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                c => field.push(c),
            }
        } else if c == '"' && field.trim().is_empty() {
            field.clear();
            in_quotes = true;
            quote_line = line;
        } else if c == delim {
            record.push(std::mem::take(&mut field));
        } else if c == '\n' {
            finish(&mut record, &mut field, record_line);
            line += 1;
            record_line = line;
        } else if c != '\r' {
            field.push(c);
        }
    }
    if in_quotes {
        return Err(format!(
            "line {quote_line}: a quoted field is never closed"
        ));
    }
    if !field.is_empty() || !record.is_empty() {
        finish(&mut record, &mut field, record_line);
    }
    Ok(records)
}

fn find_column(headers: &[String], names: &[&str]) -> Option<usize> {
    names
        .iter()
        .find_map(|n| headers.iter().position(|h| h == n))
}

fn parse_csv_number(raw: &str, delim: char) -> Option<f64> {
    let s = raw.trim();
    // Semicolon and tab files often use a decimal comma.
    if delim != ',' && s.contains(',') && !s.contains('.') {
        return s.replace(',', ".").parse().ok();
    }
    s.parse().ok()
}

/// Reads a CSV of geographic points. A header row is required.
///
/// The delimiter (comma, semicolon or tab) is detected from the header. Longitude
/// is the column `lon`, `long`, `lng` or `longitude`; latitude is `lat` or
/// `latitude` (case-insensitive, any order). Optional columns are the name
/// (`name`, `id`, `point` or `label`) and the height (`h`, `z`, `elev`,
/// `elevation` or `height`). Other columns with a value are kept as properties.
/// With a non-comma delimiter a decimal comma is accepted. Blank lines are
/// skipped and errors carry the 1-based line number. The layer is `"CSV"`.
pub fn from_csv_points(text: &str) -> Result<Vec<Feature>, String> {
    let text = strip_bom(text);
    let delim = detect_delimiter(text);
    let records = parse_csv_records(text, delim)?;
    let mut iter = records.into_iter();
    let (_, header_fields) = iter.next().ok_or("the CSV file is empty")?;
    let headers: Vec<String> = header_fields
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let lon_col = find_column(&headers, &["lon", "long", "lng", "longitude"]);
    let lat_col = find_column(&headers, &["lat", "latitude"]);
    let (lon_col, lat_col) = match (lon_col, lat_col) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            return Err(format!(
                "no longitude/latitude columns found in the CSV header (expected lon, long, lng or longitude and lat or latitude; found: {}). Projected easting/northing CSV files must be imported through the survey points import instead.",
                header_fields
                    .iter()
                    .map(|h| h.trim())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    let name_col = find_column(&headers, &["name", "id", "point", "label"]);
    let h_col = find_column(&headers, &["h", "z", "elev", "elevation", "height"]);
    let needed = lon_col.max(lat_col) + 1;

    let mut out = Vec::new();
    for (line, fields) in iter {
        if fields.len() < needed {
            return Err(format!(
                "line {line}: expected at least {needed} columns but found {}",
                fields.len()
            ));
        }
        let lon = parse_csv_number(&fields[lon_col], delim)
            .ok_or_else(|| format!("line {line}: longitude \"{}\" is not a number", fields[lon_col].trim()))?;
        let lat = parse_csv_number(&fields[lat_col], delim)
            .ok_or_else(|| format!("line {line}: latitude \"{}\" is not a number", fields[lat_col].trim()))?;
        if !in_wgs84_range(lon, lat) {
            return Err(format!(
                "line {line}: longitude {lon}, latitude {lat} is outside the WGS84 range; check the column order, and note that projected coordinates must go through the survey points import"
            ));
        }
        let h = match h_col.and_then(|c| fields.get(c)) {
            Some(s) if !s.trim().is_empty() => parse_csv_number(s, delim)
                .ok_or_else(|| format!("line {line}: height \"{}\" is not a number", s.trim()))?,
            _ => 0.0,
        };
        let name = name_col
            .and_then(|c| fields.get(c))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let properties = fields
            .iter()
            .enumerate()
            .filter(|(i, v)| {
                *i < header_fields.len()
                    && *i != lon_col
                    && *i != lat_col
                    && Some(*i) != name_col
                    && Some(*i) != h_col
                    && !v.trim().is_empty()
                    && !header_fields[*i].trim().is_empty()
            })
            .map(|(i, v)| (header_fields[i].trim().to_string(), v.trim().to_string()))
            .collect();
        out.push(Feature {
            name,
            layer: "CSV".to_string(),
            geometry: Geometry::Point([lon, lat, h]),
            properties,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// File dispatch
// ---------------------------------------------------------------------------

/// Reads a vector file, choosing the parser from the extension: `geojson` or
/// `json`, `kml`, `gpx`, `csv` or `txt`. Files over 64 MB are refused.
pub fn read_any(path: &Path) -> Result<Vec<Feature>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let parser: fn(&str) -> Result<Vec<Feature>, String> = match ext.as_str() {
        "geojson" | "json" => from_geojson,
        "kml" => from_kml,
        "gpx" => from_gpx,
        "csv" | "txt" => from_csv_points,
        "kmz" => {
            return Err("KMZ is a zipped KML: unzip it and import the doc.kml inside (KMZ support is planned).".to_string());
        }
        other => {
            return Err(format!(
                "unsupported vector file type \".{other}\" (supported: geojson, json, kml, gpx, csv, txt)"
            ));
        }
    };
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{} is {:.1} MB, over the 64 MB limit for vector imports",
            path.display(),
            meta.len() as f64 / (1024.0 * 1024.0)
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{} is not valid UTF-8 text", path.display()))?;
    parser(&text)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn feat(name: &str, layer: &str, geometry: Geometry, props: &[(&str, &str)]) -> Feature {
        Feature {
            name: name.to_string(),
            layer: layer.to_string(),
            geometry,
            properties: props
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn close(a: &[f64; 3], b: &[f64; 3], tol: f64) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
    }

    fn assert_same(a: &[Feature], b: &[Feature], tol: f64) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.name, y.name);
            assert_eq!(x.layer, y.layer);
            assert_eq!(x.properties, y.properties);
            match (&x.geometry, &y.geometry) {
                (Geometry::Point(p), Geometry::Point(q)) => assert!(close(p, q, tol)),
                (Geometry::LineString(p), Geometry::LineString(q)) => {
                    assert_eq!(p.len(), q.len());
                    assert!(p.iter().zip(q).all(|(a, b)| close(a, b, tol)));
                }
                (Geometry::Polygon(p), Geometry::Polygon(q)) => {
                    assert_eq!(p.len(), q.len());
                    for (r, s) in p.iter().zip(q) {
                        assert_eq!(r.len(), s.len());
                        assert!(r.iter().zip(s).all(|(a, b)| close(a, b, tol)));
                    }
                }
                _ => panic!("geometry kinds differ"),
            }
        }
    }

    fn sample() -> Vec<Feature> {
        let outer = vec![
            [30.26, -0.88, 0.0],
            [30.2612, -0.88, 0.0],
            [30.2612, -0.8812, 0.0],
            [30.26, -0.8812, 0.0],
            [30.26, -0.88, 0.0],
        ];
        let hole = vec![
            [30.2603, -0.8803, 0.0],
            [30.2606, -0.8803, 0.0],
            [30.2606, -0.8806, 0.0],
            [30.2603, -0.8806, 0.0],
            [30.2603, -0.8803, 0.0],
        ];
        vec![
            feat("Benchmark", "Control", Geometry::Point([30.26012345, -0.88054321, 1423.5]), &[("class", "A")]),
            feat(
                "Kabale Road",
                "Roads",
                Geometry::LineString(vec![[30.26, -0.88, 1420.0], [30.2605, -0.8809, 1421.25], [30.261, -0.8811, 1422.0]]),
                &[("surface", "asphalt"), ("width_m", "7.5")],
            ),
            feat("Plot 12", "Plots", Geometry::Polygon(vec![outer, hole]), &[("owner", "Mugisha")]),
        ]
    }

    #[test]
    fn geojson_round_trip() {
        let feats = sample();
        let text = to_geojson(&feats);
        let back = from_geojson(&text).unwrap();
        assert_same(&feats, &back, 1e-8);
    }

    #[test]
    fn geojson_height_only_when_nonzero() {
        let flat = vec![feat("P", "L", Geometry::Point([30.26, -0.88, 0.0]), &[])];
        let text = to_geojson(&flat);
        assert!(text.contains("[30.26000000,-0.88000000]"));
        let tall = vec![feat("P", "L", Geometry::Point([30.26, -0.88, 12.3456]), &[])];
        assert!(to_geojson(&tall).contains("[30.26000000,-0.88000000,12.346]"));
    }

    #[test]
    fn kml_round_trip_and_escaping() {
        let name = "A & B <road>";
        let feats = vec![
            feat(name, "Roads & Paths", Geometry::LineString(vec![[30.26, -0.88, 0.0], [30.27, -0.89, 0.0]]), &[("note", "O'Neil said \"go\" <now>"), ("code", "R1")]),
            feat("Gate", "Control", Geometry::Point([30.2601, -0.8801, 1420.5]), &[]),
            feat(
                "Plot",
                "Roads & Paths",
                Geometry::Polygon(vec![
                    vec![[30.0, -1.0, 0.0], [30.1, -1.0, 0.0], [30.1, -1.1, 0.0], [30.0, -1.0, 0.0]],
                    vec![[30.02, -1.02, 0.0], [30.05, -1.02, 0.0], [30.05, -1.05, 0.0], [30.02, -1.02, 0.0]],
                ]),
                &[],
            ),
        ];
        let kml = to_kml("Ntungamo & <co>", &feats);
        assert!(kml.contains("A &amp; B &lt;road&gt;"));
        assert!(!kml.contains("A & B"));
        assert!(kml.contains("O&apos;Neil said &quot;go&quot;"));
        assert!(kml.contains("<tessellate>1</tessellate>"));
        assert!(kml.contains("<altitudeMode>clampToGround</altitudeMode>"));
        // Folders in first-seen order, one style per layer, distinct colours.
        assert!(kml.find("<name>Roads &amp; Paths</name>").unwrap() < kml.find("<name>Control</name>").unwrap());
        assert_eq!(kml.matches("<Style ").count(), 2);
        let back = from_kml(&kml).unwrap();
        // Layer grouping reorders features: Roads first (line, polygon), then Control.
        let expected = vec![feats[0].clone(), feats[2].clone(), feats[1].clone()];
        assert_same(&expected, &back, 1e-8);
        assert_eq!(back[0].name, name);
    }

    #[test]
    fn kml_google_earth_style() {
        let kml = r##"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:gx="http://www.google.com/kml/ext/2.2">
<Document>
  <name>Ntungamo survey.kml</name>
  <Style id="s1"><LineStyle><color>ff0000ff</color></LineStyle></Style>
  <Folder>
    <name>Roads</name>
    <Folder>
      <name>Primary</name>
      <Placemark>
        <name>Kabale Road</name>
        <styleUrl>#s1</styleUrl>
        <ExtendedData>
          <Data name="surface"><value>asphalt</value></Data>
          <SchemaData schemaUrl="#x"><SimpleData name="width_m">7.5</SimpleData></SchemaData>
        </ExtendedData>
        <LineString><tessellate>1</tessellate><coordinates>
          30.2600,-0.8800,1500
          30.2610,-0.8810,1501
        </coordinates></LineString>
      </Placemark>
    </Folder>
  </Folder>
  <Folder>
    <name>Plots</name>
    <Placemark>
      <name>Plot 7</name>
      <MultiGeometry>
        <Point><coordinates>30.27,-0.89,0</coordinates></Point>
        <Polygon>
          <outerBoundaryIs><LinearRing><coordinates>
            30.0,-1.0,0 30.1,-1.0,0 30.1,-1.1,0 30.0,-1.0,0
          </coordinates></LinearRing></outerBoundaryIs>
          <innerBoundaryIs><LinearRing><coordinates>
            30.02,-1.02,0 30.05,-1.02,0 30.05,-1.05,0 30.02,-1.02,0
          </coordinates></LinearRing></innerBoundaryIs>
        </Polygon>
      </MultiGeometry>
    </Placemark>
  </Folder>
  <Placemark><name>Loose</name><Point><coordinates>30.3, -0.9, 0</coordinates></Point></Placemark>
</Document>
</kml>"##;
        let f = from_kml(kml).unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!(f[0].name, "Kabale Road");
        assert_eq!(f[0].layer, "Primary");
        assert_eq!(
            f[0].properties,
            vec![("surface".to_string(), "asphalt".to_string()), ("width_m".to_string(), "7.5".to_string())]
        );
        assert_eq!(f[0].geometry, Geometry::LineString(vec![[30.26, -0.88, 1500.0], [30.261, -0.881, 1501.0]]));
        assert_eq!(f[1].layer, "Plots");
        assert!(matches!(f[1].geometry, Geometry::Point(_)));
        match &f[2].geometry {
            Geometry::Polygon(rings) => {
                assert_eq!(rings.len(), 2);
                assert_eq!(rings[0].len(), 4);
                assert_eq!(rings[1][0], [30.02, -1.02, 0.0]);
            }
            _ => panic!("expected polygon"),
        }
        assert_eq!(f[3].name, "Loose");
        assert_eq!(f[3].layer, "Ntungamo survey.kml");
        assert_eq!(f[3].geometry, Geometry::Point([30.3, -0.9, 0.0]));
    }

    #[test]
    fn kml_layer_fallbacks_and_errors() {
        let bare = r#"<kml><Placemark><name>x</name><Point><coordinates>1,2</coordinates></Point></Placemark></kml>"#;
        assert_eq!(from_kml(bare).unwrap()[0].layer, "KML");
        let bad = r#"<kml><Placemark><name>far</name><Point><coordinates>800000,9900000</coordinates></Point></Placemark></kml>"#;
        let err = from_kml(bad).unwrap_err();
        assert!(err.contains("WGS84") && err.contains("far"));
        assert!(from_kml("<gpx/>").is_err());
    }

    #[test]
    fn gpx_waypoints_tracks_routes() {
        let gpx = r#"<?xml version="1.0"?>
<gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="0.35" lon="32.58"><ele>1190.5</ele><name>Kampala</name></wpt>
  <trk><name>Patrol</name>
    <trkseg>
      <trkpt lat="-0.88" lon="30.26"><ele>1500</ele></trkpt>
      <trkpt lat="-0.881" lon="30.261"></trkpt>
    </trkseg>
    <trkseg>
      <trkpt lat="-0.89" lon="30.27"/>
      <trkpt lat="-0.891" lon="30.271"/>
      <trkpt lat="-0.892" lon="30.272"/>
    </trkseg>
  </trk>
  <rte><name>Route A</name><rtept lat="-0.9" lon="30.3"/><rtept lat="-0.91" lon="30.31"><ele>10</ele></rtept></rte>
</gpx>"#;
        let f = from_gpx(gpx).unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!((f[0].name.as_str(), f[0].layer.as_str()), ("Kampala", "Waypoints"));
        assert_eq!(f[0].geometry, Geometry::Point([32.58, 0.35, 1190.5]));
        assert_eq!(f[1].layer, "Tracks");
        assert_eq!(f[2].layer, "Tracks");
        assert_eq!(f[1].name, "Patrol");
        assert_eq!(f[1].geometry, Geometry::LineString(vec![[30.26, -0.88, 1500.0], [30.261, -0.881, 0.0]]));
        match &f[2].geometry {
            Geometry::LineString(l) => assert_eq!(l.len(), 3),
            _ => panic!(),
        }
        assert_eq!((f[3].name.as_str(), f[3].layer.as_str()), ("Route A", "Routes"));
        assert!(from_gpx("<kml/>").is_err());
        assert!(from_gpx(r#"<gpx><wpt lat="95" lon="1"/></gpx>"#).is_err());
    }

    #[test]
    fn csv_comma_and_semicolon() {
        let comma = "Name,Lon,Lat,Elevation,Remark\nBM1,30.26,-0.88,1500.5,corner\nBM2,30.27,-0.89,,\n\n";
        let f = from_csv_points(comma).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].name, "BM1");
        assert_eq!(f[0].layer, "CSV");
        assert_eq!(f[0].geometry, Geometry::Point([30.26, -0.88, 1500.5]));
        assert_eq!(f[0].properties, vec![("Remark".to_string(), "corner".to_string())]);
        assert_eq!(f[1].geometry, Geometry::Point([30.27, -0.89, 0.0]));

        let semi = "id;longitude;latitude;z\nA;30,26;-0,88;12,5\nB;30.27;-0.89;3\n";
        let f = from_csv_points(semi).unwrap();
        assert_eq!(f[0].geometry, Geometry::Point([30.26, -0.88, 12.5]));
        assert_eq!(f[1].geometry, Geometry::Point([30.27, -0.89, 3.0]));
        assert_eq!(f[1].name, "B");
    }

    #[test]
    fn csv_quoting_and_column_order() {
        let text = "\u{feff}Label,Latitude,Longitude\n\"Gate, north \"\"main\"\"\",-0.88,30.26\r\n\"Multi\nline\",-0.89,30.27\n";
        let f = from_csv_points(text).unwrap();
        assert_eq!(f[0].name, "Gate, north \"main\"");
        // Latitude first in the file must not swap the order.
        assert_eq!(f[0].geometry, Geometry::Point([30.26, -0.88, 0.0]));
        assert_eq!(f[1].name, "Multi\nline");
        assert_eq!(f[1].geometry, Geometry::Point([30.27, -0.89, 0.0]));
        let tab = "name\tlng\tlat\nT\t30.1\t-0.1\n";
        assert_eq!(from_csv_points(tab).unwrap()[0].geometry, Geometry::Point([30.1, -0.1, 0.0]));
    }

    #[test]
    fn csv_errors() {
        let err = from_csv_points("Point,Easting,Northing\nA,800000,9900000\n").unwrap_err();
        assert!(err.contains("survey points import"));
        let err = from_csv_points("name,lat\nA,-0.88\n").unwrap_err();
        assert!(err.contains("survey points import"));
        let err = from_csv_points("name,lon,lat\nA,30.26,-0.88\n\nB,abc,-0.89\n").unwrap_err();
        assert!(err.starts_with("line 4:"), "{err}");
        let err = from_csv_points("name,lon,lat\nA,30.26\n").unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
        let err = from_csv_points("name,lon,lat\nA,800000,9900000\n").unwrap_err();
        assert!(err.starts_with("line 2:") && err.contains("WGS84"), "{err}");
        assert!(from_csv_points("").is_err());
    }

    #[test]
    fn geojson_utm_coordinates_rejected() {
        let text = r#"{"type":"Feature","properties":{"name":"x"},"geometry":{"type":"Point","coordinates":[800000,9900000]}}"#;
        let err = from_geojson(text).unwrap_err();
        assert!(err.contains("WGS84"));
        assert!(err.contains("probably not in WGS84"));
    }

    #[test]
    fn geojson_legacy_crs() {
        let utm = r#"{"type":"FeatureCollection","crs":{"type":"name","properties":{"name":"urn:ogc:def:crs:EPSG::32636"}},"features":[]}"#;
        let err = from_geojson(utm).unwrap_err();
        assert!(err.contains("EPSG::32636"));
        let wgs = r#"{"type":"FeatureCollection","crs":{"type":"name","properties":{"name":"urn:ogc:def:crs:OGC:1.3:CRS84"}},"features":[]}"#;
        assert!(from_geojson(wgs).unwrap().is_empty());
    }

    #[test]
    fn geojson_input_shapes() {
        let multi = r#"{"type":"Feature","properties":{"Name":"Pts","layer":"Survey","n":5,"ok":true,"skip":null,"obj":{"a":1}},
            "geometry":{"type":"MultiPoint","coordinates":[[30.1,-0.1],[30.2,-0.2,5]]}}"#;
        let f = from_geojson(multi).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].name, "Pts");
        assert_eq!(f[0].layer, "Survey");
        assert!(f[0].properties.contains(&("n".to_string(), "5".to_string())));
        assert!(f[0].properties.contains(&("ok".to_string(), "true".to_string())));
        assert_eq!(f[0].properties.len(), 2);
        assert_eq!(f[1].geometry, Geometry::Point([30.2, -0.2, 5.0]));

        let bare = r#"{"type":"MultiPolygon","coordinates":[[[[30,-1],[31,-1],[31,-2]]],[[[40,1],[41,1],[41,2],[40,1]]]]}"#;
        let f = from_geojson(bare).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].layer, "GeoJSON");
        match &f[0].geometry {
            Geometry::Polygon(r) => assert_eq!(r[0].first(), r[0].last()),
            _ => panic!(),
        }

        let mls = r#"{"type":"MultiLineString","coordinates":[[[1,1],[2,2]],[[3,3],[4,4]]]}"#;
        assert_eq!(from_geojson(mls).unwrap().len(), 2);
        let null_geom = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":null}]}"#;
        assert!(from_geojson(null_geom).unwrap().is_empty());
        assert!(from_geojson("not json").is_err());
        assert!(from_geojson(r#"{"type":"Circle"}"#).is_err());
    }

    #[test]
    fn lon_lat_order_is_never_swapped() {
        let kampala = vec![feat("Kampala", "Cities", Geometry::Point([32.58, 0.35, 0.0]), &[])];
        let gj = to_geojson(&kampala);
        assert!(gj.find("[32.58").unwrap() < gj.find("0.35").unwrap());
        assert!(gj.contains("[32.58000000,0.35000000]"));
        let kml = to_kml("Test", &kampala);
        assert!(kml.contains("32.58000000,0.35000000"));
        let back = from_kml(&kml).unwrap();
        assert_eq!(back[0].geometry, Geometry::Point([32.58, 0.35, 0.0]));
    }

    #[test]
    fn negative_zero_is_not_printed() {
        assert_eq!(fmt_fixed(-0.000000001, 8), "0.00000000");
        assert_eq!(fmt_fixed(-1.5, 3), "-1.500");
    }

    #[test]
    fn read_any_dispatch_and_limits() {
        let dir = std::env::temp_dir().join(format!("afrobim_vector_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let csv = dir.join("pts.CSV");
        std::fs::write(&csv, "name,lon,lat\nA,30.1,-0.1\n").unwrap();
        assert_eq!(read_any(&csv).unwrap().len(), 1);

        let gj = dir.join("a.geojson");
        std::fs::write(&gj, to_geojson(&sample())).unwrap();
        assert_eq!(read_any(&gj).unwrap().len(), 3);

        let kmz = dir.join("a.kmz");
        std::fs::write(&kmz, b"PK").unwrap();
        assert_eq!(
            read_any(&kmz).unwrap_err(),
            "KMZ is a zipped KML: unzip it and import the doc.kml inside (KMZ support is planned)."
        );

        let dxf = dir.join("a.dxf");
        assert!(read_any(&dxf).unwrap_err().contains("unsupported"));

        let big = dir.join("big.geojson");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert!(read_any(&big).unwrap_err().contains("64 MB"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
