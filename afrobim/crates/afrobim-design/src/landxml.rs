//! LandXML 1.2 export of an alignment with its profile and station equations.
//!
//! Element and attribute names were checked against the schema retrieved from
//! <http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd> and the output validated against
//! it with `xmllint --schema` (see the README for the command). What is written:
//!
//! - root `LandXML` with the required `date`, `time`, `version="1.2"`, the namespace, and
//!   `Units/Metric` with every required unit attribute (metres; the schema requires them);
//! - `Application` naming this crate;
//! - `Alignments/Alignment` with the required `name`, `length`, `staStart`;
//! - `CoordGeom` with `Line` (`Start`, `End`), `Curve` (`Start`, `Center`, `End`, `PI`, `rot`,
//!   `radius`, `length`, `delta`) and `Spiral` (`Start`, `PI`, `End`, `rot`, `spiType="clothoid"`,
//!   `length`, `radiusStart`, `radiusEnd`, with `INF` for the tangent end). The schema requires a
//!   `PI` in a `Spiral`: it is the intersection of the tangents at the two ends. Points are text
//!   `northing easting` (northing first, as the schema's point type has it);
//! - `StaEquation` (`staInternal`, `staAhead`, `staBack`, `staIncrement`) for each station equation;
//! - `Profile/ProfAlign` with `PVI`, `ParaCurve@length`, `UnsymParaCurve@lengthIn/lengthOut` and
//!   `CircCurve@radius/length`, each with text `station elevation`.
//!
//! Numbers are written with Rust's shortest representation that reads back to the same `f64`:
//! nothing is rounded, so coordinates of UTM size (easting 450 000 m, northing 9 960 000 m) come
//! back bit for bit.
//!
//! [`write_landxml`] writes one alignment (as always). [`write_landxml_file`] writes a whole
//! [`LandXmlFile`] as `afrobim_alignment::parse_landxml_file` reads it, and adds:
//!
//! - `CoordinateSystem` with every attribute the schema defines;
//! - `CgPoints/CgPoint` with nested `CgPoints` groups (`name`, `code`, `desc`, `state`, `oID`,
//!   `pointGeometry`, `DTMAttribute`, `latitude`, `longitude`);
//! - `Parcels/Parcel` (`area` in square metres, `Center`, `CoordGeom` of lines, curves, spirals,
//!   irregular lines and chains, nested `Parcels`) and `PlanFeatures/PlanFeature`;
//! - `Surfaces/Surface` with `Definition` (`Pnts/P`, `Faces/F`, `i="1"` for invisible faces; a
//!   TIN or a grid as it was read) and `SourceData` (`DataPoints`, `Breaklines`, `Contours`,
//!   `Boundaries`); [`surface_from_tin`] makes a surface from a corridor `Tin`;
//! - `Profile/ProfSurf` (existing ground, `PntList2D` of station and elevation) next to `ProfAlign`;
//! - `CrossSects/CrossSect` with `CrossSectSurf` and `DesignCrossSectSurf/CrossSectPnt`, with the
//!   attributes of `CrossSects` (`calcMethod`, `swellFactor`, ...) and of `CrossSect` (`angleSkew`,
//!   `areaCut`, `volumeFill`, `sectType`, ...);
//! - `Superelevation`: the records of the file as they were read (`staStart`, `staEnd`, every
//!   critical station, every `FullSuperelev` with its sign, every `AdverseSE`), while the engine
//!   superelevation of the alignment is still the one derived from them. When it was changed since,
//!   one element per engine section that states a rate: LandXML gives one rate, so such a section is
//!   written only when its full-super stations hold one rate with opposite signs on the two sides
//!   and the horizontal curve tells the inside; anything else is left out with a warning;
//! - `Feature` elements (with `Property`, `DocFileRef`, nested `Feature`) on the alignment, its
//!   profiles and cross-sections, surfaces, plan features and the coordinate system, `FieldNote` of
//!   the coordinate system, `SourceData/PointFiles`, and the statistics of `Surface/Definition`.
//!
//! **The geometry as the file stated it.** An alignment read by `parse_landxml_file` is written from
//! the points the file gave (`LandXmlAlignmentInfo::coord_geoms`: every `Start`, `Center`, `PI`,
//! `End`), not recomputed from the engine elements, so a file that is read and written comes back
//! with the same coordinates bit for bit. That is done only while the engine alignment is the one
//! built from them (compared by element count, length, start and end); an alignment that was
//! edited is written from its engine elements, with a warning. The same goes for the design
//! profile (written from the entries of the file, including a vertical step that the engine
//! profile cannot hold, while the engine profile is still the one derived from them).
//!
//! Not written: the `Application`s of a source file (the output was not made by them),
//! `LandXML@readOnly`, `Alignments@name`, `Surface/Watersheds`, `SurfVolumes`, and a unit other than
//! metres. What the reader does not read does not come back: the redundant attributes of `Line`,
//! `Curve` and `Spiral` (`dir`, `length` of a line, `chord`, `delta`, `dirStart`, `theta`, `totalX`,
//! ...) and the neighbour lists `F@n`; consumers recompute them from the points. A `Feature` that
//! hangs on any object the writer does not write one for, a `FieldNote` anywhere but in the
//! coordinate system, is counted in a warning. A `Spiral` of a parcel or plan feature that was read
//! without a `PI` is written without one, which the schema does not allow (a warning says so).
//!
//! **Direction and `staStart` of each element** are left out; consumers recompute them from the
//! points, as the `afrobim-alignment` importer does.

use std::collections::HashSet;

use afrobim_alignment::{
    critical_station_element, Alignment, AlignmentElement, CriticalStation, ElementKind, LandXmlCgPointGroup, LandXmlCoordGeom, LandXmlCrossSectDetail, LandXmlCrossSection, LandXmlDesignProfile,
    LandXmlFeature, LandXmlFile, LandXmlGeomElement, LandXmlGeometryFingerprint, LandXmlParcel, LandXmlPlanFeature, LandXmlPoint, LandXmlProfEntryKind, LandXmlProfSurf, LandXmlSurfFace,
    LandXmlSurfPoint, LandXmlSurface, StationDirection, VerticalCurve,
};
use afrobim_corridor::Tin;

