//! LandXML: everything in a whole-file import except the alignments' own geometry, which
//! `landxml.rs` reads. Plain data in metres, no dependency on any CRS or surface type: the
//! application maps what it needs.
//!
//! Element and attribute names were taken from `LandXML-1.2.xsd`
//! (<http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd>, fetched while this was written;
//! the type of each element is quoted below). Elements are matched by local name, so files in the
//! 1.0, 1.1 and 2.0 namespaces read the same way.
//!
//! [`parse_landxml_file`] / [`read_landxml_file`] read a whole file into a [`LandXmlFile`]:
//!
//! - `LandXML@version date time language`, `Units`, `CoordinateSystem`, `Project`, `Application`;
//! - `Alignments/Alignment` as by [`crate::parse_landxml`], and with them `Profile/ProfSurf`
//!   (existing ground, `PntList2D` of station and elevation) and `CrossSects/CrossSect` with
//!   `CrossSectSurf` and `DesignCrossSectSurf/CrossSectPnt`;
//! - `CgPoints/CgPoint`, with the nested `CgPoints` groups the schema allows;
//! - `Parcels/Parcel` (`Center`, `CoordGeom`, nested `Parcels`), `PlanFeatures/PlanFeature`;
//! - `Surfaces/Surface`: `Definition@surfType` (TIN or grid), `Pnts/P`, `Faces/F`, and
//!   `SourceData` with `Breaklines`, `Contours`, `Boundaries` and `DataPoints`.
//!
//! **Coordinates are kept exactly as the file gives them.** Point text is `northing easting
//! [elevation]`; it becomes X = easting, Y = northing. The only arithmetic is the multiplication by
//! the metres per unit of the file's `linearUnit` (1.0 for metres, so nothing at all happens to a
//! metric file). Nothing is shifted, normalised or rounded, and `UTM`-sized values read back as the
//! same `f64`. Elevations and stations take the linear unit too; `Units@elevationUnit` is reported
//! and, when it names a different unit from the linear one, a warning is made and no conversion is
//! applied. Areas take the factor of `areaUnit`. Angles (`rotationAngle`, the direction units) are
//! not converted and are reported with their unit.
//!
//! Unknown elements are never an error: every element that the reader does not read is counted in
//! [`LandXmlFile::warnings`] and in [`LandXmlSummary::skipped`] by its path
//! (`LandXML/Roadways`, `LandXML/Alignments/Alignment/Feature`, ...). Files from other versions that
//! use the same names are read; a value that cannot be read is dropped with a warning.
//!
//! Not tested against anything but hand-made files and one Civil 3D export; see the crate README.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::landxml::{children, parse_landxml, LandXmlError, LocalName};
use crate::Alignment;

/// A point in metres: X = easting, Y = northing, Z the elevation when the file gives one.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlPoint {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
}

/// An axis-aligned plan extent in metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LandXmlExtent {
    pub min_easting: f64,
    pub max_easting: f64,
    pub min_northing: f64,
    pub max_northing: f64,
}

impl LandXmlExtent {
    pub fn of(x: f64, y: f64) -> Self {
        LandXmlExtent { min_easting: x, max_easting: x, min_northing: y, max_northing: y }
    }
    pub fn include(&mut self, x: f64, y: f64) {
        self.min_easting = self.min_easting.min(x);
        self.max_easting = self.max_easting.max(x);
        self.min_northing = self.min_northing.min(y);
        self.max_northing = self.max_northing.max(y);
    }
    pub fn merge(&mut self, other: &LandXmlExtent) {
        self.include(other.min_easting, other.min_northing);
        self.include(other.max_easting, other.max_northing);
    }
}

fn extend(extent: &mut Option<LandXmlExtent>, x: f64, y: f64) {
    match extent {
        Some(e) => e.include(x, y),
        None => *extent = Some(LandXmlExtent::of(x, y)),
    }
}

