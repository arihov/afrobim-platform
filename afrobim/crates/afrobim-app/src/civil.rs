//! The civil objects as document elements: points, surfaces, ground profiles, profile views,
//! assemblies, corridors, sample line groups and parcels.
//!
//! Each object is one element whose `data` is the engine's own serde type (kept in a small wrapper
//! where the app adds a style or a reference). The element sits at its source coordinates: its
//! origin is the least easting and northing of what it holds, times 1000 (model millimetres), and
//! its site placement says the same, so nothing is reprojected or shifted.
//!
//! Every object has a name (the element name), a status, a source record and a property set (the
//! element's `properties`, which the Properties panel edits), and an IFC mapping that is written
//! only where it was verified (see `docs/user-manual.md`).
//!
//! The geometry that is drawn is not part of the element: `civil_build` derives a [`Scene`] from the
//! data (on a worker thread, because a surface or a corridor can be slow) and the canvas paints it
//! (`civil_paint`). An element carries a revision number in `properties["civil.rev"]`; it changes on
//! every edit made through this module, so a scene is rebuilt only when it changed.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use afrobim_cogo::{DescriptionKeySet, PointGroupSet, PointStore};
use afrobim_corridor::{Assembly, CorridorModel, SampleLineGroup};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, ModuleRegistry, SitePlacement};
use serde::{Deserialize, Serialize};

pub const LAYER: &str = "Civil";
pub const POINTS: &str = "civil-points";
pub const SURFACE: &str = "civil-surface";
pub const PROFILE: &str = "civil-profile";
pub const PROFILE_VIEW: &str = "civil-profile-view";
pub const ASSEMBLY: &str = "civil-assembly";
pub const CORRIDOR: &str = "civil-corridor";
pub const SAMPLES: &str = "civil-samples";
pub const PARCEL: &str = "civil-parcel";
pub const PIPENET: &str = "civil-pipenet";
pub const PIPEPROFILE: &str = "civil-pipeprofile";
pub const FEATURELINE: &str = "civil-featureline";
pub const GRADING: &str = "civil-grading";
pub const INTERSECTION: &str = "civil-intersection";
pub const ROUNDABOUT: &str = "civil-roundabout";
pub const POINTCLOUD: &str = "civil-pointcloud";
pub const FIGURES: &str = "civil-figures";

/// Category, display name and the plural the toolspace shows.
pub const KINDS: [(&str, &str, &str); 16] = [
    (POINTS, "Point set", "Points"),
    (SURFACE, "Surface", "Surfaces"),
    (PROFILE, "Ground profile", "Profiles"),
    (PROFILE_VIEW, "Profile view", "Profile Views"),
    (ASSEMBLY, "Assembly", "Assemblies"),
    (CORRIDOR, "Corridor", "Corridors"),
    (SAMPLES, "Sample line group", "Sample Line Groups"),
    (PARCEL, "Parcel", "Parcels"),
    (PIPENET, "Pipe network", "Pipe Networks"),
    (PIPEPROFILE, "Pipe profile", "Pipe Profiles"),
    (FEATURELINE, "Feature line", "Feature Lines"),
    (GRADING, "Grading group", "Grading Groups"),
    (INTERSECTION, "Intersection", "Intersections"),
    (ROUNDABOUT, "Roundabout", "Roundabouts"),
    (FIGURES, "Survey figures", "Survey Figures"),
    (POINTCLOUD, "Point cloud", "Point Clouds"),
];

pub fn is_civil(category: &str) -> bool {
    category.starts_with("civil-")
}

pub const PROP_REV: &str = "civil.rev";
pub const PROP_HIDDEN: &str = "civil.hidden";
pub const PROP_STATUS: &str = "Status";
pub const PROP_SOURCE: &str = "Source";
/// The statuses an object can have (the same four words as the terrain engine's surface status).
pub const STATUSES: [&str; 4] = ["Draft", "For review", "Approved", "Superseded"];

static REV: AtomicU64 = AtomicU64::new(1);

pub fn next_rev() -> u64 {
    REV.fetch_add(1, Ordering::Relaxed)
}

/// Whether an element is hidden from the drawing (the toolspace's show and hide).
pub fn hidden(e: &Element) -> bool {
    e.properties.get(PROP_HIDDEN).is_some_and(|v| v == "1")
}

