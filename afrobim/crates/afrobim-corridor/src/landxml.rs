//! LandXML import of TIN surfaces.
//!
//! Reads `LandXML/Surfaces/Surface/Definition` as laid down by the LandXML 1.2 schema
//! (<http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd>; every element and attribute name
//! below was checked against it). Elements are matched by local name, so files in the 1.0, 1.1
//! and 2.0 namespaces read the same way.
//!
//! - `Units/Metric@linearUnit` or `Units/Imperial@linearUnit`: northing, easting and elevation
//!   are all converted to metres by it. `Units@elevationUnit` is not applied: by the schema it is
//!   for elevation attributes, and the alignment reader of this workspace treats it the same way.
//! - `Surface@name` (required by the schema).
//! - `Definition@surfType`: `TIN` (faces of three points) is read. A `grid` (faces of four points;
//!   the schema's description of `F`) is left out with a note, unless
//!   [`SurfaceReadOptions::grid_as_tin`] is set: a cell `a b c d` then becomes the two triangles
//!   `a b c` and `a c d` (the split along the diagonal from its first to its third point is
//!   **this reader's choice**; the schema does not give one), with a note. The raw points and
//!   four-point faces of a grid are in `afrobim_alignment::parse_landxml_file` either way. Only
//!   the first `Definition` of a surface is read.
//! - `Surface@desc`, `Surface@state`, and `SourceData` (`Breaklines`, `Contours`, `Boundaries`,
//!   `DataPoints`, read as by `afrobim_alignment::parse_landxml_file` and kept on the result as
//!   data; they do not shape the TIN, which is what `Definition` says).
//! - `Definition/Pnts/P`: `@id` (a positive integer, unique within the surface) and the text
//!   `northing easting elevation`, **northing first** (the schema: 'a space delimited "northing
//!   easting elevation" text value'). A `P` with no text and a `pntRef` takes the coordinates of
//!   the `CgPoint` of that name. A point becomes X = easting, Y = northing, Z = elevation.
//! - `Definition/Faces/F`: the text is three `P` ids. `@i="1"` marks a face that "is part of
//!   the triangulation but is invisible" (the schema's words); such faces are **skipped**, so
//!   they leave a hole or a notch in the surface. A `Definition` may hold several `Faces`; all
//!   are read.
//!
//! Also read: the `SourceData/PointFiles` (named, as [`LandXmlSourceData::point_files`]; the files are
//! not opened) and the statistics `Definition` states (`area2DSurf`, `area3DSurf`, `elevMin`,
//! `elevMax`) as [`ImportedSurface::stated`]. Civil 3D computes the areas from the visible faces: the
//! plan area of the TIN read here equals `area2DSurf` (checked on a real file, to 1e-3 m2), which is
//! the check that skipping the invisible faces is right.
//!
//! Points that lie far from the rest (a stray or untransformed survey point; 100 times the median
//! distance from the middle of the surface) are kept as written, listed in
//! [`ImportedSurface::outlier_points`] and named in the notes, because they make the plan extent of
//! the TIN much larger than the surface.
//!
//! Not read: `F@n` (neighbours) and `F@b` (breakline sides; derivable from the faces),
//! `SourceData/Chain`, `Watersheds`, `SurfVolumes`.
//!
//! Faces that cannot be used are dropped and counted in a note: a face that does not have
//! exactly three ids (four for a grid), names a point that is not in `Pnts`, or has no area in plan. A surface left
//! with no usable face is not imported, with a note.
//!
//! Tested against hand-made files and a synthetic file with the structure Civil 3D writes
//! (`afrobim-alignment/tests/landxml/civil3d_like.xml`), and, when `AFROBIM_PRIVATE_LANDXML` names it,
//! a real Civil 3D 2025 export of four TIN surfaces (about 20 000 points and 41 000 faces).

use std::collections::HashMap;
use std::path::Path;

use afrobim_alignment::{area_scale, plan_outliers, read_surface_source_data, read_surface_stats, LandXmlSourceData, LandXmlSurfaceStats, OUTLIER_FACTOR};

use crate::tin::Tin;

#[derive(Debug, thiserror::Error)]
pub enum SurfaceXmlError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not well-formed XML: {0}")]
    Xml(String),
    #[error("not a LandXML file: the root element is <{0}>")]
    NotLandXml(String),
    #[error("unsupported linear unit '{0}'")]
    Unit(String),
}