fn extend_with(extent: &mut Option<LandXmlExtent>, other: Option<LandXmlExtent>) {
    if let Some(o) = other {
        match extent {
            Some(e) => e.merge(&o),
            None => *extent = Some(o),
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Header, units, coordinate system, project, application
// ----------------------------------------------------------------------------------------------

/// `LandXML` root attributes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlHeader {
    pub version: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
    pub language: Option<String>,
    /// The namespace of the root element, when it has one.
    pub namespace: Option<String>,
}

/// `Units/Metric` or `Units/Imperial`, every attribute the schema defines. The units that have a
/// default in the schema (`angularUnit`, `directionUnit` radians; `latLongAngularUnit` decimal
/// degrees; `elevationUnit` meter) hold it when the file leaves them out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandXmlUnits {
    /// False when the file has no `Units` element; metres were then assumed.
    pub declared: bool,
    /// True for `Imperial`.
    pub imperial: bool,
    pub linear_unit: String,
    pub area_unit: Option<String>,
    pub volume_unit: Option<String>,
    pub temperature_unit: Option<String>,
    pub pressure_unit: Option<String>,
    pub diameter_unit: Option<String>,
    pub width_unit: Option<String>,
    pub height_unit: Option<String>,
    pub velocity_unit: Option<String>,
    pub flow_unit: Option<String>,
    pub angular_unit: String,
    pub direction_unit: String,
    pub lat_long_angular_unit: String,
    pub elevation_unit: String,
    /// Metres per `linear_unit`: what every length, coordinate, station and elevation was multiplied by.
    pub metres_per_linear_unit: f64,
    /// Square metres per `area_unit`, when it is one the reader knows.
    pub square_metres_per_area_unit: Option<f64>,
}

impl Default for LandXmlUnits {
    fn default() -> Self {
        LandXmlUnits {
            declared: false,
            imperial: false,
            linear_unit: "meter".into(),
            area_unit: Some("squareMeter".into()),
            volume_unit: None,
            temperature_unit: None,
            pressure_unit: None,
            diameter_unit: None,
            width_unit: None,
            height_unit: None,
            velocity_unit: None,
            flow_unit: None,
            angular_unit: "radians".into(),
            direction_unit: "radians".into(),
            lat_long_angular_unit: "decimal degrees".into(),
            elevation_unit: "meter".into(),
            metres_per_linear_unit: 1.0,
            square_metres_per_area_unit: Some(1.0),
        }
    }
}

/// Metres per unit for a `metLinear` / `impLinear` name of the schema.
pub(crate) fn linear_scale(unit: &str) -> Option<f64> {
    Some(match unit {
        "meter" => 1.0,
        "millimeter" => 0.001,
        "centimeter" => 0.01,
        "kilometer" => 1000.0,
        "foot" => 0.3048,
        "USSurveyFoot" => 1200.0 / 3937.0,
        "inch" => 0.0254,
        "mile" => 1609.344,
        _ => return None,
    })
}

/// Square metres per unit for a `metArea` / `impArea` name of the schema.
fn area_scale(unit: &str) -> Option<f64> {
    Some(match unit {
        "squareMeter" => 1.0,
        "hectare" => 10_000.0,
        "squareMillimeter" => 1e-6,
        "squareCentimeter" => 1e-4,
        "acre" => 4_046.856_422_4,
        "squareFoot" => 0.092_903_04,
        "squareInch" => 0.000_645_16,
        "squareMiles" => 2_589_988.110_336,
        _ => return None,
    })
}

/// Metres per `elevationType` name of the schema (`meter`, `kilometer`, `feet`, `miles`).
fn elevation_scale(unit: &str) -> Option<f64> {
    Some(match unit {
        "meter" => 1.0,
        "kilometer" => 1000.0,
        "feet" => 0.3048,
        "miles" => 1609.344,
        _ => return None,
    })
}

fn read_units(root: roxmltree::Node, warnings: &mut Vec<String>) -> Result<LandXmlUnits, LandXmlError> {
    let Some(node) = children(root, "Units").flat_map(|u| u.children().filter(|n| n.has_tag_name_local("Metric") || n.has_tag_name_local("Imperial"))).next() else {
        warnings.push("The file does not state its linear unit; metres assumed.".to_string());
        return Ok(LandXmlUnits::default());
    };
    let attr = |name: &str| node.attribute(name).map(str::to_string);
    let linear = attr("linearUnit").unwrap_or_default();
    let scale = match linear_scale(&linear) {
        Some(s) => s,
        None if linear.is_empty() => {
            warnings.push("The file does not state its linear unit; metres assumed.".to_string());
            1.0
        }
        None => return Err(LandXmlError::Unit(linear)),
    };
    let area_unit = attr("areaUnit");
    let square = area_unit.as_deref().and_then(area_scale);
    if area_unit.is_some() && square.is_none() {
        warnings.push(format!("The area unit '{}' is not one the reader knows; areas are reported as stated, without conversion.", area_unit.as_deref().unwrap_or("")));
    }
    let units = LandXmlUnits {
        declared: true,
        imperial: node.has_tag_name_local("Imperial"),
        linear_unit: if linear.is_empty() { "meter".into() } else { linear },
        area_unit,
        volume_unit: attr("volumeUnit"),
        temperature_unit: attr("temperatureUnit"),
        pressure_unit: attr("pressureUnit"),
        diameter_unit: attr("diameterUnit"),
        width_unit: attr("widthUnit"),
        height_unit: attr("heightUnit"),
        velocity_unit: attr("velocityUnit"),
        flow_unit: attr("flowUnit"),
        angular_unit: attr("angularUnit").unwrap_or_else(|| "radians".into()),
        direction_unit: attr("directionUnit").unwrap_or_else(|| "radians".into()),
        lat_long_angular_unit: attr("latLongAngularUnit").unwrap_or_else(|| "decimal degrees".into()),
        elevation_unit: attr("elevationUnit").unwrap_or_else(|| "meter".into()),
        metres_per_linear_unit: scale,
        square_metres_per_area_unit: square,
    };
    if node.attribute("elevationUnit").is_some() {
        match elevation_scale(&units.elevation_unit) {
            Some(e) if (e - scale).abs() > 1e-12 => warnings.push(format!(
                "Units/elevationUnit is '{}' but linearUnit is '{}'; elevations were read in the linear unit, not converted.",
                units.elevation_unit, units.linear_unit
            )),
            Some(_) => {}
            None => warnings.push(format!("Units/elevationUnit '{}' is not a unit the schema defines; elevations were read in the linear unit.", units.elevation_unit)),
        }
    }
    Ok(units)
}

/// `CoordinateSystem`: every attribute the schema defines, as the file wrote it.
/// `rotation_angle` is in the file's `angularUnit` (not converted).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCoordinateSystem {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub epsg_code: Option<String>,
    pub ogc_wkt_code: Option<String>,
    pub horizontal_datum: Option<String>,
    pub vertical_datum: Option<String>,
    pub ellipsoid_name: Option<String>,
    pub horizontal_coordinate_system_name: Option<String>,
    pub geocentric_coordinate_system_name: Option<String>,
    pub file_location: Option<String>,
    pub rotation_angle: Option<f64>,
    pub datum: Option<String>,
    pub fitted_coordinate_system_name: Option<String>,
    pub compound_coordinate_system_name: Option<String>,
    pub local_coordinate_system_name: Option<String>,
    pub geographic_coordinate_system_name: Option<String>,
    pub projected_coordinate_system_name: Option<String>,
    pub vertical_coordinate_system_name: Option<String>,
}

fn read_coordinate_system(node: roxmltree::Node, warnings: &mut Vec<String>) -> LandXmlCoordinateSystem {
    let a = |name: &str| node.attribute(name).map(str::to_string);
    let rotation_angle = match node.attribute("rotationAngle") {
        None => None,
        Some(t) => match t.trim().parse::<f64>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => {
                warnings.push(format!("CoordinateSystem@rotationAngle is not a number: '{t}'."));
                None
            }
        },
    };
    LandXmlCoordinateSystem {
        name: a("name"),
        desc: a("desc"),
        epsg_code: a("epsgCode"),
        ogc_wkt_code: a("ogcWktCode"),
        horizontal_datum: a("horizontalDatum"),
        vertical_datum: a("verticalDatum"),
        ellipsoid_name: a("ellipsoidName"),
        horizontal_coordinate_system_name: a("horizontalCoordinateSystemName"),
        geocentric_coordinate_system_name: a("geocentricCoordinateSystemName"),
        file_location: a("fileLocation"),
        rotation_angle,
        datum: a("datum"),
        fitted_coordinate_system_name: a("fittedCoordinateSystemName"),
        compound_coordinate_system_name: a("compoundCoordinateSystemName"),
        local_coordinate_system_name: a("localCoordinateSystemName"),
        geographic_coordinate_system_name: a("geographicCoordinateSystemName"),
        projected_coordinate_system_name: a("projectedCoordinateSystemName"),
        vertical_coordinate_system_name: a("verticalCoordinateSystemName"),
    }
}

/// `Project`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlProject {
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
}

/// `Application`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlApplication {
    pub name: String,
    pub desc: Option<String>,
    pub manufacturer: Option<String>,
    pub version: Option<String>,
    pub manufacturer_url: Option<String>,
    pub time_stamp: Option<String>,
}

impl LandXmlApplication {
    /// True when the name, description or manufacturer says Civil 3D (case-insensitive match on
    /// "civil 3d" or "civil3d"). A heuristic about the text of the file, not a check of its origin.
    pub fn is_civil_3d(&self) -> bool {
        [Some(self.name.as_str()), self.desc.as_deref(), self.manufacturer.as_deref()]
            .into_iter()
            .flatten()
            .any(|s| {
                let s = s.to_ascii_lowercase();
                s.contains("civil 3d") || s.contains("civil3d")
            })
    }
}

// ----------------------------------------------------------------------------------------------
// Control points
// ----------------------------------------------------------------------------------------------

/// `CgPoints/CgPoint`: `name`, `code`, `desc`, `state`, `oID`, `pointGeometry`, `DTMAttribute`,
/// `latitude`, `longitude` and the text `northing easting [elevation]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCgPoint {
    pub name: String,
    pub code: Option<String>,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub oid: Option<String>,
    pub point_geometry: Option<String>,
    pub dtm_attribute: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub easting_m: f64,
    pub northing_m: f64,
    pub elevation_m: Option<f64>,
}

/// A `CgPoints` element: its points and the `CgPoints` nested in it (the schema nests them), with
/// the attributes `name`, `desc`, `code`, `state`, `zoneNumber`, `DTMAttribute`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCgPointGroup {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub code: Option<String>,
    pub state: Option<String>,
    pub zone_number: Option<u32>,
    pub dtm_attribute: Option<String>,
    pub points: Vec<LandXmlCgPoint>,
    pub groups: Vec<LandXmlCgPointGroup>,
}