pub fn rev_of(e: &Element) -> String {
    e.properties.get(PROP_REV).cloned().unwrap_or_default()
}

// ─── data ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PointsData {
    pub store: PointStore,
    pub groups: PointGroupSet,
    pub keys: DescriptionKeySet,
    pub show_numbers: bool,
    pub show_elevations: bool,
    pub show_descriptions: bool,
    pub decimals: u8,
}

impl Default for PointsData {
    fn default() -> Self {
        PointsData {
            store: PointStore::new(),
            groups: PointGroupSet::new(),
            keys: DescriptionKeySet::new("Description keys"),
            show_numbers: true,
            show_elevations: true,
            show_descriptions: true,
            decimals: 2,
        }
    }
}

/// How a surface is shown. Every interval is an input: none has a default, and a blank one draws
/// nothing. (`SurfaceStyle::example` fills demonstration numbers, marked as such in the dialog.)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceStyle {
    pub triangles: bool,
    pub border: bool,
    pub minor_interval: Option<f64>,
    pub major_interval: Option<f64>,
    pub elevation_band_step: Option<f64>,
    pub slope_band_step_percent: Option<f64>,
    pub arrow_length: Option<f64>,
    pub arrow_min_slope_percent: Option<f64>,
}

impl SurfaceStyle {
    /// Numbers for a demonstration. They are not from any standard.
    pub fn example() -> SurfaceStyle {
        SurfaceStyle { triangles: false, border: true, minor_interval: Some(0.5), major_interval: Some(2.5), elevation_band_step: None, slope_band_step_percent: None, arrow_length: None, arrow_min_slope_percent: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceData {
    pub surface: afrobim_terrain::Surface,
    #[serde(default)]
    pub style: SurfaceStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    pub alignment: String,
    /// Element id of the surface it is sampled from.
    pub surface: u64,
    /// Spacing of the sample stations, an input.
    pub interval_m: f64,
    /// Levels read from a LandXML ProfSurf (station, level), shown instead of a sampled profile.
    #[serde(default)]
    pub imported: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileViewData {
    pub alignment: String,
    /// Ground profile elements shown (ids).
    pub profiles: Vec<u64>,
    pub show_design: bool,
    /// Vertical exaggeration (vertical scale over horizontal), an input.
    pub vertical_exaggeration: f64,
    /// Datum level in metres; blank follows the lowest level shown, rounded down.
    pub datum: Option<f64>,
    /// Insertion point of the view's lower left corner: easting, northing.
    pub insertion: [f64; 2],
    pub show_bands: bool,
    /// Height of the plotted ground and design lines above the datum is exaggerated; the bands sit below.
    pub band_height_m: f64,
    /// Point cloud slices along the alignment, drawn as points (station, level).
    #[serde(default)]
    pub cloud: Vec<CloudProfile>,
}

/// A point cloud sliced along an alignment at a thickness, for a profile view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudProfile {
    pub cloud: String,
    pub thickness_m: f64,
    /// Station (displayed internal station) and level.
    pub points: Vec<[f64; 2]>,
}

/// A point cloud sliced at the sample lines, for the section views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceSet {
    pub cloud: String,
    pub thickness_m: f64,
    pub lines: Vec<SliceLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceLine {
    /// The name of the sample line.
    pub name: String,
    /// Offset from the alignment (left negative) and level.
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorridorData {
    pub model: CorridorModel,
    /// Element id of the existing ground surface the corridor is cut against.
    pub ground: u64,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleData {
    pub group: SampleLineGroup,
    /// Element id of a corridor whose design is drawn in the sections, if any.
    pub corridor: Option<u64>,
    /// Element ids of the surfaces cut at each line.
    pub surfaces: Vec<u64>,
    /// Where the array of section views starts (lower left): easting, northing.
    pub insertion: [f64; 2],
    pub columns: usize,
    /// Vertical exaggeration of the views.
    pub vertical_exaggeration: f64,
    /// Point cloud slices drawn in the section views.
    #[serde(default)]
    pub slices: Vec<SliceSet>,
    /// Sections read from a LandXML file: the file's own named section surfaces at each line.
    #[serde(default)]
    pub imported: Vec<ImportedSection>,
    /// Comparison mode for imported sections: AfroBIM's own cut of `surfaces` is drawn over the
    /// file's lines and the largest level difference per station is reported.
    #[serde(default)]
    pub compare: bool,
}

/// One cross-section of a LandXML file: the sample line it belongs to and its named surfaces, as
/// runs of (offset, level) in metres, offsets negative to the left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedSection {
    pub line: String,
    pub station_m: f64,
    pub surfaces: Vec<ImportedSectionSurface>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedSectionSurface {
    pub name: String,
    pub runs: Vec<Vec<[f64; 2]>>,
}

// ─── element helpers ──────────────────────────────────────────────────────

/// The bounds a new element is given, in metres: least and greatest easting and northing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_e: f64,
    pub min_n: f64,
    pub max_e: f64,
    pub max_n: f64,
}

impl Bounds {
    pub fn point(e: f64, n: f64) -> Bounds {
        Bounds { min_e: e, min_n: n, max_e: e, max_n: n }
    }
    pub fn grow(&mut self, e: f64, n: f64) {
        self.min_e = self.min_e.min(e);
        self.min_n = self.min_n.min(n);
        self.max_e = self.max_e.max(e);
        self.max_n = self.max_n.max(n);
    }
    pub fn of(points: impl Iterator<Item = (f64, f64)>) -> Option<Bounds> {
        let mut b: Option<Bounds> = None;
        for (e, n) in points {
            if !(e.is_finite() && n.is_finite()) {
                continue;
            }
            match &mut b {
                Some(b) => b.grow(e, n),
                None => b = Some(Bounds::point(e, n)),
            }
        }
        b
    }
}

/// Adds an object at the coordinates of its source: origin and site placement are the least
/// easting and northing times 1000.
pub fn add(doc: &mut Document, category: &str, name: &str, data: serde_json::Value, bounds: Option<Bounds>, source: &str) -> ElementId {
    let name = doc.unique_name(if name.trim().is_empty() { kind_name(category) } else { name.trim() });
    let b = bounds.unwrap_or(Bounds::point(0.0, 0.0));
    let at = afrobim_kernel::model_point_from_crs(b.min_e, b.min_n);
    let (w, h) = (((b.max_e - b.min_e) * 1000.0).max(1.0), ((b.max_n - b.min_n) * 1000.0).max(1.0));
    doc.ensure_layer(LAYER);
    let id = doc.add(category, &name, at, w, h, LAYER, data);
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(SitePlacement { x: at.x, y: at.y, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None });
        e.properties.insert(PROP_REV.into(), next_rev().to_string());
        e.properties.insert(PROP_STATUS.into(), STATUSES[0].into());
        e.properties.insert(PROP_SOURCE.into(), source.to_string());
    }
    id
}

pub fn kind_name(category: &str) -> &'static str {
    KINDS.iter().find(|k| k.0 == category).map(|k| k.1).unwrap_or("Civil object")
}

/// The element's data decoded, if it is of that category and readable.
pub fn get<T: serde::de::DeserializeOwned>(doc: &Document, id: ElementId, category: &str) -> Option<T> {
    let e = doc.find(id)?;
    if e.category != category {
        return None;
    }
    serde_json::from_value(e.data.clone()).ok()
}

/// The profile view data of an element (the category is checked).
pub fn get_profile_view(doc: &Document, id: ElementId) -> Option<ProfileViewData> {
    get(doc, id, PROFILE_VIEW)
}

/// Replaces the element's data, moves it to the new bounds (source coordinates) and bumps its
/// revision so the scene is rebuilt.
pub fn set(doc: &mut Document, id: ElementId, data: serde_json::Value, bounds: Option<Bounds>) {
    if let Some(e) = doc.find_mut(id) {
        e.data = data;
        if let Some(b) = bounds {
            let at = afrobim_kernel::model_point_from_crs(b.min_e, b.min_n);
            e.origin = at;
            e.width = ((b.max_e - b.min_e) * 1000.0).max(1.0);
            e.height = ((b.max_n - b.min_n) * 1000.0).max(1.0);
            if let Some(s) = &mut e.site {
                s.x = at.x;
                s.y = at.y;
            }
        }
        e.properties.insert(PROP_REV.into(), next_rev().to_string());
    }
}

/// Bumps the revision without changing the data (a dependency changed).
pub fn touch(doc: &mut Document, id: ElementId) {
    if let Some(e) = doc.find_mut(id) {
        e.properties.insert(PROP_REV.into(), next_rev().to_string());
    }
}

pub fn of_category(doc: &Document, category: &str) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == category).map(|e| (e.id, e.name.clone())).collect()
}