/// One surface read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSurface {
    pub name: String,
    pub description: Option<String>,
    pub state: Option<String>,
    /// `Definition@surfType` as the file wrote it: `TIN` or `grid`.
    pub surf_type: String,
    pub tin: Tin,
    /// What the surface was built from (breaklines, contours, boundaries, data points), in metres.
    pub source_data: LandXmlSourceData,
    /// Faces left out because the file marks them invisible (`i="1"`).
    pub invisible_faces: usize,
    /// What `Definition` states (`area2DSurf`, `area3DSurf`, `elevMin`, `elevMax`), in metres and
    /// square metres. Civil 3D computes the areas from the visible faces, so the plan area of the
    /// triangles read here (invisible faces skipped) matches `area_2d_m2`.
    pub stated: Option<LandXmlSurfaceStats>,
    /// Indices into the TIN's points of the points that lie [`OUTLIER_FACTOR`] times the median
    /// distance or more from the middle of the surface. They are in the TIN as the file gives them.
    pub outlier_points: Vec<usize>,
}

/// The surfaces read from a file, and what the reader has to say about it: surfaces left out and
/// why, faces dropped, assumptions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurfaceImport {
    pub surfaces: Vec<ImportedSurface>,
    pub notes: Vec<String>,
}

/// How surfaces are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurfaceReadOptions {
    /// Read `surfType="grid"` surfaces as TINs, splitting each four-point cell in two triangles.
    pub grid_as_tin: bool,
}

/// Reads the TIN surfaces of a LandXML file.
pub fn read_landxml_surfaces(path: &Path) -> Result<SurfaceImport, SurfaceXmlError> {
    parse_landxml_surfaces(&std::fs::read_to_string(path)?)
}

fn named<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    node.children().filter(move |n| n.is_element() && n.tag_name().name() == name)
}

/// `northing easting elevation` → [easting, northing, elevation] in file units.
fn point_text(text: &str) -> Option<[f64; 3]> {
    let mut it = text.split_whitespace().map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()));
    let (n, e, z) = (it.next()??, it.next()??, it.next()??);
    if it.next().is_some() {
        return None;
    }
    Some([e, n, z])
}

/// Reads the TIN surfaces of a LandXML document held in a string.
pub fn parse_landxml_surfaces(text: &str) -> Result<SurfaceImport, SurfaceXmlError> {
    parse_landxml_surfaces_with(text, &SurfaceReadOptions::default())
}