impl LandXmlCgPointGroup {
    /// Every point of the group and of the groups below it, in file order.
    pub fn all_points(&self) -> Vec<&LandXmlCgPoint> {
        let mut out: Vec<&LandXmlCgPoint> = self.points.iter().collect();
        for g in &self.groups {
            out.extend(g.all_points());
        }
        out
    }
}

// ----------------------------------------------------------------------------------------------
// Plan geometry (parcels, plan features)
// ----------------------------------------------------------------------------------------------

/// One element of a `CoordGeom`, in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LandXmlGeomElement {
    Line { start: LandXmlPoint, end: LandXmlPoint },
    /// `Curve`: `rot` is `cw` for `clockwise`. `radius_m` and `length_m` are the stated attributes.
    Curve { start: LandXmlPoint, center: LandXmlPoint, end: LandXmlPoint, pi: Option<LandXmlPoint>, clockwise: bool, radius_m: Option<f64>, length_m: Option<f64> },
    /// `Spiral`: `radius_*_m` is `None` for `INF`.
    Spiral {
        start: LandXmlPoint,
        pi: Option<LandXmlPoint>,
        end: LandXmlPoint,
        spiral_type: String,
        clockwise: bool,
        length_m: f64,
        radius_start_m: Option<f64>,
        radius_end_m: Option<f64>,
    },
    /// `IrregularLine`: `points` is the whole list the file gives (the schema says it includes the start and end).
    IrregularLine { start: LandXmlPoint, end: LandXmlPoint, points: Vec<LandXmlPoint> },
    /// `Chain`: the `CgPoint` names, with the coordinates of those that were found.
    Chain { names: Vec<String>, points: Vec<LandXmlPoint> },
}

/// A `CoordGeom`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCoordGeom {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub elements: Vec<LandXmlGeomElement>,
}

impl LandXmlCoordGeom {
    /// The points the geometry is made of, in order: for each element its start (and its end after
    /// the last), irregular-line and chain vertices in full. Arcs are not densified.
    pub fn vertices(&self) -> Vec<LandXmlPoint> {
        let mut out: Vec<LandXmlPoint> = Vec::new();
        let mut push = |p: LandXmlPoint| {
            if out.last() != Some(&p) {
                out.push(p);
            }
        };
        for e in &self.elements {
            match e {
                LandXmlGeomElement::Line { start, end } => {
                    push(*start);
                    push(*end);
                }
                LandXmlGeomElement::Curve { start, end, .. } | LandXmlGeomElement::Spiral { start, end, .. } => {
                    push(*start);
                    push(*end);
                }
                LandXmlGeomElement::IrregularLine { points, start, end } => {
                    if points.is_empty() {
                        push(*start);
                        push(*end);
                    }
                    points.iter().for_each(|p| push(*p));
                }
                LandXmlGeomElement::Chain { points, .. } => points.iter().for_each(|p| push(*p)),
            }
        }
        out
    }

    /// The plan extent of the stated points, with the true extent of each circular arc (its start,
    /// its end and the four axis extremes of the circle that fall within the sweep). A spiral
    /// contributes its ends only.
    pub fn extent(&self) -> Option<LandXmlExtent> {
        let mut ext = None;
        for p in self.vertices() {
            extend(&mut ext, p.x, p.y);
        }
        for e in &self.elements {
            if let LandXmlGeomElement::Curve { start, center, end, clockwise, .. } = e {
                let r = (start.x - center.x).hypot(start.y - center.y);
                let a0 = (start.y - center.y).atan2(start.x - center.x);
                let a1 = (end.y - center.y).atan2(end.x - center.x);
                let turn = if *clockwise { -1.0 } else { 1.0 };
                let sweep = (turn * (a1 - a0)).rem_euclid(std::f64::consts::TAU);
                for k in 0..4 {
                    let theta = k as f64 * std::f64::consts::FRAC_PI_2;
                    if (turn * (theta - a0)).rem_euclid(std::f64::consts::TAU) <= sweep {
                        extend(&mut ext, center.x + r * theta.cos(), center.y + r * theta.sin());
                    }
                }
            }
        }
        ext
    }
}

/// `Parcels/Parcel`. `area_m2` is `area` converted by `Units@areaUnit`; `area` is as written.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlParcel {
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub owner: Option<String>,
    pub parcel_type: Option<String>,
    pub class: Option<String>,
    pub area: Option<f64>,
    pub area_m2: Option<f64>,
    pub center: Option<LandXmlPoint>,
    /// The `CoordGeom` of the parcel (its boundary: lines, curves, spirals).
    pub coord_geoms: Vec<LandXmlCoordGeom>,
    /// Parcels nested in `Parcel/Parcels`.
    pub parcels: Vec<LandXmlParcel>,
}

impl LandXmlParcel {
    fn extent(&self) -> Option<LandXmlExtent> {
        let mut ext = None;
        if let Some(c) = self.center {
            extend(&mut ext, c.x, c.y);
        }
        for g in &self.coord_geoms {
            extend_with(&mut ext, g.extent());
        }
        for p in &self.parcels {
            extend_with(&mut ext, p.extent());
        }
        ext
    }
    fn count(&self) -> usize {
        1 + self.parcels.iter().map(LandXmlParcel::count).sum::<usize>()
    }
}

/// `PlanFeatures/PlanFeature`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlPlanFeature {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub state: Option<String>,
    /// The `name` of the enclosing `PlanFeatures`.
    pub group: Option<String>,
    pub coord_geoms: Vec<LandXmlCoordGeom>,
}

// ----------------------------------------------------------------------------------------------
// Profiles and cross-sections of alignments
// ----------------------------------------------------------------------------------------------

/// `Profile/ProfSurf`: an existing-ground profile. Each `PntList2D` is one run of
/// `[station_m, elevation_m]`; the schema starts a new list for a gap.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlProfSurf {
    /// `name` of the `Alignment` it belongs to.
    pub alignment: String,
    /// `name` of the enclosing `Profile`.
    pub profile: Option<String>,
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub point_lists: Vec<Vec<[f64; 2]>>,
}

/// `CrossSectSurf`: runs of `[offset_m, elevation_m]` from the centreline (`PntList2D`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCrossSectSurf {
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub point_lists: Vec<Vec<[f64; 2]>>,
}

/// `CrossSectPnt` with `dataFormat="Offset Elevation"` (the schema's default).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LandXmlCrossSectPoint {
    pub offset_m: f64,
    pub elevation_m: f64,
}

/// `DesignCrossSectSurf`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlDesignCrossSectSurf {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub state: Option<String>,
    /// `right`, `left` or `both`.
    pub side: Option<String>,
    pub material: Option<String>,
    pub closed_area: Option<bool>,
    pub points: Vec<LandXmlCrossSectPoint>,
}

/// `CrossSects/CrossSect`: one station of one alignment. `station_m` is `sta` in metres, as written
/// (the schema does not say whether it is internal or displayed).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCrossSection {
    pub alignment: String,
    pub station_m: f64,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub surfaces: Vec<LandXmlCrossSectSurf>,
    pub design_surfaces: Vec<LandXmlDesignCrossSectSurf>,
}

// ----------------------------------------------------------------------------------------------
// Surfaces
// ----------------------------------------------------------------------------------------------