/// The surfaces a profile or corridor can use: civil surfaces and imported TIN surfaces.
pub fn surface_choices(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == SURFACE || e.category == GRADING || e.category == afrobim_corridor::SURFACE_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

pub fn status_of(e: &Element) -> String {
    e.properties.get(PROP_STATUS).cloned().unwrap_or_default()
}

// ─── modules ──────────────────────────────────────────────────────────────

/// One kernel module per civil category. It generates no drawing of its own: the scene is built
/// elsewhere. It does check that the data reads, so a broken element says so in Properties.
pub struct CivilModule {
    pub category: &'static str,
    pub display: &'static str,
}

fn readable(category: &str, data: &serde_json::Value) -> Result<(), String> {
    fn ok<T: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Result<(), String> {
        serde_json::from_value::<T>(v.clone()).map(|_| ()).map_err(|e| e.to_string())
    }
    match category {
        POINTS => ok::<PointsData>(data),
        SURFACE => ok::<SurfaceData>(data),
        PROFILE => ok::<ProfileData>(data),
        PROFILE_VIEW => ok::<ProfileViewData>(data),
        ASSEMBLY => ok::<Assembly>(data),
        CORRIDOR => ok::<CorridorData>(data),
        SAMPLES => ok::<SampleData>(data),
        PARCEL => ok::<afrobim_cogo::Parcel>(data),
        PIPENET => ok::<crate::civil_pipes::PipeNetData>(data),
        PIPEPROFILE => ok::<crate::civil_pipes::PipeProfileData>(data),
        FEATURELINE => ok::<crate::civil_grading::FeatureLineData>(data),
        GRADING => ok::<crate::civil_grading::GradingData>(data),
        INTERSECTION => ok::<crate::civil_junction::IntersectionData>(data),
        ROUNDABOUT => ok::<crate::civil_junction::RoundaboutData>(data),
        FIGURES => ok::<crate::survey_db::FigureData>(data),
        POINTCLOUD => ok::<crate::cloud::CloudData>(data),
        _ => Ok(()),
    }
}

impl Module for CivilModule {
    fn category(&self) -> &str {
        self.category
    }
    fn display_name(&self) -> &str {
        self.display
    }
    fn generate(&self, element: &Element) -> Result<Generated, String> {
        // Reading a 100 000 point set again on every edit is wasted work: the points are checked
        // when they are loaded or changed through this module (the scene build decodes them).
        if self.category != POINTS {
            readable(self.category, &element.data).map_err(|e| format!("The {} data cannot be read: {e}", self.display.to_lowercase()))?;
        }
        Ok(Generated::default())
    }
}

pub fn register(modules: &mut ModuleRegistry) {
    for (c, d, _) in KINDS {
        modules.register(Box::new(CivilModule { category: c, display: d }));
    }
}

/// Group counts by category, for the toolspace.
pub fn counts(doc: &Document) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for (c, _, _) in KINDS {
        m.insert(c, doc.elements().iter().filter(|e| e.category == c).count());
    }
    m
}

/// Whether the category has a place in the plan (an assembly and a ground profile do not: they sit
/// at the origin and do not count when the view is fitted to the drawing).
pub fn has_plan_position(category: &str) -> bool {
    category != ASSEMBLY && category != PROFILE
}

/// Moves and sizes an element to bounds (source coordinates) without changing its revision: used
/// when the drawn extent of an object is known only after its scene is built.
pub fn set_bounds(doc: &mut Document, id: ElementId, b: Bounds) -> bool {
    if let Some(e) = doc.find_mut(id) {
        let at = afrobim_kernel::model_point_from_crs(b.min_e, b.min_n);
        let (w, h) = (((b.max_e - b.min_e) * 1000.0).max(1.0), ((b.max_n - b.min_n) * 1000.0).max(1.0));
        if (e.origin.x - at.x).abs() < 1.0 && (e.origin.y - at.y).abs() < 1.0 && (e.width - w).abs() < 1.0 && (e.height - h).abs() < 1.0 {
            return false;
        }
        e.origin = at;
        e.width = w;
        e.height = h;
        if let Some(s) = &mut e.site {
            s.x = at.x;
            s.y = at.y;
        }
        return true;
    }
    false
}
