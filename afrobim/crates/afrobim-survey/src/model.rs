//! The survey database: settings, equipment, networks (points, directions, setups with
//! observations, levelling runs, baselines, figures), import events and a change log. The layout is
//! modelled on the objects of the Autodesk Civil 3D survey database as described in its help
//! (Survey Database Settings, Equipment Database Manager, networks, setups, import events, figure
//! prefixes), without copying any file format of that product.
//!
//! Everything is stored in metres, decimal degrees and zenith angles. No tolerance, standard error,
//! refraction coefficient or earth radius has a built-in value: they are inputs recorded here.

use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};
use crate::figures::{Figure, FigurePrefix, LineworkCodes};
use crate::units::{AngleUnit, HzDirection, VerticalKind};

pub const FORMAT_TAG: &str = "afrobim-survey-db";
pub const FORMAT_VERSION: u32 = 1;

/// Curvature and refraction correction parameters. Both numbers are inputs. The refraction coefficient
/// is k = R / r, the ratio of the earth radius to the radius of curvature of the light path
/// (Baselga, Garcia-Asenjo and Garrigues 2014, J. Surv. Eng. 140(2), eq. 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvatureRefraction {
    pub earth_radius_m: f64,
    pub refraction_coefficient: f64,
}

/// Reduction of horizontal distances to sea level (or the ellipsoid). The radius is an input.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeaLevel {
    pub earth_radius_m: f64,
}

/// Each correction is `None` (off) or `Some(parameters)` (on). The order of application is fixed and
/// stated in `reduce.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Corrections {
    pub prism_constant_m: Option<f64>,
    /// Atmospheric correction in parts per million, computed by the caller from temperature and
    /// pressure with the formula of the instrument maker. Applied as SD (1 + ppm 1e-6).
    pub atmospheric_ppm: Option<f64>,
    pub curvature_refraction: Option<CurvatureRefraction>,
    pub sea_level: Option<SeaLevel>,
    /// Grid scale factor (combined or line scale factor as supplied), horizontal distance multiplied by it.
    pub grid_scale_factor: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Label only; lengths are stored in metres.
    pub distance_unit: String,
    /// Unit of angles in incoming raw files unless the file says otherwise.
    pub angle_unit: AngleUnit,
    pub hz_direction: HzDirection,
    pub vertical_kind: VerticalKind,
    /// Free text coordinate system label, for example "UTM zone 36S, WGS 84".
    pub coordinate_system: String,
    pub epsg: Option<u32>,
    pub corrections: Corrections,
}

impl Settings {
    /// Metres, decimal degrees, angles to the right, zenith angles, no corrections.
    pub fn metric() -> Settings {
        Settings {
            distance_unit: "m".into(),
            angle_unit: AngleUnit::Degrees,
            hz_direction: HzDirection::Right,
            vertical_kind: VerticalKind::Zenith,
            coordinate_system: String::new(),
            epsg: None,
            corrections: Corrections::default(),
        }
    }
}

/// Instrument and target records. Every value is a standard error you state (from a maker's data
/// sheet or a calibration); none is supplied. Angles in arc seconds, lengths in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Equipment {
    pub name: String,
    /// EDM constant part a of sigma_D^2 = a^2 + (b D)^2 (metres).
    pub sigma_distance_const_m: f64,
    /// EDM proportional part b in parts per million.
    pub sigma_distance_ppm: f64,
    /// Pointing error of one sighting, arc seconds.
    pub sigma_pointing_arcsec: f64,
    /// Circle reading error of one reading, arc seconds.
    pub sigma_reading_arcsec: f64,
    /// Standard error of one zenith angle (pointing and reading together), arc seconds.
    pub sigma_zenith_arcsec: f64,
    /// Centring error of the instrument over the station: standard deviation in each horizontal
    /// coordinate, metres.
    pub centring_instrument_m: f64,
    /// Centring error of the target over the point, metres, same meaning.
    pub centring_target_m: f64,
    pub sigma_instrument_height_m: f64,
    pub sigma_target_height_m: f64,
    /// Levelling: constant part and per square root of kilometre part of sigma of one height difference.
    pub level_sigma_const_m: f64,
    pub level_sigma_per_sqrt_km_m: f64,
    /// Horizontal collimation of the instrument in degrees if known (applied to face 1 / face 2
    /// readings of single faced observations when `apply_collimation` is on in the reduction options).
    pub horizontal_collimation_deg: Option<f64>,
    /// Vertical index error in degrees if known.
    pub vertical_index_error_deg: Option<f64>,
}