/// `Definition/Pnts/P`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LandXmlSurfPoint {
    pub id: u64,
    pub easting_m: f64,
    pub northing_m: f64,
    pub elevation_m: f64,
}

/// `Definition/Faces/F`: three point ids for a TIN, four for a grid. `invisible` is `i="1"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandXmlSurfFace {
    pub ids: Vec<u64>,
    pub invisible: bool,
}

/// `Breaklines/Breakline`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlBreakline {
    pub name: Option<String>,
    pub desc: Option<String>,
    /// `standard`, `wall`, `proximity` or `nondestructive`.
    pub brk_type: Option<String>,
    pub state: Option<String>,
    /// From `PntList2D` (no `z`) or `PntList3D`.
    pub points: Vec<LandXmlPoint>,
}

/// `Contours/Contour`: `elev` and the `PntList2D` of northing and easting.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlContour {
    pub elevation_m: f64,
    pub points: Vec<LandXmlPoint>,
}

/// `Boundaries/Boundary`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlBoundary {
    pub name: Option<String>,
    pub desc: Option<String>,
    /// `outer`, `void` or `island`.
    pub bnd_type: String,
    pub edge_trim: bool,
    pub state: Option<String>,
    pub area: Option<f64>,
    pub points: Vec<LandXmlPoint>,
}

/// `DataPoints`: the `PntList3D` elements, joined.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlDataPoints {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub code: Option<String>,
    pub state: Option<String>,
    pub points: Vec<LandXmlPoint>,
}

/// `Surface/SourceData`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSourceData {
    pub breaklines: Vec<LandXmlBreakline>,
    pub contours: Vec<LandXmlContour>,
    pub boundaries: Vec<LandXmlBoundary>,
    pub data_points: Vec<LandXmlDataPoints>,
}

impl LandXmlSourceData {
    pub fn is_empty(&self) -> bool {
        self.breaklines.is_empty() && self.contours.is_empty() && self.boundaries.is_empty() && self.data_points.is_empty()
    }
    fn extent(&self) -> Option<LandXmlExtent> {
        let mut ext = None;
        let lists = self
            .breaklines
            .iter()
            .map(|b| &b.points)
            .chain(self.contours.iter().map(|c| &c.points))
            .chain(self.boundaries.iter().map(|b| &b.points))
            .chain(self.data_points.iter().map(|d| &d.points));
        for list in lists {
            for p in list {
                extend(&mut ext, p.x, p.y);
            }
        }
        ext
    }
}

/// `Surfaces/Surface`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSurface {
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    /// `Definition@surfType`: `TIN` or `grid`; `None` when the surface has no `Definition`.
    pub surf_type: Option<String>,
    pub points: Vec<LandXmlSurfPoint>,
    pub faces: Vec<LandXmlSurfFace>,
    pub source_data: LandXmlSourceData,
}

impl LandXmlSurface {
    pub fn extent(&self) -> Option<LandXmlExtent> {
        let mut ext = None;
        for p in &self.points {
            extend(&mut ext, p.easting_m, p.northing_m);
        }
        extend_with(&mut ext, self.source_data.extent());
        ext
    }
}

// ----------------------------------------------------------------------------------------------
// The file
// ----------------------------------------------------------------------------------------------

/// Count and plan extent of one class of object.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlClassSummary {
    pub count: usize,
    pub extent: Option<LandXmlExtent>,
}

/// What a file holds, for an import dialog and a coordinate check before anything is placed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSummary {
    /// Imported alignments; the extent is sampled along their elements (every 2 m or so).
    pub alignments: LandXmlClassSummary,
    /// Control points, in all groups.
    pub cg_points: LandXmlClassSummary,
    /// Parcels, nested ones included.
    pub parcels: LandXmlClassSummary,
    pub plan_features: LandXmlClassSummary,
    /// Surfaces; the extent is that of their points and source data.
    pub surfaces: LandXmlClassSummary,
    pub ground_profiles: usize,
    pub cross_sections: usize,
    /// Further counts by name: `cg_point_groups`, `surface_points`, `surface_faces`, `breaklines`,
    /// `contours`, `boundaries`, `data_point_sets`, `irregular_lines`, `chains`, `station_equations`.
    pub counts: BTreeMap<String, usize>,
    /// Elements that were not read, by path, with how many there were.
    pub skipped: BTreeMap<String, usize>,
}

impl LandXmlSummary {
    /// The extent of everything that has one.
    pub fn overall_extent(&self) -> Option<LandXmlExtent> {
        let mut ext = None;
        for c in [&self.alignments, &self.cg_points, &self.parcels, &self.plan_features, &self.surfaces] {
            extend_with(&mut ext, c.extent);
        }
        ext
    }
}

/// A whole LandXML file, read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandXmlFile {
    pub header: LandXmlHeader,
    pub units: LandXmlUnits,
    pub coordinate_system: Option<LandXmlCoordinateSystem>,
    pub project: Option<LandXmlProject>,
    pub applications: Vec<LandXmlApplication>,
    pub alignments: Vec<Alignment>,
    pub cg_points: Vec<LandXmlCgPointGroup>,
    pub parcels: Vec<LandXmlParcel>,
    pub plan_features: Vec<LandXmlPlanFeature>,
    pub ground_profiles: Vec<LandXmlProfSurf>,
    pub cross_sections: Vec<LandXmlCrossSection>,
    pub surfaces: Vec<LandXmlSurface>,
    /// Everything the reader has to say: alignments left out and why, assumptions, values dropped,
    /// and the elements it skipped, counted.
    pub warnings: Vec<String>,
    pub summary: LandXmlSummary,
}

impl LandXmlFile {
    /// True when an `Application` of the file says Civil 3D ([`LandXmlApplication::is_civil_3d`]).
    pub fn is_civil_3d(&self) -> bool {
        self.applications.iter().any(LandXmlApplication::is_civil_3d)
    }
}

/// Reads a whole LandXML file.
pub fn read_landxml_file(path: &Path) -> Result<LandXmlFile, LandXmlError> {
    parse_landxml_file(&std::fs::read_to_string(path)?)
}