/// What goes in the file header.
#[derive(Debug, Clone, PartialEq)]
pub struct LandXmlOptions {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM:SS`.
    pub time: String,
    /// Name of the project for the `Project` element; not written when empty.
    pub project: String,
}

impl Default for LandXmlOptions {
    fn default() -> Self {
        let (date, time) = now_utc();
        LandXmlOptions { date, time, project: String::new() }
    }
}

fn now_utc() -> (String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
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
    (format!("{y:04}-{m:02}-{d:02}"), format!("{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn pt(p: (f64, f64)) -> String {
    format!("{} {}", p.1, p.0)
}

/// Intersection of the tangent at the start and at the end of an element (the LandXML `PI`).
fn tangent_intersection(e: &AlignmentElement) -> (f64, f64) {
    let (x1, y1, h1) = e.end();
    let (x0, y0, h0) = (e.x, e.y, e.heading_rad);
    let (d0, d1) = ((h0.cos(), h0.sin()), (h1.cos(), h1.sin()));
    let cross = d0.0 * d1.1 - d0.1 * d1.0;
    if cross.abs() < 1e-12 {
        return ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    }
    // x0 + t d0 = x1 + u d1
    let (dx, dy) = (x1 - x0, y1 - y0);
    let t = (dx * d1.1 - dy * d1.0) / cross;
    (x0 + t * d0.0, y0 + t * d0.1)
}

/// The alignment as a LandXML 1.2 document.
pub fn write_landxml(alignment: &Alignment, options: &LandXmlOptions) -> String {
    let file = LandXmlFile { alignments: vec![alignment.clone()], ..Default::default() };
    write_landxml_file(&file, options).text
}

/// A LandXML document and what the writer could not put in it.
#[derive(Debug, Clone, PartialEq)]
pub struct LandXmlOutput {
    pub text: String,
    /// Content that was left out or could not be written as the schema asks, each with its reason.
    pub warnings: Vec<String>,
}

/// A surface from a TIN: points numbered 1.. in order, faces by those numbers. Coordinates are
/// copied as they are (X = easting, Y = northing).
pub fn surface_from_tin(name: &str, tin: &Tin) -> LandXmlSurface {
    LandXmlSurface {
        name: name.to_string(),
        surf_type: Some("TIN".to_string()),
        points: tin.points().iter().enumerate().map(|(i, p)| LandXmlSurfPoint { id: i as u64 + 1, easting_m: p[0], northing_m: p[1], elevation_m: p[2] }).collect(),
        faces: tin.triangles().iter().map(|t| LandXmlSurfFace { ids: t.iter().map(|&i| u64::from(i) + 1).collect(), invisible: false }).collect(),
        ..Default::default()
    }
}

/// A number as text: Rust's shortest representation that reads back to the same `f64`, so nothing
/// is rounded (UTM-sized values keep every digit they have).
fn n(v: f64) -> String {
    format!("{v}")
}

/// ` name="value"` or nothing.
fn at(name: &str, v: Option<&str>) -> String {
    v.map_or(String::new(), |v| format!(" {name}=\"{}\"", esc(v)))
}

/// `northing easting [elevation]`.
fn lx_pt(x: f64, y: f64, z: Option<f64>) -> String {
    match z {
        Some(z) => format!("{} {} {}", n(y), n(x), n(z)),
        None => format!("{} {}", n(y), n(x)),
    }
}

fn lp(p: &LandXmlPoint) -> String {
    lx_pt(p.x, p.y, p.z)
}

fn pair_list(list: &[[f64; 2]]) -> String {
    list.iter().map(|p| format!("{} {}", n(p[0]), n(p[1]))).collect::<Vec<_>>().join(" ")
}

/// A point list: `PntList3D` when every point has an elevation, else `PntList2D` (northing easting).
fn plan_list(o: &mut String, ind: &str, points: &[LandXmlPoint], warnings: &mut Vec<String>, what: &str) {
    let all_z = points.iter().all(|p| p.z.is_some());
    if !all_z && points.iter().any(|p| p.z.is_some()) {
        warnings.push(format!("{what}: some points have an elevation and some do not; they were written without elevations."));
    }
    if all_z && !points.is_empty() {
        o.push_str(&format!("{ind}<PntList3D>{}</PntList3D>\n", points.iter().map(|p| lx_pt(p.x, p.y, p.z)).collect::<Vec<_>>().join(" ")));
    } else {
        o.push_str(&format!("{ind}<PntList2D>{}</PntList2D>\n", points.iter().map(|p| lx_pt(p.x, p.y, None)).collect::<Vec<_>>().join(" ")));
    }
}

fn write_coord_geom(o: &mut String, ind: &str, g: &LandXmlCoordGeom, warnings: &mut Vec<String>, what: &str) {
    o.push_str(&format!("{ind}<CoordGeom{}{}{}>\n", at("name", g.name.as_deref()), at("desc", g.desc.as_deref()), at("state", g.state.as_deref())));
    let i2 = format!("{ind}  ");
    let i3 = format!("{ind}    ");
    for e in &g.elements {
        match e {
            LandXmlGeomElement::Line { start, end } => {
                o.push_str(&format!("{i2}<Line>\n{i3}<Start>{}</Start>\n{i3}<End>{}</End>\n{i2}</Line>\n", lp(start), lp(end)));
            }
            LandXmlGeomElement::Curve { start, center, end, pi, clockwise, radius_m, length_m } => {
                let rot = if *clockwise { "cw" } else { "ccw" };
                let radius = radius_m.map_or(String::new(), |r| format!(" radius=\"{}\"", n(r)));
                let length = length_m.map_or(String::new(), |l| format!(" length=\"{}\"", n(l)));
                o.push_str(&format!("{i2}<Curve rot=\"{rot}\"{radius}{length}>\n{i3}<Start>{}</Start>\n{i3}<Center>{}</Center>\n{i3}<End>{}</End>\n", lp(start), lp(center), lp(end)));
                if let Some(pi) = pi {
                    o.push_str(&format!("{i3}<PI>{}</PI>\n", lp(pi)));
                }
                o.push_str(&format!("{i2}</Curve>\n"));
            }
            LandXmlGeomElement::Spiral { start, pi, end, spiral_type, clockwise, length_m, radius_start_m, radius_end_m } => {
                let rot = if *clockwise { "cw" } else { "ccw" };
                let rad = |r: &Option<f64>| r.map_or("INF".to_string(), |r| n(r));
                o.push_str(&format!(
                    "{i2}<Spiral length=\"{}\" radiusStart=\"{}\" radiusEnd=\"{}\" rot=\"{rot}\" spiType=\"{}\">\n{i3}<Start>{}</Start>\n",
                    n(*length_m),
                    rad(radius_start_m),
                    rad(radius_end_m),
                    esc(spiral_type),
                    lp(start)
                ));
                match pi {
                    Some(pi) => o.push_str(&format!("{i3}<PI>{}</PI>\n", lp(pi))),
                    None => warnings.push(format!("{what}: a Spiral has no PI, which the schema requires; it was written without one.")),
                }
                o.push_str(&format!("{i3}<End>{}</End>\n{i2}</Spiral>\n", lp(end)));
            }
            LandXmlGeomElement::IrregularLine { start, end, points } => {
                o.push_str(&format!("{i2}<IrregularLine>\n{i3}<Start>{}</Start>\n{i3}<End>{}</End>\n", lp(start), lp(end)));
                plan_list(o, &i3, points, warnings, what);
                o.push_str(&format!("{i2}</IrregularLine>\n"));
            }
            LandXmlGeomElement::Chain { names, .. } => {
                o.push_str(&format!("{i2}<Chain>{}</Chain>\n", esc(&names.join(" "))));
            }
        }
    }
    o.push_str(&format!("{ind}</CoordGeom>\n"));
}

fn write_cg_group(o: &mut String, ind: &str, g: &LandXmlCgPointGroup) {
    let zone = g.zone_number.map(|z| z.to_string());
    o.push_str(&format!(
        "{ind}<CgPoints{}{}{}{}{}{}>\n",
        at("name", g.name.as_deref()),
        at("desc", g.desc.as_deref()),
        at("state", g.state.as_deref()),
        at("code", g.code.as_deref()),
        at("zoneNumber", zone.as_deref()),
        at("DTMAttribute", g.dtm_attribute.as_deref())
    ));
    for p in &g.points {
        let (lat, lon) = (p.latitude.map(n), p.longitude.map(n));
        o.push_str(&format!(
            "{ind}  <CgPoint name=\"{}\"{}{}{}{}{}{}{}{}>{}</CgPoint>\n",
            esc(&p.name),
            at("code", p.code.as_deref()),
            at("desc", p.desc.as_deref()),
            at("state", p.state.as_deref()),
            at("oID", p.oid.as_deref()),
            at("pointGeometry", p.point_geometry.as_deref()),
            at("DTMAttribute", p.dtm_attribute.as_deref()),
            at("latitude", lat.as_deref()),
            at("longitude", lon.as_deref()),
            lx_pt(p.easting_m, p.northing_m, p.elevation_m)
        ));
    }
    for sub in &g.groups {
        write_cg_group(o, &format!("{ind}  "), sub);
    }
    o.push_str(&format!("{ind}</CgPoints>\n"));
}

fn write_parcel(o: &mut String, ind: &str, p: &LandXmlParcel, warnings: &mut Vec<String>) {
    let area = p.area_m2.map(n);
    o.push_str(&format!(
        "{ind}<Parcel name=\"{}\"{}{}{}{}{}{}>\n",
        esc(&p.name),
        at("area", area.as_deref()),
        at("desc", p.desc.as_deref()),
        at("state", p.state.as_deref()),
        at("owner", p.owner.as_deref()),
        at("parcelType", p.parcel_type.as_deref()),
        at("class", p.class.as_deref())
    ));
    if let Some(c) = &p.center {
        o.push_str(&format!("{ind}  <Center>{}</Center>\n", lp(c)));
    }
    for g in &p.coord_geoms {
        write_coord_geom(o, &format!("{ind}  "), g, warnings, &format!("Parcel '{}'", p.name));
    }
    if !p.parcels.is_empty() {
        o.push_str(&format!("{ind}  <Parcels>\n"));
        for sub in &p.parcels {
            write_parcel(o, &format!("{ind}    "), sub, warnings);
        }
        o.push_str(&format!("{ind}  </Parcels>\n"));
    }
    o.push_str(&format!("{ind}</Parcel>\n"));
}

fn write_surface(o: &mut String, s: &LandXmlSurface, ctx: &mut Ctx, warnings: &mut Vec<String>) {
    o.push_str(&format!("    <Surface name=\"{}\"{}{}{}>\n", esc(&s.name), at("desc", s.desc.as_deref()), at("OID", s.oid.as_deref()), at("state", s.state.as_deref())));
    let what = format!("Surface '{}'", s.name);
    let src = &s.source_data;
    if !src.is_empty() {
        o.push_str("      <SourceData>\n");
        if !src.point_files.is_empty() {
            o.push_str("        <PointFiles>\n");
            for f in &src.point_files {
                o.push_str(&format!("          <PointFile fileName=\"{}\" fileType=\"{}\" fileFormat=\"{}\"/>\n", esc(&f.file_name), esc(&f.file_type), esc(&f.file_format)));
            }
            o.push_str("        </PointFiles>\n");
        }
        if !src.data_points.is_empty() {
            for d in &src.data_points {
                let list: Vec<String> = d.points.iter().filter_map(|p| p.z.map(|z| lx_pt(p.x, p.y, Some(z)))).collect();
                if list.len() != d.points.len() {
                    warnings.push(format!("{what}: DataPoints points without an elevation were left out."));
                }
                if list.is_empty() {
                    continue;
                }
                o.push_str(&format!(
                    "        <DataPoints{}{}{}{}>\n          <PntList3D>{}</PntList3D>\n        </DataPoints>\n",
                    at("name", d.name.as_deref()),
                    at("desc", d.desc.as_deref()),
                    at("code", d.code.as_deref()),
                    at("state", d.state.as_deref()),
                    list.join(" ")
                ));
            }
        }
        if !src.breaklines.is_empty() {
            o.push_str("        <Breaklines>\n");
            for b in &src.breaklines {
                o.push_str(&format!(
                    "          <Breakline{}{}{}{}>\n",
                    at("brkType", b.brk_type.as_deref()),
                    at("desc", b.desc.as_deref()),
                    at("name", b.name.as_deref()),
                    at("state", b.state.as_deref())
                ));
                plan_list(o, "            ", &b.points, warnings, &what);
                o.push_str("          </Breakline>\n");
            }
            o.push_str("        </Breaklines>\n");
        }
        if !src.contours.is_empty() {
            o.push_str("        <Contours>\n");
            for c in &src.contours {
                o.push_str(&format!("          <Contour elev=\"{}\">\n", n(c.elevation_m)));
                let flat: Vec<LandXmlPoint> = c.points.iter().map(|p| LandXmlPoint { z: None, ..*p }).collect();
                plan_list(o, "            ", &flat, warnings, &what);
                o.push_str("          </Contour>\n");
            }
            o.push_str("        </Contours>\n");
        }
        if !src.boundaries.is_empty() {
            o.push_str("        <Boundaries>\n");
            for b in &src.boundaries {
                let area = b.area.map(n);
                let bnd = if b.bnd_type.is_empty() { "outer" } else { b.bnd_type.as_str() };
                o.push_str(&format!(
                    "          <Boundary bndType=\"{}\" edgeTrim=\"{}\"{}{}{}{}>\n",
                    esc(bnd),
                    b.edge_trim,
                    at("area", area.as_deref()),
                    at("desc", b.desc.as_deref()),
                    at("name", b.name.as_deref()),
                    at("state", b.state.as_deref())
                ));
                plan_list(o, "            ", &b.points, warnings, &what);
                o.push_str("          </Boundary>\n");
            }
            o.push_str("        </Boundaries>\n");
        }
        o.push_str("      </SourceData>\n");
    }
    if !s.points.is_empty() || !s.faces.is_empty() {
        if s.points.len() < 3 || s.faces.is_empty() {
            warnings.push(format!("{what}: its Definition needs at least three points and one face; it was left out."));
        } else {
            let kind = s.surf_type.as_deref().unwrap_or("TIN");
            let stat = |name: &str, v: Option<f64>| v.map_or(String::new(), |v| format!(" {name}=\"{}\"", n(v)));
            let st = s.stated.unwrap_or_default();
            o.push_str(&format!(
                "      <Definition surfType=\"{}\"{}{}{}{}>\n        <Pnts>\n",
                esc(kind),
                stat("area2DSurf", st.area_2d_m2),
                stat("area3DSurf", st.area_3d_m2),
                stat("elevMax", st.elev_max_m),
                stat("elevMin", st.elev_min_m)
            ));
            for p in &s.points {
                o.push_str(&format!("          <P id=\"{}\">{}</P>\n", p.id, lx_pt(p.easting_m, p.northing_m, Some(p.elevation_m))));
            }
            o.push_str("        </Pnts>\n        <Faces>\n");
            for f in &s.faces {
                let ids = f.ids.iter().map(u64::to_string).collect::<Vec<_>>().join(" ");
                o.push_str(&format!("          <F{}>{ids}</F>\n", if f.invisible { " i=\"1\"" } else { "" }));
            }
            o.push_str("        </Faces>\n      </Definition>\n");
        }
    }
    ctx.features(o, "      ", "Surface", None, Some(&s.name), None);
    o.push_str("    </Surface>\n");
}

/// Writing state: the file being written and which `Feature` owners were written.
struct Ctx<'a> {
    file: &'a LandXmlFile,
    written_features: HashSet<usize>,
}

fn write_feature(o: &mut String, ind: &str, f: &LandXmlFeature) {
    o.push_str(&format!("{ind}<Feature{}{}{}>\n", at("name", f.name.as_deref()), at("code", f.code.as_deref()), at("source", f.source.as_deref())));
    for p in &f.properties {
        o.push_str(&format!("{ind}  <Property label=\"{}\" value=\"{}\"/>\n", esc(&p.label), esc(&p.value)));
    }
    for d in &f.doc_file_refs {
        let attrs: String = d.iter().map(|(k, v)| format!(" {k}=\"{}\"", esc(v))).collect();
        o.push_str(&format!("{ind}  <DocFileRef{attrs}/>\n"));
    }
    for sub in &f.features {
        write_feature(o, &format!("{ind}  "), sub);
    }
    o.push_str(&format!("{ind}</Feature>\n"));
}

impl Ctx<'_> {
    /// Writes the `Feature` elements that hang on the object of that kind. `alignment` is the name
    /// of the enclosing alignment, `name` the object's own name, `station` a `CrossSect`'s station.
    fn features(&mut self, o: &mut String, ind: &str, kind: &str, alignment: Option<&str>, name: Option<&str>, station: Option<f64>) {
        self.features_at(o, ind, kind, alignment, name, station, None);
    }

    /// As [`Ctx::features`], and `ordinal` (the object's place among the elements of its kind) must match when given.
    #[allow(clippy::too_many_arguments)]
    fn features_at(&mut self, o: &mut String, ind: &str, kind: &str, alignment: Option<&str>, name: Option<&str>, station: Option<f64>, ordinal: Option<usize>) {
        for (i, group) in self.file.object_features.iter().enumerate() {
            let w = &group.owner;
            let same_station = match (w.station_m, station) {
                (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
                (None, None) => true,
                _ => false,
            };
            if w.kind == kind && ordinal.map_or(true, |k| k == w.ordinal) && (alignment.is_none() || w.alignment.as_deref() == alignment) && same_station && (name.is_none() && w.name.is_none() || w.name.as_deref() == name) {
                if self.written_features.insert(i) {
                    for f in &group.features {
                        write_feature(o, ind, f);
                    }
                }
            }
        }
    }
}

fn write_alignment_geometry_from_engine(o: &mut String, alignment: &Alignment) {
    o.push_str("      <CoordGeom>\n");
    for e in &alignment.elements {
        let (x1, y1, h1) = e.end();
        let start = (e.x, e.y);
        let end = (x1, y1);
        match e.kind() {
            ElementKind::Line => {
                o.push_str(&format!("        <Line length=\"{}\">\n          <Start>{}</Start>\n          <End>{}</End>\n        </Line>\n", e.length, pt(start), pt(end)));
            }
            ElementKind::Arc => {
                let k = e.curvature_start;
                let r = 1.0 / k.abs();
                let centre = (e.x - e.heading_rad.sin() / k, e.y + e.heading_rad.cos() / k);
                let rot = if k > 0.0 { "ccw" } else { "cw" };
                let delta = (h1 - e.heading_rad).abs();
                let pi = tangent_intersection(e);
                o.push_str(&format!(
                    "        <Curve rot=\"{rot}\" radius=\"{r}\" length=\"{}\" delta=\"{delta}\">\n          <Start>{}</Start>\n          <Center>{}</Center>\n          <End>{}</End>\n          <PI>{}</PI>\n        </Curve>\n",
                    e.length,
                    pt(start),
                    pt(centre),
                    pt(end),
                    pt(pi)
                ));
            }
            ElementKind::Spiral => {
                let rad = |k: f64| if k == 0.0 { "INF".to_string() } else { format!("{}", 1.0 / k.abs()) };
                let sum = e.curvature_start + e.curvature_end;
                let rot = if sum > 0.0 { "ccw" } else { "cw" };
                o.push_str(&format!(
                    "        <Spiral length=\"{}\" radiusStart=\"{}\" radiusEnd=\"{}\" rot=\"{rot}\" spiType=\"clothoid\">\n          <Start>{}</Start>\n          <PI>{}</PI>\n          <End>{}</End>\n        </Spiral>\n",
                    e.length,
                    rad(e.curvature_start),
                    rad(e.curvature_end),
                    pt(start),
                    pt(tangent_intersection(e)),
                    pt(end)
                ));
            }
        }
    }
    o.push_str("      </CoordGeom>\n");
}

/// `Superelevation` elements. When the file was read, the records are written as they were read
/// (every station, every rate, with the sign it had), provided the alignment's engine superelevation
/// is still the one derived from them. Otherwise one element per engine section that states a rate:
/// LandXML gives one rate, so a section is written only when its full-super stations hold one rate
/// with opposite signs on the two sides and the horizontal curve tells the inside; anything else is
/// left out with a warning.
fn write_superelevation(o: &mut String, file: &LandXmlFile, alignment: &Alignment, warnings: &mut Vec<String>) {
    if let Some(table) = file.superelevation_table(&alignment.name) {
        if table.derived == alignment.superelevation {
            for r in &table.records {
                o.push_str(&format!("      <Superelevation{}{}>\n", at("staStart", r.sta_start_m.map(n).as_deref()), at("staEnd", r.sta_end_m.map(n).as_deref())));
                for s in &r.stations {
                    let name = critical_station_element(s.kind);
                    o.push_str(&format!("        <{name}>{}</{name}>\n", n(s.station_m)));
                }
                for rate in &r.full_super_rates_pct {
                    o.push_str(&format!("        <FullSuperelev>{}</FullSuperelev>\n", n(*rate)));
                }
                for a in &r.adverse {
                    o.push_str(&format!("        <AdverseSE>{}</AdverseSE>\n", esc(a)));
                }
                o.push_str("      </Superelevation>\n");
            }
            return;
        }
        warnings.push(format!("Alignment '{}': its superelevation was changed since the file was read; the {} records of the file were replaced by what the engine superelevation can say.", alignment.name, table.records.len()));
    }
    let Some(se) = &alignment.superelevation else { return };
    const NAMES: [(CriticalStation, &str); 6] = [
        (CriticalStation::BeginRunout, "BeginRunoutSta"),
        (CriticalStation::BeginRunoff, "BeginRunoffSta"),
        (CriticalStation::FullSuper, "FullSuperSta"),
        (CriticalStation::Runoff, "RunoffSta"),
        (CriticalStation::StartOfRunout, "StartofRunoutSta"),
        (CriticalStation::EndOfRunout, "EndofRunoutSta"),
    ];
    for (i, section) in se.sections.iter().enumerate() {
        let what = format!("Alignment '{}', superelevation section {}", alignment.name, i + 1);
        let named: Vec<_> = NAMES.iter().filter_map(|(k, name)| section.points.iter().find(|p| p.kind == Some(*k)).map(|p| (*name, *k, p))).collect();
        if section.points.iter().any(|p| p.kind.is_none()) {
            warnings.push(format!("{what}: stations without a critical-station kind cannot be written (the schema names its stations)."));
        }
        let full: Vec<_> = named.iter().filter(|(_, k, _)| matches!(k, CriticalStation::FullSuper | CriticalStation::Runoff)).collect();
        let Some((_, _, first)) = full.first() else {
            warnings.push(format!("{what}: it has neither FullSuperSta nor RunoffSta, so there is no rate to write; it was left out."));
            continue;
        };
        let (Some(left), Some(right)) = (first.left_pct, first.right_pct) else {
            warnings.push(format!("{what}: it states no cross-fall at its full-superelevation station; it was left out."));
            continue;
        };
        let consistent = full.iter().all(|(_, _, p)| p.left_pct == Some(left) && p.right_pct == Some(right));
        if left != -right || left == 0.0 || !consistent {
            warnings.push(format!(
                "{what}: LandXML holds one rate (FullSuperelev); left {left:+} % and right {right:+} % at its full-superelevation stations are not one rate on both sides with opposite signs; it was left out."
            ));
            continue;
        }
        let middle = (full[0].2.station_m + full[full.len() - 1].2.station_m) / 2.0;
        let Some(k) = alignment.curvature_at(middle).filter(|k| *k != 0.0) else {
            warnings.push(format!("{what}: the horizontal alignment is straight (or absent) at its middle, so the inside of the curve cannot be told and AdverseSE cannot be set; it was left out."));
            continue;
        };
        // The reader puts the falling side on the inside of the curve unless the section is adverse.
        let adverse = (k > 0.0) != (left < 0.0);
        let (lo, hi) = (named.first().map_or(0.0, |x| x.2.station_m), named.last().map_or(0.0, |x| x.2.station_m));
        o.push_str(&format!("      <Superelevation staStart=\"{}\" staEnd=\"{}\">\n", n(lo), n(hi)));
        for (name, _, p) in &named {
            o.push_str(&format!("        <{name}>{}</{name}>\n", n(p.station_m)));
        }
        o.push_str(&format!("        <FullSuperelev>{}</FullSuperelev>\n", n(left.abs())));
        o.push_str(&format!("        <AdverseSE>{}</AdverseSE>\n      </Superelevation>\n", if adverse { "adverse" } else { "non-adverse" }));
    }
}

fn prof_entry_text(e: &afrobim_alignment::LandXmlProfEntry) -> String {
    let text = format!("{} {}", n(e.station_m), n(e.elevation_m));
    let desc = at("desc", e.desc.as_deref());
    let num = |name: &str, v: Option<f64>| v.map_or(String::new(), |v| format!(" {name}=\"{}\"", n(v)));
    match e.kind {
        LandXmlProfEntryKind::Pvi => format!("<PVI{desc}>{text}</PVI>"),
        LandXmlProfEntryKind::ParaCurve => format!("<ParaCurve{}{desc}>{text}</ParaCurve>", num("length", e.length_m)),
        LandXmlProfEntryKind::UnsymParaCurve => format!("<UnsymParaCurve{}{}{desc}>{text}</UnsymParaCurve>", num("lengthIn", e.length_in_m), num("lengthOut", e.length_out_m)),
        LandXmlProfEntryKind::CircCurve => format!("<CircCurve{}{}{desc}>{text}</CircCurve>", num("length", e.length_m), num("radius", e.radius_m)),
    }
}

fn write_alignment(o: &mut String, ctx: &mut Ctx, alignment: &Alignment, warnings: &mut Vec<String>) {
    let file = ctx.file;
    let ground: Vec<&LandXmlProfSurf> = file.ground_profiles.iter().filter(|g| g.alignment == alignment.name).collect();
    let sections: Vec<&LandXmlCrossSection> = file.cross_sections.iter().filter(|c| c.alignment == alignment.name).collect();
    let info = file.alignment_info(&alignment.name);
    // The stated geometry is written when the engine alignment is still the one built from it.
    let fresh = info.filter(|i| i.imported && !i.coord_geoms.is_empty() && i.fingerprint.is_some() && i.fingerprint == LandXmlGeometryFingerprint::of(alignment));
    if info.is_some() && fresh.is_none() && info.is_some_and(|i| i.imported) {
        warnings.push(format!("Alignment '{}': its geometry was changed since the file was read; it is written from the engine alignment, not from the points the file stated.", alignment.name));
    }
    let length = fresh.and_then(|i| i.declared_length_m).unwrap_or_else(|| alignment.length());
    let sta_start = fresh.and_then(|i| i.sta_start_m).unwrap_or(alignment.start_station_m);
    o.push_str(&format!(
        "    <Alignment name=\"{}\" length=\"{}\" staStart=\"{}\"{}{}{}>\n",
        esc(&alignment.name),
        length,
        sta_start,
        at("desc", info.and_then(|i| i.desc.as_deref())),
        at("state", info.and_then(|i| i.state.as_deref())),
        at("oID", info.and_then(|i| i.oid.as_deref()))
    ));
    match fresh {
        Some(i) => {
            for g in &i.coord_geoms {
                write_coord_geom(o, "      ", g, warnings, &format!("Alignment '{}'", alignment.name));
            }
        }
        None => write_alignment_geometry_from_engine(o, alignment),
    }
    for q in &alignment.station_equations.equations {
        let dir = match q.direction {
            StationDirection::Increasing => "increasing",
            StationDirection::Decreasing => "decreasing",
        };
        let back = q.back_m.map_or(String::new(), |b| format!(" staBack=\"{b}\""));
        o.push_str(&format!("      <StaEquation staAhead=\"{}\"{back} staInternal=\"{}\" staIncrement=\"{dir}\"/>\n", q.ahead_m, q.internal_m));
    }

    // Profiles. The design profiles as the file wrote them go back as they were while the engine
    // profile is still the one derived from the first of them; otherwise the engine profile is
    // written, with the first group of existing-ground profiles or alone in a Profile of its own name.
    let raws: Vec<&LandXmlDesignProfile> = file.design_profiles.iter().filter(|d| d.alignment == alignment.name).collect();
    let raws_current = !raws.is_empty() && raws[0].to_profile().ok() == alignment.profile;
    if !raws.is_empty() && !raws_current {
        warnings.push(format!("Alignment '{}': its profile was changed since the file was read; the engine profile is written and the design profiles of the file are not.", alignment.name));
    }
    let mut groups: Vec<Option<&str>> = Vec::new();
    for g in &ground {
        if !groups.contains(&g.profile.as_deref()) {
            groups.push(g.profile.as_deref());
        }
    }
    if raws_current {
        for r in &raws {
            if !groups.contains(&r.profile.as_deref()) {
                groups.push(r.profile.as_deref());
            }
        }
    } else if groups.is_empty() && alignment.profile.is_some() {
        groups.push(None);
    }
    for (gi, group) in groups.iter().enumerate() {
        let designs: Vec<&LandXmlDesignProfile> = if raws_current { raws.iter().copied().filter(|r| r.profile.as_deref() == *group).collect() } else { Vec::new() };
        let engine = if !raws_current && gi == 0 { alignment.profile.as_ref() } else { None };
        let name = group.map(str::to_string).or_else(|| engine.map(|p| p.name.clone()));
        let first = designs.first();
        o.push_str(&format!(
            "      <Profile{}{}{}{}>\n",
            at("name", name.as_deref()),
            at("desc", first.and_then(|d| d.profile_desc.as_deref())),
            at("state", first.and_then(|d| d.profile_state.as_deref())),
            at("staStart", first.and_then(|d| d.profile_sta_start_m).map(n).as_deref())
        ));
        for s in ground.iter().filter(|g| g.profile.as_deref() == *group) {
            if s.point_lists.is_empty() {
                warnings.push(format!("Alignment '{}': ground profile '{}' has no points; it was left out.", alignment.name, s.name));
                continue;
            }
            o.push_str(&format!("        <ProfSurf name=\"{}\"{}{}>\n", esc(&s.name), at("desc", s.desc.as_deref()), at("state", s.state.as_deref())));
            for l in &s.point_lists {
                o.push_str(&format!("          <PntList2D>{}</PntList2D>\n", pair_list(l)));
            }
            ctx.features(o, "          ", "ProfSurf", Some(&alignment.name), Some(&s.name), None);
            o.push_str("        </ProfSurf>\n");
        }
        for d in &designs {
            o.push_str(&format!("        <ProfAlign name=\"{}\"{}{}>\n", esc(&d.name), at("desc", d.desc.as_deref()), at("state", d.state.as_deref())));
            for e in &d.entries {
                o.push_str(&format!("          {}\n", prof_entry_text(e)));
            }
            ctx.features(o, "          ", "ProfAlign", Some(&alignment.name), Some(&d.name), None);
            o.push_str("        </ProfAlign>\n");
        }
        if let Some(p) = engine {
            o.push_str(&format!("        <ProfAlign name=\"{}\">\n", esc(&p.name)));
            for (i, v) in p.pvis.iter().enumerate() {
                let text = format!("{} {}", v.station_m, v.elevation_m);
                match v.curve {
                    VerticalCurve::None => o.push_str(&format!("          <PVI>{text}</PVI>\n")),
                    VerticalCurve::Parabola { length_m } => o.push_str(&format!("          <ParaCurve length=\"{length_m}\">{text}</ParaCurve>\n")),
                    VerticalCurve::AsymmetricParabola { length_in_m, length_out_m } => {
                        o.push_str(&format!("          <UnsymParaCurve lengthIn=\"{length_in_m}\" lengthOut=\"{length_out_m}\">{text}</UnsymParaCurve>\n"))
                    }
                    VerticalCurve::Circular { radius_m } => {
                        // The arc length of the circle between the tangent points.
                        let g1 = i.checked_sub(1).and_then(|k| p.grade_between(k)).unwrap_or(0.0);
                        let g2 = p.grade_between(i).unwrap_or(0.0);
                        let arc = radius_m * ((g2.atan() - g1.atan()).abs());
                        o.push_str(&format!("          <CircCurve length=\"{arc}\" radius=\"{radius_m}\">{text}</CircCurve>\n"));
                    }
                }
            }
            o.push_str("        </ProfAlign>\n");
        }
        ctx.features(o, "        ", "Profile", Some(&alignment.name), group.as_deref().or(name.as_deref()), None);
        o.push_str("      </Profile>\n");
    }
    write_superelevation(o, file, alignment, warnings);

    if !sections.is_empty() {
        // The CrossSects elements as they were read (their attributes), the sections in the one
        // each came from; a file built in memory has one group.
        let group_attrs: Vec<&afrobim_alignment::LandXmlCrossSectGroup> = file.cross_sect_groups.iter().filter(|g| g.alignment == alignment.name).collect();
        let details: Vec<&LandXmlCrossSectDetail> = file.cross_sect_details.iter().filter(|d| d.alignment == alignment.name).collect();
        let detail_of = |c: &LandXmlCrossSection| details.iter().copied().find(|d| d.station_m.to_bits() == c.station_m.to_bits() && d.name == c.name);
        let group_count = group_attrs.len().max(1);
        for gi in 0..group_count {
            let members: Vec<&LandXmlCrossSection> = sections.iter().copied().filter(|c| detail_of(c).map_or(0, |d| d.group).min(group_count - 1) == gi).collect();
            if members.is_empty() {
                continue;
            }
            let g = group_attrs.get(gi);
            let num = |name: &str, v: Option<f64>| v.map_or(String::new(), |v| format!(" {name}=\"{}\"", n(v)));
            o.push_str(&format!(
                "      <CrossSects{}{}{}{}{}{}{}>\n",
                at("desc", g.and_then(|g| g.desc.as_deref())),
                at("name", g.and_then(|g| g.name.as_deref())),
                at("state", g.and_then(|g| g.state.as_deref())),
                at("calcMethod", g.and_then(|g| g.calc_method.as_deref())),
                at("curveCorrection", g.and_then(|g| g.curve_correction).map(|b| b.to_string()).as_deref()),
                num("swellFactor", g.and_then(|g| g.swell_factor)),
                num("shrinkFactor", g.and_then(|g| g.shrink_factor))
            ));
            for cs in members {
                let d = detail_of(cs);
                o.push_str(&format!(
                    "        <CrossSect sta=\"{}\"{}{}{}{}{}{}{}{}{}{}>\n",
                    n(cs.station_m),
                    at("name", cs.name.as_deref()),
                    at("desc", cs.desc.as_deref()),
                    num("angleSkew", d.and_then(|d| d.angle_skew)),
                    num("areaCut", d.and_then(|d| d.area_cut)),
                    num("areaFill", d.and_then(|d| d.area_fill)),
                    num("centroidCut", d.and_then(|d| d.centroid_cut)),
                    num("centroidFill", d.and_then(|d| d.centroid_fill)),
                    at("sectType", d.and_then(|d| d.sect_type.as_deref())),
                    num("volumeCut", d.and_then(|d| d.volume_cut)),
                    num("volumeFill", d.and_then(|d| d.volume_fill)),
                ));
                for s in &cs.surfaces {
                    if s.point_lists.is_empty() {
                        warnings.push(format!("Alignment '{}', station {}: cross-section surface '{}' has no points; it was left out.", alignment.name, n(cs.station_m), s.name));
                        continue;
                    }
                    o.push_str(&format!("          <CrossSectSurf name=\"{}\"{}{}>\n", esc(&s.name), at("desc", s.desc.as_deref()), at("state", s.state.as_deref())));
                    for l in &s.point_lists {
                        o.push_str(&format!("            <PntList2D>{}</PntList2D>\n", pair_list(l)));
                    }
                    o.push_str("          </CrossSectSurf>\n");
                }
                for d in &cs.design_surfaces {
                    let closed = d.closed_area.map(|b| b.to_string());
                    o.push_str(&format!(
                        "          <DesignCrossSectSurf{}{}{}{}{}{}>\n",
                        at("name", d.name.as_deref()),
                        at("desc", d.desc.as_deref()),
                        at("state", d.state.as_deref()),
                        at("side", d.side.as_deref()),
                        at("material", d.material.as_deref()),
                        at("closedArea", closed.as_deref())
                    ));
                    for p in &d.points {
                        o.push_str(&format!("            <CrossSectPnt>{} {}</CrossSectPnt>\n", n(p.offset_m), n(p.elevation_m)));
                    }
                    o.push_str("          </DesignCrossSectSurf>\n");
                }
                ctx.features(o, "          ", "CrossSect", Some(&alignment.name), cs.name.as_deref(), Some(cs.station_m));
                o.push_str("        </CrossSect>\n");
            }
            ctx.features(o, "        ", "CrossSects", Some(&alignment.name), g.and_then(|g| g.name.as_deref()), None);
            o.push_str("      </CrossSects>\n");
        }
    }
    ctx.features(o, "      ", "Alignment", Some(&alignment.name), Some(&alignment.name), None);
    o.push_str("    </Alignment>\n");
}

/// A whole [`LandXmlFile`] as a LandXML 1.2 document, in metres: coordinate system, project,
/// alignments with their profile, existing-ground profiles, cross-sections, station equations and
/// superelevation, control points, parcels, plan features and surfaces with their source data.
///
/// Every number is written with Rust's shortest representation that reads back to the same `f64`,
/// so **coordinates are written exactly as they are held** (no rounding; a UTM easting of
/// 450123.4567 is written as `450123.4567`). The units written are metric, whatever the source's:
/// the values in a [`LandXmlFile`] are already metres. `angularUnit`, `directionUnit` and
/// `latLongAngularUnit` are those of the file read, because `CoordinateSystem@rotationAngle` is kept
/// in that unit. The `Application` is this crate; the applications of the source file are not
/// carried over (the output was not made by them). `Project` is the file's, or `options.project`.
///
/// Left out, with a warning: a ground profile or cross-section whose alignment is not in the file
/// (the schema places them inside an alignment), a surface definition with fewer than three points,
/// superelevation that LandXML's single rate cannot hold, a `Spiral` that has no `PI` is written
/// without one (so the file does not validate). Unit conversion is not offered: write what was
/// read, or build a [`LandXmlFile`] in metres.
pub fn write_landxml_file(file: &LandXmlFile, options: &LandXmlOptions) -> LandXmlOutput {
    let mut warnings: Vec<String> = Vec::new();
    let mut ctx = Ctx { file, written_features: HashSet::new() };
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    o.push_str(&format!(
        "<LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://www.landxml.org/schema/LandXML-1.2 http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd\" date=\"{}\" time=\"{}\" version=\"1.2\" language=\"English\">\n",
        esc(&options.date),
        esc(&options.time)
    ));
    let u = &file.units;
    o.push_str(&format!(
        "  <Units>\n    <Metric areaUnit=\"squareMeter\" linearUnit=\"meter\" volumeUnit=\"cubicMeter\" temperatureUnit=\"celsius\" pressureUnit=\"milliBars\" angularUnit=\"{}\" directionUnit=\"{}\" latLongAngularUnit=\"{}\" elevationUnit=\"meter\"/>\n  </Units>\n",
        esc(&u.angular_unit),
        esc(&u.direction_unit),
        esc(&u.lat_long_angular_unit)
    ));
    if let Some(c) = &file.coordinate_system {
        let rot = c.rotation_angle.map(n);
        o.push_str(&format!(
            "  <CoordinateSystem{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}>\n",
            at("desc", c.desc.as_deref()),
            at("name", c.name.as_deref()),
            at("epsgCode", c.epsg_code.as_deref()),
            at("ogcWktCode", c.ogc_wkt_code.as_deref()),
            at("horizontalDatum", c.horizontal_datum.as_deref()),
            at("verticalDatum", c.vertical_datum.as_deref()),
            at("ellipsoidName", c.ellipsoid_name.as_deref()),
            at("horizontalCoordinateSystemName", c.horizontal_coordinate_system_name.as_deref()),
            at("geocentricCoordinateSystemName", c.geocentric_coordinate_system_name.as_deref()),
            at("fileLocation", c.file_location.as_deref()),
            at("rotationAngle", rot.as_deref()),
            at("datum", c.datum.as_deref()),
            at("fittedCoordinateSystemName", c.fitted_coordinate_system_name.as_deref()),
            at("compoundCoordinateSystemName", c.compound_coordinate_system_name.as_deref()),
            at("localCoordinateSystemName", c.local_coordinate_system_name.as_deref()),
            at("geographicCoordinateSystemName", c.geographic_coordinate_system_name.as_deref()),
            at("projectedCoordinateSystemName", c.projected_coordinate_system_name.as_deref()),
            at("verticalCoordinateSystemName", c.vertical_coordinate_system_name.as_deref())
        ));
        for note in file.field_notes.iter().filter(|f| f.owner.kind == "CoordinateSystem") {
            o.push_str(&format!("    <FieldNote>{}</FieldNote>\n", esc(&note.text)));
        }
        ctx.features(&mut o, "    ", "CoordinateSystem", None, c.name.as_deref(), None);
        o.push_str("  </CoordinateSystem>\n");
    }
    match (&file.project, options.project.is_empty()) {
        (Some(p), _) => o.push_str(&format!("  <Project name=\"{}\"{}{}/>\n", esc(&p.name), at("desc", p.desc.as_deref()), at("state", p.state.as_deref()))),
        (None, false) => o.push_str(&format!("  <Project name=\"{}\"/>\n", esc(&options.project))),
        (None, true) => {}
    }
    o.push_str("  <Application name=\"AfroBIM afrobim-design\" desc=\"Concept alignment design\"/>\n");

    for g in &file.cg_points {
        write_cg_group(&mut o, "  ", g);
    }

    if !file.alignments.is_empty() {
        o.push_str("  <Alignments>\n");
        for a in &file.alignments {
            write_alignment(&mut o, &mut ctx, a, &mut warnings);
        }
        o.push_str("  </Alignments>\n");
    }
    for g in &file.ground_profiles {
        if !file.alignments.iter().any(|a| a.name == g.alignment) {
            warnings.push(format!("Ground profile '{}' belongs to alignment '{}', which is not in the file; it was left out.", g.name, g.alignment));
        }
    }
    let mut orphan_sections = 0;
    for c in &file.cross_sections {
        if !file.alignments.iter().any(|a| a.name == c.alignment) {
            orphan_sections += 1;
        }
    }
    if orphan_sections > 0 {
        warnings.push(format!("{orphan_sections} cross-section(s) belong to alignments that are not in the file; they were left out."));
    }

    if !file.parcels.is_empty() {
        o.push_str("  <Parcels>\n");
        for p in &file.parcels {
            write_parcel(&mut o, "    ", p, &mut warnings);
        }
        o.push_str("  </Parcels>\n");
    }

    // Plan features, grouped by the PlanFeatures they came from, in order of first appearance.
    let mut groups: Vec<(Option<&str>, Vec<&LandXmlPlanFeature>)> = Vec::new();
    for f in &file.plan_features {
        match groups.iter_mut().find(|(g, _)| *g == f.group.as_deref()) {
            Some((_, v)) => v.push(f),
            None => groups.push((f.group.as_deref(), vec![f])),
        }
    }
    for (gi, (group, features)) in groups.into_iter().enumerate() {
        o.push_str(&format!("  <PlanFeatures{}>\n", at("name", group)));
        for f in features {
            o.push_str(&format!("    <PlanFeature{}{}{}>\n", at("desc", f.desc.as_deref()), at("name", f.name.as_deref()), at("state", f.state.as_deref())));
            for g in &f.coord_geoms {
                write_coord_geom(&mut o, "      ", g, &mut warnings, &format!("PlanFeature '{}'", f.name.as_deref().unwrap_or("")));
            }
            let ordinal = file.plan_features.iter().position(|p| std::ptr::eq(p, f));
            ctx.features_at(&mut o, "      ", "PlanFeature", None, f.name.as_deref(), None, ordinal);
            o.push_str("    </PlanFeature>\n");
        }
        ctx.features_at(&mut o, "    ", "PlanFeatures", None, group, None, Some(gi));
        o.push_str("  </PlanFeatures>\n");
    }

    if !file.surfaces.is_empty() {
        o.push_str("  <Surfaces>\n");
        for s in &file.surfaces {
            write_surface(&mut o, s, &mut ctx, &mut warnings);
        }
        o.push_str("  </Surfaces>\n");
    }
    o.push_str("</LandXML>\n");
    let unwritten: usize = file.object_features.iter().enumerate().filter(|(i, _)| !ctx.written_features.contains(i)).map(|(_, g)| g.features.len()).sum();
    if unwritten > 0 {
        let mut kinds: Vec<String> = file.object_features.iter().enumerate().filter(|(i, _)| !ctx.written_features.contains(i)).map(|(_, g)| g.owner.kind.clone()).collect();
        kinds.sort();
        kinds.dedup();
        warnings.push(format!("{unwritten} Feature element(s) hang on objects the writer does not write them for ({}), or on objects that are not in the file; they were left out.", kinds.join(", ")));
    }
    let dropped_notes = file.field_notes.iter().filter(|f| f.owner.kind != "CoordinateSystem" || file.coordinate_system.is_none()).count();
    if dropped_notes > 0 {
        warnings.push(format!("{dropped_notes} FieldNote element(s) hang on objects other than the coordinate system; they were left out."));
    }
    LandXmlOutput { text: o, warnings }
}