impl Equipment {
    /// All standard errors zero: only useful for noise free simulations. Real values must be set.
    pub fn exact(name: &str) -> Equipment {
        Equipment {
            name: name.into(),
            sigma_distance_const_m: 0.0,
            sigma_distance_ppm: 0.0,
            sigma_pointing_arcsec: 0.0,
            sigma_reading_arcsec: 0.0,
            sigma_zenith_arcsec: 0.0,
            centring_instrument_m: 0.0,
            centring_target_m: 0.0,
            sigma_instrument_height_m: 0.0,
            sigma_target_height_m: 0.0,
            level_sigma_const_m: 0.0,
            level_sigma_per_sqrt_km_m: 0.0,
            horizontal_collimation_deg: None,
            vertical_index_error_deg: None,
        }
    }
}

/// A point of a network. A control point has components that are fixed (held exactly) or free; free
/// components with a stated sigma are weighted constraints (for example GNSS coordinates with their
/// standard errors), free components without sigma are only approximate values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurveyPoint {
    pub name: String,
    pub e: Option<f64>,
    pub n: Option<f64>,
    pub z: Option<f64>,
    pub control: bool,
    pub fix_e: bool,
    pub fix_n: bool,
    pub fix_z: bool,
    pub sigma_e: Option<f64>,
    pub sigma_n: Option<f64>,
    pub sigma_z: Option<f64>,
    pub description: String,
    /// True when e, n, z were computed by radiation or adjustment and not given.
    pub computed: bool,
    pub event: Option<u64>,
}