/// Reads a whole LandXML document held in a string. Errors only for what is not LandXML at all
/// (not XML, wrong root, a linear unit outside the schema's list); everything else is read as far as
/// it can be and the rest is reported in [`LandXmlFile::warnings`].
pub fn parse_landxml_file(text: &str) -> Result<LandXmlFile, LandXmlError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| LandXmlError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "LandXML" {
        return Err(LandXmlError::NotLandXml(root.tag_name().name().to_string()));
    }
    // Alignments first: this checks the linear unit too, and returns its own notes.
    let import = parse_landxml(text)?;
    let mut warnings: Vec<String> = Vec::new();
    let units = read_units(root, &mut warnings)?;
    // `parse_landxml` has already said "no linear unit" and "no alignments"; do not repeat the first.
    warnings.retain(|w| !import.notes.contains(w));
    let mut all_warnings = import.notes.clone();
    all_warnings.extend(warnings);

    let header = LandXmlHeader {
        version: root.attribute("version").map(str::to_string),
        date: root.attribute("date").map(str::to_string),
        time: root.attribute("time").map(str::to_string),
        language: root.attribute("language").map(str::to_string),
        namespace: root.tag_name().namespace().map(str::to_string),
    };

    let mut rd = Rd { scale: units.metres_per_linear_unit, area_scale: units.square_metres_per_area_unit, points: HashMap::new(), warnings: Vec::new() };
    for n in root.descendants().filter(|n| n.has_tag_name_local("CgPoint")) {
        if let (Some(name), Some(p)) = (n.attribute("name"), parse_point(n.text().unwrap_or(""), rd.scale)) {
            rd.points.insert(name.to_string(), p);
        }
    }

    let coordinate_system = {
        let all: Vec<_> = children(root, "CoordinateSystem").collect();
        if all.len() > 1 {
            rd.warnings.push(format!("The file has {} CoordinateSystem elements; the first is used.", all.len()));
        }
        all.first().map(|n| read_coordinate_system(*n, &mut rd.warnings))
    };
    let project = children(root, "Project").next().map(|n| LandXmlProject {
        name: n.attribute("name").unwrap_or("").to_string(),
        desc: n.attribute("desc").map(str::to_string),
        state: n.attribute("state").map(str::to_string),
    });
    let applications = children(root, "Application")
        .map(|n| LandXmlApplication {
            name: n.attribute("name").unwrap_or("").to_string(),
            desc: n.attribute("desc").map(str::to_string),
            manufacturer: n.attribute("manufacturer").map(str::to_string),
            version: n.attribute("version").map(str::to_string),
            manufacturer_url: n.attribute("manufacturerURL").map(str::to_string),
            time_stamp: n.attribute("timeStamp").map(str::to_string),
        })
        .collect();

    let cg_points: Vec<LandXmlCgPointGroup> = children(root, "CgPoints").map(|n| rd.cg_group(n)).collect();
    let mut parcels = Vec::new();
    for group in children(root, "Parcels") {
        for p in children(group, "Parcel") {
            parcels.push(rd.parcel(p));
        }
    }
    let mut plan_features = Vec::new();
    for group in children(root, "PlanFeatures") {
        for f in children(group, "PlanFeature") {
            plan_features.push(LandXmlPlanFeature {
                name: f.attribute("name").map(str::to_string),
                desc: f.attribute("desc").map(str::to_string),
                state: f.attribute("state").map(str::to_string),
                group: group.attribute("name").filter(|n| !n.is_empty()).map(str::to_string),
                coord_geoms: children(f, "CoordGeom").map(|g| rd.coord_geom(g)).collect(),
            });
        }
    }
    let mut ground_profiles = Vec::new();
    let mut cross_sections = Vec::new();
    let mut unnamed = 0;
    for node in children(root, "Alignments").flat_map(|g| children(g, "Alignment")) {
        // The same name the alignment reader gave it.
        let name = match node.attribute("name") {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => {
                unnamed += 1;
                format!("Alignment {unnamed}")
            }
        };
        for profile in children(node, "Profile") {
            for s in children(profile, "ProfSurf") {
                ground_profiles.push(LandXmlProfSurf {
                    alignment: name.clone(),
                    profile: profile.attribute("name").map(str::to_string),
                    name: s.attribute("name").unwrap_or("").to_string(),
                    desc: s.attribute("desc").map(str::to_string),
                    state: s.attribute("state").map(str::to_string),
                    point_lists: rd.pair_lists(s, "ProfSurf"),
                });
            }
        }
        for cs in children(node, "CrossSects").flat_map(|g| children(g, "CrossSect")) {
            let Some(sta) = cs.attribute("sta").and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite()) else {
                rd.warnings.push(format!("A CrossSect of alignment '{name}' has no usable sta and was left out."));
                continue;
            };
            cross_sections.push(rd.cross_section(cs, &name, sta));
        }
    }
    let mut surfaces = Vec::new();
    let mut unnamed_surfaces = 0;
    for s in children(root, "Surfaces").flat_map(|g| children(g, "Surface")) {
        surfaces.push(rd.surface(s, &mut unnamed_surfaces));
    }

    all_warnings.extend(rd.warnings);
    let mut skipped = BTreeMap::new();
    count_unknown(root, "LandXML", &mut skipped);
    for (path, n) in &skipped {
        all_warnings.push(format!("{n} element{} at {path} {} not read.", if *n == 1 { "" } else { "s" }, if *n == 1 { "was" } else { "were" }));
    }
    // A file of surfaces or points is not at fault for having no alignments.
    if import.alignments.is_empty() && !(cg_points.is_empty() && parcels.is_empty() && plan_features.is_empty() && surfaces.is_empty()) {
        all_warnings.retain(|w| w != "The file contains no alignments.");
    }

    let mut file = LandXmlFile {
        header,
        units,
        coordinate_system,
        project,
        applications,
        alignments: import.alignments,
        cg_points,
        parcels,
        plan_features,
        ground_profiles,
        cross_sections,
        surfaces,
        warnings: all_warnings,
        summary: LandXmlSummary::default(),
    };
    file.summary = summarize(&file, skipped);
    Ok(file)
}

fn summarize(f: &LandXmlFile, skipped: BTreeMap<String, usize>) -> LandXmlSummary {
    let mut s = LandXmlSummary { skipped, ..Default::default() };

    s.alignments.count = f.alignments.len();
    for a in &f.alignments {
        for e in &a.elements {
            let n = ((e.length / 2.0).ceil() as usize).clamp(2, 100_000);
            for i in 0..=n {
                let (x, y) = e.point_at(e.length * i as f64 / n as f64);
                extend(&mut s.alignments.extent, x, y);
            }
            let (x, y, _) = e.end();
            extend(&mut s.alignments.extent, x, y);
        }
    }

    for g in &f.cg_points {
        for p in g.all_points() {
            s.cg_points.count += 1;
            extend(&mut s.cg_points.extent, p.easting_m, p.northing_m);
        }
    }
    fn count_groups(g: &LandXmlCgPointGroup) -> usize {
        1 + g.groups.iter().map(count_groups).sum::<usize>()
    }
    let groups: usize = f.cg_points.iter().map(count_groups).sum();
    for p in &f.parcels {
        s.parcels.count += p.count();
        extend_with(&mut s.parcels.extent, p.extent());
    }
    s.plan_features.count = f.plan_features.len();
    for p in &f.plan_features {
        for g in &p.coord_geoms {
            extend_with(&mut s.plan_features.extent, g.extent());
        }
    }
    s.surfaces.count = f.surfaces.len();
    for surf in &f.surfaces {
        extend_with(&mut s.surfaces.extent, surf.extent());
    }
    s.ground_profiles = f.ground_profiles.len();
    s.cross_sections = f.cross_sections.len();

    let mut c = |name: &str, n: usize| {
        if n > 0 {
            s.counts.insert(name.to_string(), n);
        }
    };
    c("cg_point_groups", groups);
    c("surface_points", f.surfaces.iter().map(|x| x.points.len()).sum());
    c("surface_faces", f.surfaces.iter().map(|x| x.faces.len()).sum());
    c("breaklines", f.surfaces.iter().map(|x| x.source_data.breaklines.len()).sum());
    c("contours", f.surfaces.iter().map(|x| x.source_data.contours.len()).sum());
    c("boundaries", f.surfaces.iter().map(|x| x.source_data.boundaries.len()).sum());
    c("data_point_sets", f.surfaces.iter().map(|x| x.source_data.data_points.len()).sum());
    c("station_equations", f.alignments.iter().map(|a| a.station_equations.equations.len()).sum());
    let geoms = || f.parcels.iter().flat_map(all_parcel_geoms).chain(f.plan_features.iter().flat_map(|p| p.coord_geoms.iter()));
    c("irregular_lines", geoms().flat_map(|g| g.elements.iter()).filter(|e| matches!(e, LandXmlGeomElement::IrregularLine { .. })).count());
    c("chains", geoms().flat_map(|g| g.elements.iter()).filter(|e| matches!(e, LandXmlGeomElement::Chain { .. })).count());
    s
}