/// Reads the surfaces of a LandXML document held in a string, with the given options.
pub fn parse_landxml_surfaces_with(text: &str, options: &SurfaceReadOptions) -> Result<SurfaceImport, SurfaceXmlError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| SurfaceXmlError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "LandXML" {
        return Err(SurfaceXmlError::NotLandXml(root.tag_name().name().to_string()));
    }
    let mut import = SurfaceImport::default();
    let unit = named(root, "Units")
        .flat_map(|u| u.children().filter(|n| n.is_element() && matches!(n.tag_name().name(), "Metric" | "Imperial")))
        .find_map(|n| n.attribute("linearUnit"));
    let scale = match unit {
        None => {
            import.notes.push("The file does not state its linear unit; metres assumed.".to_string());
            1.0
        }
        Some("meter") => 1.0,
        Some("millimeter") => 0.001,
        Some("centimeter") => 0.01,
        Some("kilometer") => 1000.0,
        Some("foot") => 0.3048,
        Some("USSurveyFoot") => 1200.0 / 3937.0,
        Some("inch") => 0.0254,
        Some("mile") => 1609.344,
        Some(other) => return Err(SurfaceXmlError::Unit(other.to_string())),
    };

    let square_metres = named(root, "Units")
        .flat_map(|u| u.children().filter(|n| n.is_element() && matches!(n.tag_name().name(), "Metric" | "Imperial")))
        .find_map(|n| n.attribute("areaUnit"))
        .and_then(area_scale);

    // Named points that a P may refer to with pntRef. Only looked for when a P needs one.
    let mut cg_points: Option<HashMap<&str, [f64; 3]>> = None;
    let mut cg = |name: &str| -> Option<[f64; 3]> {
        cg_points
            .get_or_insert_with(|| {
                root.descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "CgPoint")
                    .filter_map(|n| Some((n.attribute("name")?, point_text(n.text().unwrap_or(""))?)))
                    .collect()
            })
            .get(name)
            .copied()
    };

    let mut unnamed = 0;
    for surface in named(root, "Surfaces").flat_map(|g| named(g, "Surface")) {
        let name = match surface.attribute("name") {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => {
                unnamed += 1;
                format!("Surface {unnamed}")
            }
        };
        let Some(definition) = named(surface, "Definition").next() else {
            import.notes.push(format!("Surface '{name}' was not imported: it has no Definition (points and faces)."));
            continue;
        };
        if named(surface, "Definition").count() > 1 {
            import.notes.push(format!("Surface '{name}' has more than one Definition; only the first was read."));
        }
        let surf_type = match definition.attribute("surfType") {
            Some("TIN") => "TIN".to_string(),
            Some("grid") if options.grid_as_tin => "grid".to_string(),
            other => {
                import.notes.push(format!("Surface '{name}' was not imported: its surfType is '{}', and only TIN is read.", other.unwrap_or("")));
                continue;
            }
        };
        let corners = if surf_type == "grid" { 4 } else { 3 };

        let mut points: Vec<[f64; 3]> = Vec::new();
        let mut index: HashMap<u64, u32> = HashMap::new();
        let (mut bad_points, mut duplicate_ids) = (0usize, 0usize);
        for p in named(definition, "Pnts").flat_map(|g| named(g, "P")) {
            let id = p.attribute("id").and_then(|v| v.trim().parse::<u64>().ok());
            let own = p.text().filter(|t| !t.trim().is_empty());
            let xyz = match own {
                Some(t) => point_text(t),
                None => p.attribute("pntRef").and_then(&mut cg),
            };
            let (Some(id), Some(xyz)) = (id, xyz) else {
                bad_points += 1;
                continue;
            };
            if index.insert(id, points.len() as u32).is_some() {
                duplicate_ids += 1;
            }
            points.push(xyz.map(|v| v * scale));
        }
        if bad_points > 0 {
            import.notes.push(format!(
                "Surface '{name}': {bad_points} point(s) were left out because they have no id or not three coordinates (northing easting elevation)."
            ));
        }
        if duplicate_ids > 0 {
            import.notes.push(format!("Surface '{name}': {duplicate_ids} point id(s) occur more than once; the last point with an id is the one used."));
        }

        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let (mut invisible, mut not_three, mut unknown, mut flat) = (0usize, 0usize, 0usize, 0usize);
        for f in named(definition, "Faces").flat_map(|g| named(g, "F")) {
            if f.attribute("i").map(str::trim) == Some("1") {
                invisible += 1;
                continue;
            }
            let ids: Vec<Option<u64>> = f.text().unwrap_or("").split_whitespace().map(|t| t.parse().ok()).collect();
            if ids.len() != corners || ids.iter().any(Option::is_none) {
                not_three += 1;
                continue;
            }
            let corner = |k: usize| ids[k].and_then(|id| index.get(&id).copied());
            let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
                unknown += 1;
                continue;
            };
            let mut cell = vec![[a, b, c]];
            if corners == 4 {
                let Some(d) = corner(3) else {
                    unknown += 1;
                    continue;
                };
                cell.push([a, c, d]);
            }
            for [a, b, c] in cell {
                let (pa, pb, pc) = (points[a as usize], points[b as usize], points[c as usize]);
                if (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]) == 0.0 {
                    flat += 1;
                    continue;
                }
                triangles.push([a, b, c]);
            }
        }
        if corners == 4 && !triangles.is_empty() {
            import.notes.push(format!("Surface '{name}' is a grid: each four-point cell was split into two triangles along the diagonal from its first to its third point."));
        }
        let mut dropped = |count: usize, why: &str| {
            if count > 0 {
                import.notes.push(format!("Surface '{name}': {count} face(s) were left out because they {why}."));
            }
        };
        dropped(not_three, if corners == 4 { "do not have exactly four point ids (a grid)" } else { "do not have exactly three point ids" });
        dropped(unknown, "name a point that is not among the surface's points");
        dropped(flat, "have no area in plan");
        if invisible > 0 {
            import.notes.push(format!("Surface '{name}': {invisible} face(s) marked invisible (i=\"1\") were skipped."));
        }
        match Tin::new(points, triangles) {
            Ok(tin) => {
                let source_data = read_surface_source_data(surface, scale, &mut import.notes);
                let stated = read_surface_stats(surface, scale, square_metres);
                let outlier_points = plan_outliers(tin.points().iter().map(|p| (p[0], p[1]))).map(|o| o.indices).unwrap_or_default();
                if !outlier_points.is_empty() {
                    import.notes.push(format!(
                        "Surface '{name}': {} of {} points lie more than {OUTLIER_FACTOR:.0} times the median distance from the middle of the surface; they are kept as written (see ImportedSurface::outlier_points) and make the plan extent of the TIN much larger than the rest of it.",
                        outlier_points.len(),
                        tin.points().len()
                    ));
                }
                import.surfaces.push(ImportedSurface {
                    name,
                    description: surface.attribute("desc").map(str::to_string),
                    state: surface.attribute("state").map(str::to_string),
                    surf_type,
                    tin,
                    source_data,
                    invisible_faces: invisible,
                    stated,
                    outlier_points,
                })
            }
            Err(why) => import.notes.push(format!("Surface '{name}' was not imported: {why}.")),
        }
    }
    if import.surfaces.is_empty() && import.notes.is_empty() {
        import.notes.push("The file contains no surfaces.".to_string());
    }
    Ok(import)
}