impl SurveyPoint {
    /// Control point with all of E, N, Z fixed (`z` may be `None`, then Z is not fixed).
    pub fn control(name: &str, e: f64, n: f64, z: Option<f64>) -> SurveyPoint {
        SurveyPoint {
            name: name.into(),
            e: Some(e),
            n: Some(n),
            z,
            control: true,
            fix_e: true,
            fix_n: true,
            fix_z: z.is_some(),
            sigma_e: None,
            sigma_n: None,
            sigma_z: None,
            description: String::new(),
            computed: false,
            event: None,
        }
    }
    /// A non control point with given coordinates (they are approximate values for an adjustment).
    pub fn given(name: &str, e: f64, n: f64, z: Option<f64>) -> SurveyPoint {
        let mut p = SurveyPoint::control(name, e, n, z);
        p.control = false;
        p.fix_e = false;
        p.fix_n = false;
        p.fix_z = false;
        p
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Face {
    Single,
    Face1,
    Face2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HzKind {
    /// Horizontal angle turned from the backsight (right or left per the settings).
    Angle,
    /// Horizontal circle reading (a direction).
    Direction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub id: u64,
    pub target: String,
    pub face: Face,
    pub hz_kind: HzKind,
    pub hz_deg: Option<f64>,
    /// Zenith angle in degrees.
    pub zenith_deg: Option<f64>,
    pub slope_distance: Option<f64>,
    pub horizontal_distance: Option<f64>,
    /// Height difference from the instrument axis to the target point (Civil 3D "vertical distance").
    pub vertical_distance: Option<f64>,
    pub target_height: f64,
    pub description: String,
    pub disabled: bool,
    pub source_line: Option<usize>,
}

impl Observation {
    pub fn new(target: &str) -> Observation {
        Observation {
            id: 0,
            target: target.into(),
            face: Face::Single,
            hz_kind: HzKind::Direction,
            hz_deg: None,
            zenith_deg: None,
            slope_distance: None,
            horizontal_distance: None,
            vertical_distance: None,
            target_height: 0.0,
            description: String::new(),
            disabled: false,
            source_line: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub id: u64,
    pub station: String,
    pub instrument_height: f64,
    pub backsight_point: Option<String>,
    /// Azimuth to the backsight when it is not a point with coordinates (degrees).
    pub backsight_azimuth_deg: Option<f64>,
    /// Circle reading on the backsight (degrees). For observations of kind `Angle` this is not used.
    pub backsight_reading_deg: f64,
    pub equipment: Option<String>,
    pub observations: Vec<Observation>,
    pub event: Option<u64>,
}

impl Setup {
    pub fn new(station: &str, instrument_height: f64) -> Setup {
        Setup {
            id: 0,
            station: station.into(),
            instrument_height,
            backsight_point: None,
            backsight_azimuth_deg: None,
            backsight_reading_deg: 0.0,
            equipment: None,
            observations: Vec::new(),
            event: None,
        }
    }
}

/// A known azimuth from one point to another (the AZ command of the Civil 3D survey command language).
/// With `sigma_arcsec` it is also an observation in the adjustment; without it it only orients setups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownDirection {
    pub from: String,
    pub to: String,
    pub azimuth_deg: f64,
    pub sigma_arcsec: Option<f64>,
    pub event: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelLeg {
    pub from: String,
    pub to: String,
    /// Height difference to minus from, metres.
    pub dh: f64,
    pub length_m: f64,
}

impl LevelLeg {
    /// From a backsight and a foresight staff reading: dh = backsight - foresight (height of the
    /// foresight point minus height of the backsight point).
    pub fn from_readings(from: &str, to: &str, backsight: f64, foresight: f64, length_m: f64) -> LevelLeg {
        LevelLeg { from: from.into(), to: to.into(), dh: backsight - foresight, length_m }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelRun {
    pub id: u64,
    pub name: String,
    pub legs: Vec<LevelLeg>,
    pub equipment: Option<String>,
    pub event: Option<u64>,
}

/// A GNSS baseline (coordinate differences in the grid frame, to minus from) with stated standard
/// errors, no correlations. Left out: baselines with full covariance matrices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub from: String,
    pub to: String,
    pub de: f64,
    pub dn: f64,
    pub du: f64,
    pub sigma_e: f64,
    pub sigma_n: f64,
    pub sigma_u: f64,
    pub event: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
    pub description: String,
    pub points: Vec<SurveyPoint>,
    pub directions: Vec<KnownDirection>,
    pub setups: Vec<Setup>,
    pub level_runs: Vec<LevelRun>,
    pub baselines: Vec<Baseline>,
    pub figures: Vec<Figure>,
}

impl Network {
    pub fn new(name: &str) -> Network {
        Network {
            name: name.into(),
            description: String::new(),
            points: Vec::new(),
            directions: Vec::new(),
            setups: Vec::new(),
            level_runs: Vec::new(),
            baselines: Vec::new(),
            figures: Vec::new(),
        }
    }
    pub fn point(&self, name: &str) -> Option<&SurveyPoint> {
        self.points.iter().find(|p| p.name == name)
    }
    pub fn point_mut(&mut self, name: &str) -> Option<&mut SurveyPoint> {
        self.points.iter_mut().find(|p| p.name == name)
    }
    pub fn setup(&self, id: u64) -> Option<&Setup> {
        self.setups.iter().find(|s| s.id == id)
    }
}

/// What an import added, by the names and ids it created, so the event can be removed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AddedItems {
    pub points: Vec<String>,
    pub setups: Vec<u64>,
    pub level_runs: Vec<u64>,
    pub directions: usize,
    pub baselines: usize,
    pub figures: Vec<String>,
    pub observations: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportEvent {
    pub id: u64,
    pub file_name: String,
    /// SHA-256 of the text as read, lower case hex.
    pub sha256: String,
    /// Time as given by the caller (this crate reads no clock).
    pub imported_at: String,
    pub format: crate::import::ImportFormat,
    pub network: String,
    pub description: String,
    pub added: AddedItems,
    pub warnings: Vec<String>,
    /// The text, kept when the caller asked, so the event can be reprocessed.
    pub source_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeEntry {
    pub seq: u64,
    pub time: String,
    pub action: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurveyDatabase {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub settings: Settings,
    pub equipment: Vec<Equipment>,
    pub networks: Vec<Network>,
    pub import_events: Vec<ImportEvent>,
    pub figure_prefixes: Vec<FigurePrefix>,
    pub linework: LineworkCodes,
    pub change_log: Vec<ChangeEntry>,
    pub next_id: u64,
}

impl SurveyDatabase {
    pub fn new(name: &str) -> SurveyDatabase {
        SurveyDatabase {
            format: FORMAT_TAG.into(),
            version: FORMAT_VERSION,
            name: name.into(),
            settings: Settings::metric(),
            equipment: Vec::new(),
            networks: Vec::new(),
            import_events: Vec::new(),
            figure_prefixes: Vec::new(),
            linework: LineworkCodes::civil3d_documented(),
            change_log: Vec::new(),
            next_id: 1,
        }
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| SurveyError::Format(e.to_string()))
    }

    /// Read a database file. Rejects other format tags and newer versions.
    pub fn from_json(text: &str) -> Result<SurveyDatabase> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| SurveyError::Format(e.to_string()))?;
        let tag = v.get("format").and_then(|f| f.as_str()).unwrap_or("");
        if tag != FORMAT_TAG {
            return Err(SurveyError::Format(format!("format tag '{tag}' is not '{FORMAT_TAG}'")));
        }
        let ver = v.get("version").and_then(|f| f.as_u64()).unwrap_or(0);
        if ver == 0 || ver > FORMAT_VERSION as u64 {
            return Err(SurveyError::Format(format!("version {ver} is not supported (this crate reads up to {FORMAT_VERSION})")));
        }
        serde_json::from_value(v).map_err(|e| SurveyError::Format(e.to_string()))
    }

    pub fn log(&mut self, time: &str, action: &str, detail: &str) {
        let seq = self.change_log.len() as u64 + 1;
        self.change_log.push(ChangeEntry { seq, time: time.into(), action: action.into(), detail: detail.into() });
    }

    pub fn network(&self, name: &str) -> Result<&Network> {
        self.networks.iter().find(|n| n.name == name).ok_or_else(|| SurveyError::NoNetwork(name.into()))
    }
    pub fn network_mut(&mut self, name: &str) -> Result<&mut Network> {
        self.networks.iter_mut().find(|n| n.name == name).ok_or_else(|| SurveyError::NoNetwork(name.into()))
    }
    pub fn add_network(&mut self, name: &str, time: &str) -> Result<()> {
        if self.networks.iter().any(|n| n.name == name) {
            return Err(SurveyError::Invalid(format!("network '{name}' already exists")));
        }
        self.networks.push(Network::new(name));
        self.log(time, "add network", name);
        Ok(())
    }
    pub fn equipment(&self, name: &str) -> Option<&Equipment> {
        self.equipment.iter().find(|e| e.name == name)
    }
    pub(crate) fn fresh_id(&mut self) -> u64 {
        let i = self.next_id;
        self.next_id += 1;
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_and_tag_check() {
        let mut db = SurveyDatabase::new("job");
        db.settings.coordinate_system = "UTM 36S".into();
        db.settings.epsg = Some(32736);
        db.settings.corrections.atmospheric_ppm = Some(12.5);
        db.equipment.push(Equipment::exact("T1"));
        db.add_network("Main", "2026-11-16T10:00:00Z").unwrap();
        db.network_mut("Main").unwrap().points.push(SurveyPoint::control("A", 500000.0, 9_960_000.123, Some(1200.0)));
        let t = db.to_json().unwrap();
        let back = SurveyDatabase::from_json(&t).unwrap();
        assert_eq!(back, db);
        assert!(SurveyDatabase::from_json("{\"format\":\"x\",\"version\":1}").is_err());
        let newer = t.replacen("\"version\": 1", "\"version\": 99", 1);
        assert!(matches!(SurveyDatabase::from_json(&newer), Err(SurveyError::Format(_))));
        assert_eq!(back.network("Main").unwrap().points[0].n, Some(9_960_000.123));
        assert!(db.add_network("Main", "t").is_err());
    }
}