fn all_parcel_geoms(p: &LandXmlParcel) -> Vec<&LandXmlCoordGeom> {
    let mut v: Vec<&LandXmlCoordGeom> = p.coord_geoms.iter().collect();
    for n in &p.parcels {
        v.extend(all_parcel_geoms(n));
    }
    v
}

// ----------------------------------------------------------------------------------------------
// Counting what is not read
// ----------------------------------------------------------------------------------------------

/// The children of each element that the reader reads. Anything else under a listed element is
/// skipped and counted. Elements that are not listed are leaves as far as the reader goes.
fn known_children(parent: &str) -> Option<&'static [&'static str]> {
    Some(match parent {
        "LandXML" => &["Units", "CoordinateSystem", "Project", "Application", "Alignments", "CgPoints", "Parcels", "PlanFeatures", "Surfaces"],
        "Units" => &["Metric", "Imperial"],
        "Alignments" => &["Alignment"],
        "Alignment" => &["CoordGeom", "StaEquation", "Profile", "CrossSects", "Superelevation"],
        "CoordGeom" => &["Line", "Curve", "Spiral", "IrregularLine", "Chain"],
        "Line" => &["Start", "End"],
        "Curve" => &["Start", "Center", "End", "PI"],
        "Spiral" => &["Start", "PI", "End"],
        "IrregularLine" => &["Start", "End", "PntList2D", "PntList3D"],
        "Profile" => &["ProfAlign", "ProfSurf"],
        "ProfAlign" => &["PVI", "ParaCurve", "UnsymParaCurve", "CircCurve"],
        "ProfSurf" => &["PntList2D"],
        "CrossSects" => &["CrossSect"],
        "CrossSect" => &["CrossSectSurf", "DesignCrossSectSurf"],
        "CrossSectSurf" => &["PntList2D"],
        "DesignCrossSectSurf" => &["CrossSectPnt"],
        "Superelevation" => &["BeginRunoutSta", "BeginRunoffSta", "FullSuperSta", "RunoffSta", "StartofRunoutSta", "EndofRunoutSta", "FullSuperelev", "AdverseSE"],
        "CgPoints" => &["CgPoint", "CgPoints"],
        "Parcels" => &["Parcel"],
        "Parcel" => &["Center", "CoordGeom", "Parcels"],
        "PlanFeatures" => &["PlanFeature"],
        "PlanFeature" => &["CoordGeom"],
        "Surfaces" => &["Surface"],
        "Surface" => &["Definition", "SourceData"],
        "Definition" => &["Pnts", "Faces"],
        "Pnts" => &["P"],
        "Faces" => &["F"],
        "SourceData" => &["Breaklines", "Contours", "Boundaries", "DataPoints"],
        "Breaklines" => &["Breakline"],
        "Contours" => &["Contour"],
        "Boundaries" => &["Boundary"],
        "Breakline" | "Boundary" => &["PntList2D", "PntList3D"],
        "Contour" => &["PntList2D"],
        "DataPoints" => &["PntList3D"],
        "CoordinateSystem" | "Project" | "Application" => &[],
        _ => return None,
    })
}

