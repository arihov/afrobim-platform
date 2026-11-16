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

use crate::landxml::{children, import_from_root, Imported, LandXmlError, LocalName};
use crate::landxml_civil::{read_features_and_notes, read_superelevation_records, LandXmlAlignmentInfo, LandXmlGeometryFingerprint, LandXmlSuperelevationTable};
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
    /// `LandXML@readOnly`, as written.
    #[serde(default)]
    pub read_only: Option<String>,
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
    /// Cubic metres per `volume_unit`, when it is one the reader knows.
    #[serde(default)]
    pub cubic_metres_per_volume_unit: Option<f64>,
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
            cubic_metres_per_volume_unit: None,
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

/// Square metres per unit for a `metArea` / `impArea` name of the schema (`None` for a name the
/// schema does not list).
pub fn area_scale(unit: &str) -> Option<f64> {
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

/// Cubic metres per unit for a `metVolume` / `impVolume` name of the schema that is a plain volume
/// unit (`US_gallon`, `IMP_gallon` and `acreFeet` are not offered).
pub fn volume_scale(unit: &str) -> Option<f64> {
    Some(match unit {
        "cubicMeter" => 1.0,
        "liter" => 0.001,
        "hectareMeter" => 10_000.0,
        "cubicInch" => 0.0254_f64.powi(3),
        "cubicFeet" => 0.3048_f64.powi(3),
        "cubicYard" => 0.9144_f64.powi(3),
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
        cubic_metres_per_volume_unit: attr("volumeUnit").as_deref().and_then(volume_scale),
    };
    if units.volume_unit.is_some() && units.cubic_metres_per_volume_unit.is_none() {
        warnings.push(format!("The volume unit '{}' is not one the reader converts; volumes (cross-section cut and fill) are reported as stated, without conversion.", units.volume_unit.as_deref().unwrap_or("")));
    }
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

/// `SourceData/PointFiles/PointFile`: an external file the surface was built from. The names are
/// as written (a path on the exporting machine); the file is not opened.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlPointFile {
    pub file_name: String,
    pub file_type: String,
    pub file_format: String,
}

/// `Surface/SourceData`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSourceData {
    pub breaklines: Vec<LandXmlBreakline>,
    pub contours: Vec<LandXmlContour>,
    pub boundaries: Vec<LandXmlBoundary>,
    pub data_points: Vec<LandXmlDataPoints>,
    /// The external point files the surface was built from.
    #[serde(default)]
    pub point_files: Vec<LandXmlPointFile>,
}

impl LandXmlSourceData {
    pub fn is_empty(&self) -> bool {
        self.breaklines.is_empty() && self.contours.is_empty() && self.boundaries.is_empty() && self.data_points.is_empty() && self.point_files.is_empty()
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

/// What `Definition` states about a surface (`area2DSurf`, `area3DSurf`, `elevMin`, `elevMax`), as
/// written: areas in square metres (the file's area unit applied), levels in the linear unit.
/// Civil 3D computes them from the visible faces, so they can be checked against the points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSurfaceStats {
    pub area_2d_m2: Option<f64>,
    pub area_3d_m2: Option<f64>,
    pub elev_min_m: Option<f64>,
    pub elev_max_m: Option<f64>,
}

/// `Surfaces/Surface`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSurface {
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    /// `Surface@OID`.
    #[serde(default)]
    pub oid: Option<String>,
    /// The statistics `Definition` states, when it states any.
    #[serde(default)]
    pub stated: Option<LandXmlSurfaceStats>,
    /// `Definition@surfType`: `TIN` or `grid`; `None` when the surface has no `Definition`.
    pub surf_type: Option<String>,
    pub points: Vec<LandXmlSurfPoint>,
    pub faces: Vec<LandXmlSurfFace>,
    pub source_data: LandXmlSourceData,
}

/// A point is called an outlier when it lies this many times the median distance of the surface's
/// points from the middle of the surface (the median easting and northing). A diagnostic only: no
/// point is ever dropped or moved.
pub const OUTLIER_FACTOR: f64 = 100.0;

/// Points far from the rest, see [`plan_outliers`].
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOutliers {
    /// The median easting and northing.
    pub middle: (f64, f64),
    /// The median distance of the points from `middle`.
    pub median_distance_m: f64,
    /// Indices (in input order) of the points more than [`OUTLIER_FACTOR`] times that distance from `middle`.
    pub indices: Vec<usize>,
}

/// Finds the points of a plan point set that lie [`OUTLIER_FACTOR`] times the median distance from
/// its middle (the median easting and northing) or farther. `None` for no points. A diagnostic: the
/// caller keeps every point as it is.
pub fn plan_outliers(points: impl Iterator<Item = (f64, f64)> + Clone) -> Option<PlanOutliers> {
    fn median(mut v: Vec<f64>) -> f64 {
        let mid = v.len() / 2;
        *v.select_nth_unstable_by(mid, f64::total_cmp).1
    }
    let n = points.clone().count();
    if n == 0 {
        return None;
    }
    let middle = (median(points.clone().map(|p| p.0).collect()), median(points.clone().map(|p| p.1).collect()));
    let dist: Vec<f64> = points.map(|p| (p.0 - middle.0).hypot(p.1 - middle.1)).collect();
    let median_distance_m = median(dist.clone());
    let indices = if median_distance_m > 0.0 { dist.iter().enumerate().filter(|(_, d)| **d > OUTLIER_FACTOR * median_distance_m).map(|(i, _)| i).collect() } else { Vec::new() };
    Some(PlanOutliers { middle, median_distance_m, indices })
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

    /// Distance in plan of point `index` from the middle of the surface (the median easting and northing).
    pub fn distance_from_middle(&self, index: usize) -> f64 {
        match (self.points.get(index), plan_outliers(self.points.iter().map(|p| (p.easting_m, p.northing_m)))) {
            (Some(p), Some(o)) => (p.easting_m - o.middle.0).hypot(p.northing_m - o.middle.1),
            _ => 0.0,
        }
    }

    /// The median distance of the points from the middle of the surface.
    pub fn median_distance_m(&self) -> Option<f64> {
        plan_outliers(self.points.iter().map(|p| (p.easting_m, p.northing_m))).map(|o| o.median_distance_m)
    }

    /// Indices (into [`LandXmlSurface::points`]) of the points that lie [`OUTLIER_FACTOR`] times the
    /// median distance from the middle of the surface or farther. Survey data with a stray or
    /// untransformed point shows up here; the point is still in the surface as the file gives it.
    pub fn outlier_points(&self) -> Vec<usize> {
        plan_outliers(self.points.iter().map(|p| (p.easting_m, p.northing_m))).map(|o| o.indices).unwrap_or_default()
    }

    /// The plan extent of the points that are not [`outlier_points`](Self::outlier_points).
    pub fn core_extent(&self) -> Option<LandXmlExtent> {
        let far: std::collections::HashSet<usize> = self.outlier_points().into_iter().collect();
        let mut ext = None;
        for (i, p) in self.points.iter().enumerate() {
            if !far.contains(&i) {
                extend(&mut ext, p.easting_m, p.northing_m);
            }
        }
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
    /// The extent of the surface points that are not outliers ([`LandXmlSurface::outlier_points`]);
    /// the same as `surfaces.extent` when there are none.
    #[serde(default)]
    pub surfaces_core_extent: Option<LandXmlExtent>,
    pub ground_profiles: usize,
    pub cross_sections: usize,
    /// Further counts by name: `cg_point_groups`, `surface_points`, `surface_faces`, `breaklines`,
    /// `contours`, `boundaries`, `data_point_sets`, `irregular_lines`, `chains`, `station_equations`.
    pub counts: BTreeMap<String, usize>,
    /// Elements that were not read, by path, with how many there were.
    pub skipped: BTreeMap<String, usize>,
    /// Attributes that were not read, as `path@attribute`, with how many there were. They are the
    /// redundant ones (directions, angles, tangent lengths and the like, which the points say) and
    /// the neighbour lists of faces; nothing in them is needed to rebuild the geometry.
    #[serde(default)]
    pub ignored_attributes: BTreeMap<String, usize>,
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
    /// The attributes and the stated geometry of each `Alignment`, imported or not (file order).
    #[serde(default)]
    pub alignment_infos: Vec<crate::landxml_civil::LandXmlAlignmentInfo>,
    /// Every `ProfAlign` as written, also those the engine [`crate::Profile`] cannot hold.
    #[serde(default)]
    pub design_profiles: Vec<crate::landxml_civil::LandXmlDesignProfile>,
    /// Every `Superelevation` element as written, per alignment that has any.
    #[serde(default)]
    pub superelevation_tables: Vec<crate::landxml_civil::LandXmlSuperelevationTable>,
    /// The attributes of each `CrossSects` element.
    #[serde(default)]
    pub cross_sect_groups: Vec<crate::landxml_civil::LandXmlCrossSectGroup>,
    /// The attributes of the `CrossSect`s that have any beyond `sta`, `name` and `desc`.
    #[serde(default)]
    pub cross_sect_details: Vec<crate::landxml_civil::LandXmlCrossSectDetail>,
    /// Every `Feature` element (a generic property bag) with the object it hangs on.
    #[serde(default)]
    pub object_features: Vec<crate::landxml_civil::LandXmlObjectFeatures>,
    /// Every `FieldNote` with the object it hangs on.
    #[serde(default)]
    pub field_notes: Vec<crate::landxml_civil::LandXmlFieldNote>,
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
    let bytes = std::fs::read(path)?;
    let (text, note) = decode_text(&bytes);
    let mut file = parse_landxml_file(&text)?;
    if let Some(note) = note {
        file.warnings.insert(0, note);
    }
    Ok(file)
}

/// The text of a file's bytes. A UTF-8 byte order mark is dropped; UTF-16 (with its mark) is
/// decoded; bytes that are not valid UTF-8 are read one byte to one character (ISO 8859-1) and a
/// note says so. XML is parsed from text, so a declared `encoding` is not consulted.
pub(crate) fn decode_text(bytes: &[u8]) -> (std::borrow::Cow<'_, str>, Option<String>) {
    use std::borrow::Cow;
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return match std::str::from_utf8(rest) {
            Ok(t) => (Cow::Borrowed(t), None),
            Err(_) => (Cow::Owned(rest.iter().map(|&b| b as char).collect()), Some("The file has a UTF-8 byte order mark but is not valid UTF-8; it was read as ISO 8859-1 (one byte, one character).".to_string())),
        };
    }
    for (mark, big_endian) in [([0xFF, 0xFE], false), ([0xFE, 0xFF], true)] {
        if let Some(rest) = bytes.strip_prefix(&mark) {
            let units: Vec<u16> = rest.chunks_exact(2).map(|c| if big_endian { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
            return match String::from_utf16(&units) {
                Ok(t) => (Cow::Owned(t), Some("The file is UTF-16; it was decoded.".to_string())),
                Err(_) => (Cow::Owned(String::from_utf16_lossy(&units)), Some("The file is UTF-16 with invalid sequences; they were replaced by U+FFFD.".to_string())),
            };
        }
    }
    match std::str::from_utf8(bytes) {
        Ok(t) => (Cow::Borrowed(t), None),
        Err(e) => (
            Cow::Owned(bytes.iter().map(|&b| b as char).collect()),
            Some(format!("The file is not valid UTF-8 (first bad byte at offset {}); it was read as ISO 8859-1 (one byte, one character).", e.valid_up_to())),
        ),
    }
}

/// Reads a whole LandXML document held in a string. Errors only for what is not LandXML at all
/// (not XML, wrong root, a linear unit outside the schema's list); everything else is read as far as
/// it can be and the rest is reported in [`LandXmlFile::warnings`].
pub fn parse_landxml_file(text: &str) -> Result<LandXmlFile, LandXmlError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| LandXmlError::Xml(e.to_string()))?;
    let root = doc.root_element();
    // Alignments first: this checks the root and the linear unit too, and returns its own notes.
    let Imported { import, nodes } = import_from_root(root)?;
    let mut warnings: Vec<String> = Vec::new();
    let units = read_units(root, &mut warnings)?;
    // `import_from_root` has already said "no linear unit" and "no alignments"; do not repeat the first.
    warnings.retain(|w| !import.notes.contains(w));
    let mut all_warnings = import.notes.clone();
    all_warnings.extend(warnings);

    let header = LandXmlHeader {
        version: root.attribute("version").map(str::to_string),
        date: root.attribute("date").map(str::to_string),
        time: root.attribute("time").map(str::to_string),
        language: root.attribute("language").map(str::to_string),
        namespace: root.tag_name().namespace().map(str::to_string),
        read_only: root.attribute("readOnly").map(str::to_string),
    };

    let mut rd = Rd {
        scale: units.metres_per_linear_unit,
        area_scale: units.square_metres_per_area_unit,
        volume_scale: units.cubic_metres_per_volume_unit,
        points: HashMap::new(),
        warnings: Vec::new(),
    };
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
    let mut alignment_infos = Vec::new();
    let mut design_profiles = Vec::new();
    let mut superelevation_tables = Vec::new();
    let mut cross_sect_groups = Vec::new();
    let mut cross_sect_details = Vec::new();
    let mut alignment_names: HashMap<roxmltree::NodeId, String> = HashMap::new();
    for an in &nodes {
        let (node, name) = (an.node, &an.name);
        alignment_names.insert(node.id(), name.clone());
        let engine = an.index.map(|i| &import.alignments[i]);
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
        design_profiles.extend(rd.design_profiles(node, name));
        for cs in children(node, "CrossSects").flat_map(|g| children(g, "CrossSect")) {
            let Some(sta) = cs.attribute("sta").and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite()) else {
                rd.warnings.push(format!("A CrossSect of alignment '{name}' has no usable sta and was left out."));
                continue;
            };
            cross_sections.push(rd.cross_section(cs, name, sta));
        }
        rd.cross_sect_attributes(node, name, &mut cross_sect_groups, &mut cross_sect_details);

        let records = read_superelevation_records(node, rd.scale, engine, name, &mut rd.warnings);
        if !records.is_empty() {
            let unsure = records.iter().filter(|r| r.full_super_rates_pct.len() > 1).count();
            if unsure > 0 {
                rd.warnings.push(format!("Alignment '{name}': {unsure} Superelevation element(s) have more than one FullSuperelev; all are kept in the table, and none is applied as cross-fall."));
            }
            superelevation_tables.push(LandXmlSuperelevationTable { alignment: name.clone(), records, derived: engine.and_then(|a| a.superelevation.clone()) });
        }

        let coord_geoms: Vec<LandXmlCoordGeom> = children(node, "CoordGeom").map(|g| rd.coord_geom(g)).collect();
        let number = |a: &str| node.attribute(a).and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v * rd.scale);
        alignment_infos.push(LandXmlAlignmentInfo {
            name: name.clone(),
            desc: node.attribute("desc").map(str::to_string),
            state: node.attribute("state").map(str::to_string),
            oid: node.attribute("oID").map(str::to_string),
            declared_length_m: number("length"),
            sta_start_m: number("staStart"),
            imported: engine.is_some(),
            coord_geoms,
            fingerprint: engine.and_then(LandXmlGeometryFingerprint::of),
        });
    }
    let (object_features, field_notes) = read_features_and_notes(root, rd.scale, &alignment_names);

    let mut surfaces = Vec::new();
    let mut unnamed_surfaces = 0;
    for s in children(root, "Surfaces").flat_map(|g| children(g, "Surface")) {
        surfaces.push(rd.surface(s, &mut unnamed_surfaces));
    }
    let mut outliers = 0;
    for s in &surfaces {
        let far = s.outlier_points();
        if !far.is_empty() {
            outliers += far.len();
            let core = s.core_extent();
            rd.warnings.push(format!(
                "Surface '{}': {} of {} points lie more than {OUTLIER_FACTOR:.0} times the median distance ({:.1} m) from the middle of the surface, as far as {:.1} m; they are kept as written, but the plan extent of the surface is dominated by them{}.",
                s.name,
                far.len(),
                s.points.len(),
                s.median_distance_m().unwrap_or(0.0),
                far.iter().map(|&i| s.distance_from_middle(i)).fold(0.0, f64::max),
                core.map_or(String::new(), |e| format!(" (without them: easting {} to {}, northing {} to {})", e.min_easting, e.max_easting, e.min_northing, e.max_northing))
            ));
        }
    }

    all_warnings.extend(rd.warnings);
    let mut skipped = BTreeMap::new();
    let mut ignored = BTreeMap::new();
    walk_unread(root, &mut vec!["LandXML"], &mut skipped, &mut ignored);
    for (path, n) in &skipped {
        all_warnings.push(format!("{n} element{} at {path} {} not read.", if *n == 1 { "" } else { "s" }, if *n == 1 { "was" } else { "were" }));
    }
    if !ignored.is_empty() {
        let total: usize = ignored.values().sum();
        all_warnings.push(format!(
            "{total} attribute value(s) of {} kind(s) were not read, because the points and elements say the same or they can be derived (see summary.ignored_attributes): {}.",
            ignored.len(),
            ignored.iter().map(|(k, v)| format!("{} x{v}", k.trim_start_matches("LandXML/"))).collect::<Vec<_>>().join(", ")
        ));
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
        alignment_infos,
        design_profiles,
        superelevation_tables,
        cross_sect_groups,
        cross_sect_details,
        object_features,
        field_notes,
        warnings: all_warnings,
        summary: LandXmlSummary::default(),
    };
    file.summary = summarize(&file, skipped);
    file.summary.ignored_attributes = ignored;
    if outliers > 0 {
        file.summary.counts.insert("surface_outlier_points".into(), outliers);
    }
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
        extend_with(&mut s.surfaces_core_extent, surf.core_extent());
        let mut own = surf.source_data.extent();
        extend_with(&mut s.surfaces_core_extent, own.take());
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
    c("design_profiles", f.design_profiles.len());
    c("superelevation_records", f.superelevation_tables.iter().map(|t| t.records.len()).sum());
    c("cross_sect_groups", f.cross_sect_groups.len());
    c("features", f.object_features.iter().map(|o| o.features.len()).sum());
    c("field_notes", f.field_notes.len());
    c("point_files", f.surfaces.iter().map(|x| x.source_data.point_files.len()).sum());
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
/// `Feature` and `FieldNote` are read wherever they stand and are not listed.
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
        "SourceData" => &["Breaklines", "Contours", "Boundaries", "DataPoints", "PointFiles"],
        "PointFiles" => &["PointFile"],
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