fn count_unknown(node: roxmltree::Node, path: &str, out: &mut BTreeMap<String, usize>) {
    let Some(known) = known_children(node.tag_name().name()) else { return };
    for c in node.children().filter(|n| n.is_element()) {
        let name = c.tag_name().name();
        if known.contains(&name) {
            count_unknown(c, &format!("{path}/{name}"), out);
        } else {
            *out.entry(format!("{path}/{name}")).or_insert(0) += 1;
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Reading
// ----------------------------------------------------------------------------------------------

/// `northing easting [elevation]` → a point in metres. 2 or 3 numbers.
fn parse_point(text: &str, scale: f64) -> Option<LandXmlPoint> {
    let v = numbers(text)?;
    match v.as_slice() {
        [n, e] => Some(LandXmlPoint { x: e * scale, y: n * scale, z: None }),
        [n, e, z] => Some(LandXmlPoint { x: e * scale, y: n * scale, z: Some(z * scale) }),
        _ => None,
    }
}

fn numbers(text: &str) -> Option<Vec<f64>> {
    text.split_whitespace().map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

/// The numbers of a point list, `None` if one is not a number.
pub(crate) fn list_numbers(text: &str) -> Option<Vec<f64>> {
    numbers(text)
}

struct Rd {
    scale: f64,
    area_scale: Option<f64>,
    points: HashMap<String, LandXmlPoint>,
    warnings: Vec<String>,
}

impl Rd {
    /// The point of a `Start`, `End`, `Center`, `PI` or `P`-like node: its text, or a `pntRef`.
    fn node_point(&self, node: roxmltree::Node) -> Option<LandXmlPoint> {
        let text = node.text().unwrap_or("").trim();
        if !text.is_empty() {
            return parse_point(text, self.scale);
        }
        node.attribute("pntRef").and_then(|r| self.points.get(r).copied())
    }

    /// A point child that may be absent (`PI`).
    fn opt_point(&mut self, parent: roxmltree::Node, name: &'static str, what: &str) -> Option<LandXmlPoint> {
        children(parent, name).next()?;
        self.child_point(parent, name, what)
    }

    fn child_point(&mut self, parent: roxmltree::Node, name: &'static str, what: &str) -> Option<LandXmlPoint> {
        let Some(node) = children(parent, name).next() else {
            self.warnings.push(format!("{what}: it has no <{name}>."));
            return None;
        };
        let p = self.node_point(node);
        if p.is_none() {
            self.warnings.push(format!("{what}: the <{name}> has no readable coordinates (northing easting [elevation], or a pntRef that is in the file)."));
        }
        p
    }

    /// The runs of a `PntList2D` of pairs (station/offset first), in metres.
    fn pair_lists(&mut self, parent: roxmltree::Node, what: &str) -> Vec<Vec<[f64; 2]>> {
        let mut out = Vec::new();
        for list in children(parent, "PntList2D") {
            match numbers(list.text().unwrap_or("")) {
                Some(v) if v.len() >= 2 => {
                    if v.len() % 2 != 0 {
                        self.warnings.push(format!("{what}: a PntList2D has an odd number of values; the last one was dropped."));
                    }
                    out.push(v.chunks_exact(2).map(|c| [c[0] * self.scale, c[1] * self.scale]).collect());
                }
                _ => self.warnings.push(format!("{what}: a PntList2D is empty or has a value that is not a number; it was left out.")),
            }
        }
        out
    }

    /// A `PntList2D` of `northing easting` or a `PntList3D` of `northing easting elevation`.
    fn plan_list(&mut self, parent: roxmltree::Node, what: &str) -> Vec<LandXmlPoint> {
        let mut out = Vec::new();
        for (name, stride) in [("PntList2D", 2usize), ("PntList3D", 3)] {
            for list in children(parent, name) {
                let Some(v) = numbers(list.text().unwrap_or("")) else {
                    self.warnings.push(format!("{what}: a {name} has a value that is not a number; it was left out."));
                    continue;
                };
                if v.len() % stride != 0 {
                    self.warnings.push(format!("{what}: a {name} has {} values, not a multiple of {stride}; the last {} were dropped.", v.len(), v.len() % stride));
                }
                for c in v.chunks_exact(stride) {
                    out.push(LandXmlPoint { x: c[1] * self.scale, y: c[0] * self.scale, z: (stride == 3).then(|| c[2] * self.scale) });
                }
            }
        }
        out
    }

    fn cg_group(&mut self, node: roxmltree::Node) -> LandXmlCgPointGroup {
        let mut group = LandXmlCgPointGroup {
            name: node.attribute("name").map(str::to_string),
            desc: node.attribute("desc").map(str::to_string),
            code: node.attribute("code").map(str::to_string),
            state: node.attribute("state").map(str::to_string),
            zone_number: node.attribute("zoneNumber").and_then(|t| t.trim().parse().ok()),
            dtm_attribute: node.attribute("DTMAttribute").map(str::to_string),
            ..Default::default()
        };
        for c in node.children().filter(|n| n.is_element()) {
            match c.tag_name().name() {
                "CgPoint" => {
                    let name = c.attribute("name").unwrap_or("").to_string();
                    let own = c.text().filter(|t| !t.trim().is_empty());
                    let p = match own {
                        Some(t) => parse_point(t, self.scale),
                        None => c.attribute("pntRef").and_then(|r| self.points.get(r).copied()),
                    };
                    let Some(p) = p else {
                        self.warnings.push(format!("CgPoint '{name}' has no readable coordinates (northing easting [elevation]) and was left out."));
                        continue;
                    };
                    let angle = |a: &str| c.attribute(a).and_then(|t| t.trim().parse::<f64>().ok());
                    group.points.push(LandXmlCgPoint {
                        name,
                        code: c.attribute("code").map(str::to_string),
                        desc: c.attribute("desc").map(str::to_string),
                        state: c.attribute("state").map(str::to_string),
                        oid: c.attribute("oID").map(str::to_string),
                        point_geometry: c.attribute("pointGeometry").map(str::to_string),
                        dtm_attribute: c.attribute("DTMAttribute").map(str::to_string),
                        latitude: angle("latitude"),
                        longitude: angle("longitude"),
                        easting_m: p.x,
                        northing_m: p.y,
                        elevation_m: p.z,
                    });
                }
                "CgPoints" => {
                    let g = self.cg_group(c);
                    group.groups.push(g);
                }
                _ => {}
            }
        }
        group
    }

    fn coord_geom(&mut self, node: roxmltree::Node) -> LandXmlCoordGeom {
        let mut g = LandXmlCoordGeom {
            name: node.attribute("name").map(str::to_string),
            desc: node.attribute("desc").map(str::to_string),
            state: node.attribute("state").map(str::to_string),
            elements: Vec::new(),
        };
        let label = g.name.clone().map_or("a CoordGeom".to_string(), |n| format!("CoordGeom '{n}'"));
        for (i, item) in node.children().filter(|n| n.is_element()).enumerate() {
            let kind = item.tag_name().name().to_string();
            let what = format!("{label}, element {} ({kind})", i + 1);
            let number = |name: &str, scale: f64| item.attribute(name).and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v * scale);
            let clockwise = match item.attribute("rot") {
                Some("cw") => Some(true),
                Some("ccw") => Some(false),
                _ => None,
            };
            let element = match kind.as_str() {
                "Line" => {
                    let (s, e) = (self.child_point(item, "Start", &what), self.child_point(item, "End", &what));
                    s.zip(e).map(|(start, end)| LandXmlGeomElement::Line { start, end })
                }
                "Curve" => {
                    let (s, c, e) = (self.child_point(item, "Start", &what), self.child_point(item, "Center", &what), self.child_point(item, "End", &what));
                    match (s, c, e, clockwise) {
                        (Some(start), Some(center), Some(end), Some(clockwise)) => Some(LandXmlGeomElement::Curve {
                            start,
                            center,
                            end,
                            pi: self.opt_point(item, "PI", &what),
                            clockwise,
                            radius_m: number("radius", self.scale),
                            length_m: number("length", self.scale),
                        }),
                        _ => {
                            self.warnings.push(format!("{what} needs Start, Center, End and rot (cw or ccw); it was left out."));
                            None
                        }
                    }
                }
                "Spiral" => {
                    let (s, e) = (self.child_point(item, "Start", &what), self.child_point(item, "End", &what));
                    let radius = |name: &str| -> Option<Option<f64>> {
                        let t = item.attribute(name)?.trim();
                        if matches!(t.to_ascii_lowercase().trim_start_matches(['+', '-']), "inf" | "infinity") {
                            return Some(None);
                        }
                        let r: f64 = t.parse().ok()?;
                        Some((r != 0.0 && r.is_finite()).then_some(r.abs() * self.scale))
                    };
                    match (s, e, clockwise, number("length", self.scale), radius("radiusStart"), radius("radiusEnd")) {
                        (Some(start), Some(end), Some(clockwise), Some(length_m), Some(radius_start_m), Some(radius_end_m)) => Some(LandXmlGeomElement::Spiral {
                            start,
                            pi: self.opt_point(item, "PI", &what),
                            end,
                            spiral_type: item.attribute("spiType").unwrap_or("").to_string(),
                            clockwise,
                            length_m,
                            radius_start_m,
                            radius_end_m,
                        }),
                        _ => {
                            self.warnings.push(format!("{what} needs Start, End, rot, length, radiusStart and radiusEnd; it was left out."));
                            None
                        }
                    }
                }
                "IrregularLine" => {
                    let (s, e) = (self.child_point(item, "Start", &what), self.child_point(item, "End", &what));
                    let points = self.plan_list(item, &what);
                    s.zip(e).map(|(start, end)| LandXmlGeomElement::IrregularLine { start, end, points })
                }
                "Chain" => {
                    let names: Vec<String> = item.text().unwrap_or("").split_whitespace().map(str::to_string).collect();
                    let mut points = Vec::new();
                    for n in &names {
                        match self.points.get(n) {
                            Some(p) => points.push(*p),
                            None => self.warnings.push(format!("{what} names point '{n}', which is not in the file; it was skipped in the chain.")),
                        }
                    }
                    Some(LandXmlGeomElement::Chain { names, points })
                }
                _ => None, // Feature and anything else: counted by count_unknown
            };
            if let Some(e) = element {
                g.elements.push(e);
            }
        }
        g
    }

    fn parcel(&mut self, node: roxmltree::Node) -> LandXmlParcel {
        let area = node.attribute("area").and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
        let name = node.attribute("name").unwrap_or("").to_string();
        let mut parcel = LandXmlParcel {
            desc: node.attribute("desc").map(str::to_string),
            state: node.attribute("state").map(str::to_string),
            owner: node.attribute("owner").map(str::to_string),
            parcel_type: node.attribute("parcelType").map(str::to_string),
            class: node.attribute("class").map(str::to_string),
            area,
            area_m2: area.zip(self.area_scale).map(|(a, s)| a * s),
            ..Default::default()
        };
        if area.is_some() && self.area_scale.is_none() {
            self.warnings.push(format!("Parcel '{name}': its area is in an area unit the reader does not know, so area_m2 is not set."));
        }
        for c in node.children().filter(|n| n.is_element()) {
            match c.tag_name().name() {
                "Center" => {
                    parcel.center = self.node_point(c);
                    if parcel.center.is_none() {
                        self.warnings.push(format!("Parcel '{name}': the Center has no readable coordinates."));
                    }
                }
                "CoordGeom" => {
                    let g = self.coord_geom(c);
                    parcel.coord_geoms.push(g);
                }
                "Parcels" => {
                    for p in children(c, "Parcel") {
                        let p = self.parcel(p);
                        parcel.parcels.push(p);
                    }
                }
                _ => {}
            }
        }
        parcel.name = name;
        parcel
    }

    fn cross_section(&mut self, node: roxmltree::Node, alignment: &str, sta: f64) -> LandXmlCrossSection {
        let what = format!("CrossSect at {sta} of alignment '{alignment}'");
        let mut cs = LandXmlCrossSection {
            alignment: alignment.to_string(),
            station_m: sta * self.scale,
            name: node.attribute("name").map(str::to_string),
            desc: node.attribute("desc").map(str::to_string),
            ..Default::default()
        };
        for s in children(node, "CrossSectSurf") {
            cs.surfaces.push(LandXmlCrossSectSurf {
                name: s.attribute("name").unwrap_or("").to_string(),
                desc: s.attribute("desc").map(str::to_string),
                state: s.attribute("state").map(str::to_string),
                point_lists: self.pair_lists(s, &what),
            });
        }
        for d in children(node, "DesignCrossSectSurf") {
            let mut points = Vec::new();
            let mut dropped = 0;
            for p in children(d, "CrossSectPnt") {
                let format = p.attribute("dataFormat").unwrap_or("Offset Elevation");
                let v = if format == "Offset Elevation" { numbers(p.text().unwrap_or("")) } else { None };
                match v.as_deref() {
                    Some([o, e]) => points.push(LandXmlCrossSectPoint { offset_m: o * self.scale, elevation_m: e * self.scale }),
                    _ => dropped += 1,
                }
            }
            if dropped > 0 {
                self.warnings.push(format!("{what}: {dropped} CrossSectPnt(s) were left out (not 'offset elevation' text, or dataFormat other than 'Offset Elevation')."));
            }
            cs.design_surfaces.push(LandXmlDesignCrossSectSurf {
                name: d.attribute("name").map(str::to_string),
                desc: d.attribute("desc").map(str::to_string),
                state: d.attribute("state").map(str::to_string),
                side: d.attribute("side").map(str::to_string),
                material: d.attribute("material").map(str::to_string),
                closed_area: d.attribute("closedArea").and_then(|t| match t.trim() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => None,
                }),
                points,
            });
        }
        cs
    }

    fn surface(&mut self, node: roxmltree::Node, unnamed: &mut usize) -> LandXmlSurface {
        let name = match node.attribute("name") {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => {
                *unnamed += 1;
                format!("Surface {unnamed}")
            }
        };
        let mut surface = LandXmlSurface {
            name: name.clone(),
            desc: node.attribute("desc").map(str::to_string),
            state: node.attribute("state").map(str::to_string),
            ..Default::default()
        };
        let definitions: Vec<_> = children(node, "Definition").collect();
        if definitions.len() > 1 {
            self.warnings.push(format!("Surface '{name}' has {} Definitions; the first is read.", definitions.len()));
        }
        if let Some(def) = definitions.first() {
            surface.surf_type = def.attribute("surfType").map(str::to_string);
            let (mut bad_points, mut bad_faces) = (0usize, 0usize);
            for p in children(*def, "Pnts").flat_map(|g| children(g, "P")) {
                let id = p.attribute("id").and_then(|v| v.trim().parse::<u64>().ok());
                let xyz = self.node_point(p).and_then(|q| q.z.map(|z| (q.x, q.y, z)));
                match (id, xyz) {
                    (Some(id), Some((x, y, z))) => surface.points.push(LandXmlSurfPoint { id, easting_m: x, northing_m: y, elevation_m: z }),
                    _ => bad_points += 1,
                }
            }
            for f in children(*def, "Faces").flat_map(|g| children(g, "F")) {
                let ids: Option<Vec<u64>> = f.text().unwrap_or("").split_whitespace().map(|t| t.parse().ok()).collect();
                match ids {
                    Some(ids) if matches!(ids.len(), 3 | 4) => {
                        surface.faces.push(LandXmlSurfFace { ids, invisible: f.attribute("i").map(str::trim) == Some("1") })
                    }
                    _ => bad_faces += 1,
                }
            }
            if bad_points > 0 {
                self.warnings.push(format!("Surface '{name}': {bad_points} point(s) were left out because they have no id or not three coordinates (northing easting elevation)."));
            }
            if bad_faces > 0 {
                self.warnings.push(format!("Surface '{name}': {bad_faces} face(s) were left out because they do not hold three or four point ids."));
            }
        }
        for source in children(node, "SourceData") {
            let what = format!("Surface '{name}' SourceData");
            for b in children(source, "Breaklines").flat_map(|g| children(g, "Breakline")) {
                let points = self.plan_list(b, &what);
                surface.source_data.breaklines.push(LandXmlBreakline {
                    name: b.attribute("name").map(str::to_string),
                    desc: b.attribute("desc").map(str::to_string),
                    brk_type: b.attribute("brkType").map(str::to_string),
                    state: b.attribute("state").map(str::to_string),
                    points,
                });
            }
            for c in children(source, "Contours").flat_map(|g| children(g, "Contour")) {
                let Some(elev) = c.attribute("elev").and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite()) else {
                    self.warnings.push(format!("{what}: a Contour has no usable elev and was left out."));
                    continue;
                };
                let points = self.plan_list(c, &what);
                surface.source_data.contours.push(LandXmlContour { elevation_m: elev * self.scale, points });
            }
            for b in children(source, "Boundaries").flat_map(|g| children(g, "Boundary")) {
                let points = self.plan_list(b, &what);
                surface.source_data.boundaries.push(LandXmlBoundary {
                    name: b.attribute("name").map(str::to_string),
                    desc: b.attribute("desc").map(str::to_string),
                    bnd_type: b.attribute("bndType").unwrap_or("").to_string(),
                    edge_trim: matches!(b.attribute("edgeTrim").map(str::trim), Some("true" | "1")),
                    state: b.attribute("state").map(str::to_string),
                    area: b.attribute("area").and_then(|t| t.trim().parse().ok()),
                    points,
                });
            }
            for d in children(source, "DataPoints") {
                let points = self.plan_list(d, &what);
                surface.source_data.data_points.push(LandXmlDataPoints {
                    name: d.attribute("name").map(str::to_string),
                    desc: d.attribute("desc").map(str::to_string),
                    code: d.attribute("code").map(str::to_string),
                    state: d.attribute("state").map(str::to_string),
                    points,
                });
            }
        }
        surface
    }
}

/// The `SourceData` of one `Surface` element, read with the given linear scale. Used by the
/// corridor crate's surface reader so that both read it the same way.
pub fn read_surface_source_data(surface: roxmltree::Node, metres_per_unit: f64, warnings: &mut Vec<String>) -> LandXmlSourceData {
    let mut rd = Rd { scale: metres_per_unit, area_scale: None, points: HashMap::new(), warnings: Vec::new() };
    let mut unnamed = 0;
    let s = rd.surface(surface, &mut unnamed);
    warnings.extend(rd.warnings.into_iter().filter(|w| w.contains("SourceData")));
    s.source_data
}