/// The attributes the reader reads or compares, per element. Any other attribute (except those in a
/// namespace, such as `xsi:schemaLocation`) is counted in [`LandXmlSummary::ignored_attributes`].
fn read_attributes(element: &str) -> Option<&'static [&'static str]> {
    Some(match element {
        "LandXML" => &["version", "date", "time", "language", "readOnly"],
        "Metric" | "Imperial" => &[
            "areaUnit", "linearUnit", "volumeUnit", "temperatureUnit", "pressureUnit", "diameterUnit", "widthUnit", "heightUnit", "velocityUnit", "flowUnit", "angularUnit", "directionUnit",
            "latLongAngularUnit", "elevationUnit",
        ],
        "CoordinateSystem" => &[
            "name", "desc", "epsgCode", "ogcWktCode", "horizontalDatum", "verticalDatum", "ellipsoidName", "horizontalCoordinateSystemName", "geocentricCoordinateSystemName", "fileLocation",
            "rotationAngle", "datum", "fittedCoordinateSystemName", "compoundCoordinateSystemName", "localCoordinateSystemName", "geographicCoordinateSystemName", "projectedCoordinateSystemName",
            "verticalCoordinateSystemName",
        ],
        "Project" => &["name", "desc", "state"],
        "Application" => &["name", "desc", "manufacturer", "version", "manufacturerURL", "timeStamp"],
        "Alignment" => &["name", "length", "staStart", "desc", "state", "oID"],
        "CoordGeom" => &["name", "desc", "state"],
        "StaEquation" => &["staInternal", "staAhead", "staBack", "staIncrement"],
        "Line" => &["length"],
        "Curve" => &["rot", "length", "radius"],
        "Spiral" => &["rot", "length", "radiusStart", "radiusEnd", "spiType"],
        "Start" | "End" | "Center" | "PI" => &["pntRef"],
        "Profile" => &["name", "desc", "state", "staStart"],
        "ProfAlign" | "ProfSurf" => &["name", "desc", "state"],
        "PVI" => &["desc"],
        "ParaCurve" => &["length", "desc"],
        "UnsymParaCurve" => &["lengthIn", "lengthOut", "desc"],
        "CircCurve" => &["length", "radius", "desc"],
        "CrossSects" => &["name", "desc", "state", "calcMethod", "curveCorrection", "swellFactor", "shrinkFactor"],
        "CrossSect" => &["sta", "name", "desc", "angleSkew", "areaCut", "areaFill", "centroidCut", "centroidFill", "sectType", "volumeCut", "volumeFill"],
        "CrossSectSurf" => &["name", "desc", "state"],
        "DesignCrossSectSurf" => &["name", "desc", "state", "side", "material", "closedArea"],
        "CrossSectPnt" => &["dataFormat"],
        "Superelevation" => &["staStart", "staEnd"],
        "CgPoints" => &["name", "desc", "code", "state", "zoneNumber", "DTMAttribute"],
        "CgPoint" => &["name", "code", "desc", "state", "oID", "pointGeometry", "DTMAttribute", "latitude", "longitude", "pntRef"],
        "Parcel" => &["name", "desc", "state", "owner", "parcelType", "class", "area"],
        "PlanFeatures" => &["name"],
        "PlanFeature" => &["name", "desc", "state"],
        "Surface" => &["name", "desc", "state", "OID"],
        "Definition" => &["surfType", "area2DSurf", "area3DSurf", "elevMin", "elevMax"],
        "P" => &["id", "pntRef"],
        "F" => &["i"],
        "Breakline" => &["name", "desc", "brkType", "state"],
        "Contour" => &["elev"],
        "Boundary" => &["name", "desc", "bndType", "edgeTrim", "state", "area"],
        "DataPoints" => &["name", "desc", "code", "state"],
        "PointFile" => &["fileName", "fileType", "fileFormat"],
        _ => return None,
    })
}

/// Walks the elements the reader knows and counts what it does not read: elements by path, and
/// attributes as `path@name`. `Feature` and `FieldNote` are read wherever they stand.
fn walk_unread<'a>(node: roxmltree::Node<'a, '_>, path: &mut Vec<&'a str>, skipped: &mut BTreeMap<String, usize>, ignored: &mut BTreeMap<String, usize>) {
    let element = node.tag_name().name();
    if let Some(read) = read_attributes(element) {
        for a in node.attributes().filter(|a| a.namespace().is_none() && !read.contains(&a.name())) {
            *ignored.entry(format!("{}@{}", path.join("/"), a.name())).or_insert(0) += 1;
        }
    }
    let Some(known) = known_children(element) else { return };
    for c in node.children().filter(|n| n.is_element()) {
        let name = c.tag_name().name();
        if matches!(name, "Feature" | "FieldNote") {
            continue;
        }
        path.push(name);
        if known.contains(&name) {
            walk_unread(c, path, skipped, ignored);
        } else {
            *skipped.entry(path.join("/")).or_insert(0) += 1;
        }
        path.pop();
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

pub(crate) struct Rd {
    pub(crate) scale: f64,
    pub(crate) area_scale: Option<f64>,
    pub(crate) volume_scale: Option<f64>,
    pub(crate) points: HashMap<String, LandXmlPoint>,
    pub(crate) warnings: Vec<String>,
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

    /// The `SourceData` of a `Surface` element: breaklines, contours, boundaries, data points and
    /// the external point files.
    pub(crate) fn source_data(&mut self, node: roxmltree::Node, name: &str) -> LandXmlSourceData {
        let mut out = LandXmlSourceData::default();
        for source in children(node, "SourceData") {
            let what = format!("Surface '{name}' SourceData");
            for b in children(source, "Breaklines").flat_map(|g| children(g, "Breakline")) {
                let points = self.plan_list(b, &what);
                out.breaklines.push(LandXmlBreakline {
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
                out.contours.push(LandXmlContour { elevation_m: elev * self.scale, points });
            }
            for b in children(source, "Boundaries").flat_map(|g| children(g, "Boundary")) {
                let points = self.plan_list(b, &what);
                out.boundaries.push(LandXmlBoundary {
                    name: b.attribute("name").map(str::to_string),
                    desc: b.attribute("desc").map(str::to_string),
                    bnd_type: b.attribute("bndType").unwrap_or("").to_string(),
                    edge_trim: matches!(b.attribute("edgeTrim").map(str::trim), Some("true" | "1")),
                    state: b.attribute("state").map(str::to_string),
                    area: b.attribute("area").and_then(|t| t.trim().parse().ok()),
                    points,
                });
            }
            for pf in children(source, "PointFiles").flat_map(|g| children(g, "PointFile")) {
                out.point_files.push(LandXmlPointFile {
                    file_name: pf.attribute("fileName").unwrap_or("").to_string(),
                    file_type: pf.attribute("fileType").unwrap_or("").to_string(),
                    file_format: pf.attribute("fileFormat").unwrap_or("").to_string(),
                });
            }
            for d in children(source, "DataPoints") {
                let points = self.plan_list(d, &what);
                out.data_points.push(LandXmlDataPoints {
                    name: d.attribute("name").map(str::to_string),
                    desc: d.attribute("desc").map(str::to_string),
                    code: d.attribute("code").map(str::to_string),
                    state: d.attribute("state").map(str::to_string),
                    points,
                });
            }
        }
        out
    }

    /// What `Definition` states: areas (area unit applied) and levels.
    pub(crate) fn stated_stats(&self, def: roxmltree::Node) -> Option<LandXmlSurfaceStats> {
        let number = |a: &str| def.attribute(a).and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
        let stats = LandXmlSurfaceStats {
            area_2d_m2: number("area2DSurf").zip(self.area_scale).map(|(a, k)| a * k),
            area_3d_m2: number("area3DSurf").zip(self.area_scale).map(|(a, k)| a * k),
            elev_min_m: number("elevMin").map(|v| v * self.scale),
            elev_max_m: number("elevMax").map(|v| v * self.scale),
        };
        (stats != LandXmlSurfaceStats::default()).then_some(stats)
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
            oid: node.attribute("OID").map(str::to_string),
            ..Default::default()
        };
        let definitions: Vec<_> = children(node, "Definition").collect();
        if definitions.len() > 1 {
            self.warnings.push(format!("Surface '{name}' has {} Definitions; the first is read.", definitions.len()));
        }
        if let Some(def) = definitions.first() {
            surface.surf_type = def.attribute("surfType").map(str::to_string);
            surface.stated = self.stated_stats(*def);
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
        surface.source_data = self.source_data(node, &name);
        surface
    }
}

/// The `SourceData` of one `Surface` element, read with the given linear scale. Used by the
/// corridor crate's surface reader so that both read it the same way. Only the source data is read,
/// not the points and faces.
pub fn read_surface_source_data(surface: roxmltree::Node, metres_per_unit: f64, warnings: &mut Vec<String>) -> LandXmlSourceData {
    let mut rd = Rd { scale: metres_per_unit, area_scale: None, volume_scale: None, points: HashMap::new(), warnings: Vec::new() };
    let name = surface.attribute("name").unwrap_or("");
    let data = rd.source_data(surface, name);
    warnings.extend(rd.warnings);
    data
}

/// What the `Definition` of a `Surface` element states about the surface (`area2DSurf`, `area3DSurf`,
/// `elevMin`, `elevMax`): levels by the linear scale, areas by `square_metres_per_unit` (pass `None`
/// when the area unit is not known; the areas are then left out).
pub fn read_surface_stats(surface: roxmltree::Node, metres_per_unit: f64, square_metres_per_unit: Option<f64>) -> Option<LandXmlSurfaceStats> {
    let rd = Rd { scale: metres_per_unit, area_scale: square_metres_per_unit, volume_scale: None, points: HashMap::new(), warnings: Vec::new() };
    children(surface, "Definition").next().and_then(|d| rd.stated_stats(d))
}
