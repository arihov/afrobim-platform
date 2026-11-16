//! Reprojection as an explicit, logged operation.
//!
//! The rule of the application stays: nothing is reprojected silently. A transform is something
//! the user asks for ([`plan_transform`] returns the candidates), sees the accuracy of (every
//! [`TransformPlan`] carries the registry's stated accuracy and warnings) and finds in a log
//! ([`TransformLog`], which serialises with serde so a document can keep it). The caller chooses
//! among the candidates; no plan is applied by this module on its own.
//!
//! # Heights
//!
//! The kernel has no geoid model and no vertical transformation. A datum transformation between
//! geographic systems is three-dimensional (GN7-2 section 4.3.1 goes geographic, geocentric,
//! transformation, geocentric, geographic), so a height must be put in. GN7-2 4.1.4 says what is
//! done with a two-dimensional position: "an artificial ellipsoidal height is created ... usually
//! either set to the gravity-related height ... or set to zero. As long as the height chosen is
//! within a few kilometres of sea level, no error will be induced into the horizontal position
//! ... the vertical coordinate will however be meaningless."
//!
//! Here: [`TransformPlan::apply`] takes the height the caller has and uses it, as given, as the
//! ellipsoidal height of the input (an assumption: an elevation above a levelling datum is not an
//! ellipsoidal height; the two differ by the geoid separation, which the kernel does not know),
//! transforms the horizontal position, and returns the height **unchanged**. The assumption can
//! instead be set to zero ([`HeightAssumption::Zero`]). How much the assumption can move the
//! horizontal result is a property of the plan: [`TransformPlan::height_sensitivity`] measures it,
//! and the README gives the size from the formulas.
//!
//! # Where the formulas come from
//!
//! GN7-2 = IOGP Publication 373-7-2, Geomatics Guidance Note number 7, part 2, September 2019
//! (see [`crate::crs_transform`] for the URL and what was and was not read). Section numbers are
//! cited at each item. The combined scale factor is not in GN7-2; its source is cited at
//! [`combined_scale_factor`].

use crate::crs::{AreaOfUse, CrsKind, ProjectCrs, TransverseMercator};
use crate::crs_transform::{
    coverage_of, geocentric_to_geographic, geographic_crs, geographic_to_geocentric, transformations_between, Coverage, GeographicCrs, Transformation,
};
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------------------------
// Inputs and outputs

/// The plan extent of the data, in the coordinates of the source system, in its axis order: easting
/// then northing in metres for a projected system (westing then southing for a south-orientated
/// one), latitude then longitude in degrees for a geographic one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DataExtent {
    pub min: (f64, f64),
    pub max: (f64, f64),
}

impl DataExtent {
    pub fn new(min: (f64, f64), max: (f64, f64)) -> Self {
        DataExtent { min, max }
    }
}

/// What is assumed for the ellipsoidal height of an input position during the datum step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HeightAssumption {
    /// The height the caller passes is taken as the ellipsoidal height (GN7-2 4.1.4: "usually ... set
    /// to the gravity-related height of a position"). Default.
    #[default]
    AsGiven,
    /// Zero (GN7-2 4.1.4: "or set to zero").
    Zero,
}

impl HeightAssumption {
    pub fn sentence(self) -> &'static str {
        match self {
            HeightAssumption::AsGiven => {
                "Heights were not transformed. The height of each position was used as given for its ellipsoidal height in the horizontal datum step (an assumption: no geoid model is available) and is returned unchanged."
            }
            HeightAssumption::Zero => {
                "Heights were not transformed. An ellipsoidal height of zero was assumed for each position in the horizontal datum step (no geoid model is available); heights are returned unchanged."
            }
        }
    }
}

/// A transformed position. `height_m` is the input height, unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformedPoint {
    pub first: f64,
    pub second: f64,
    pub height_m: f64,
}

/// Why a point could not be transformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformError {
    NotFinite,
    /// The latitude is beyond 90 degrees, or the point is too far from the central meridian for
    /// the projection series (80 degrees).
    OutsideProjection,
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformError::NotFinite => f.write_str("a coordinate is not a finite number"),
            TransformError::OutsideProjection => f.write_str("the position is outside what the projection can compute (latitude beyond 90 degrees or more than 80 degrees from the central meridian)"),
        }
    }
}

impl std::error::Error for TransformError {}

/// The result of transforming many positions: one entry per input, `None` where it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchResult {
    pub points: Vec<Option<TransformedPoint>>,
    pub transformed: usize,
    pub failed: usize,
}

/// Why no plan can be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoPath {
    /// A local site grid has no geodetic meaning: use a [`SiteCalibration`] to relate it to a system.
    LocalGrid { name: String },
    /// Known by name or code only; no maths here.
    NamedOnly { label: String },
    /// Both systems are known but the registry records fetched give no transformation between their datums.
    NoTransformation { from_datum: String, to_datum: String },
}

impl fmt::Display for NoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoPath::LocalGrid { name } => write!(f, "{name} is a local site grid with no geodetic meaning, so nothing can be reprojected to or from it. Fit a site calibration from control points instead."),
            NoPath::NamedOnly { label } => write!(f, "{label} is recorded by name or code only; the application has no maths for it, so there is no path to or from it."),
            NoPath::NoTransformation { from_datum, to_datum } => write!(
                f,
                "No transformation from {from_datum} to {to_datum} is recorded in the registry data the kernel holds. Nothing is invented: the datum cannot be changed here."
            ),
        }
    }
}

impl std::error::Error for NoPath {}

// ---------------------------------------------------------------------------------------------
// Plans

/// The coordinate system at one end, as recorded in a plan or a log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrsRef {
    pub epsg: Option<u32>,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanKind {
    /// Same system at both ends: nothing changes.
    Identity,
    /// Same datum, projection change only: exact up to the projection series (sub-millimetre), no
    /// datum accuracy figure applies.
    ProjectionOnly,
    /// A datum transformation of the registry is applied.
    DatumTransformation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepKind {
    InverseProjection,
    DatumTransformation,
    ForwardProjection,
}

/// One step of a plan, as it is shown to the user and kept in the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRecord {
    pub kind: StepKind,
    /// The EPSG code of the transformation, or of the projected system for a projection step.
    pub epsg: Option<u32>,
    pub name: String,
    pub method: String,
    /// The registry's accuracy, for a datum transformation; none for a projection step.
    pub accuracy_m: Option<f64>,
    /// The registry lists the transformation the other way round and its reverse is applied.
    pub reversed: bool,
    pub parameters: String,
    pub area_text: String,
    pub revision_date: String,
    pub source_url: String,
}

#[derive(Debug, Clone)]
struct Endpoint {
    geog: GeographicCrs,
    projection: Option<TransverseMercator>,
    epsg: u32,
    name: String,
    /// The area of use of the system itself (a projected system's own, else the geographic one's).
    area: AreaOfUse,
}

fn resolve(crs: &ProjectCrs) -> Result<Endpoint, NoPath> {
    match crs.kind {
        CrsKind::LocalGrid => return Err(NoPath::LocalGrid { name: crs.name.clone() }),
        CrsKind::NamedOnly | CrsKind::Projected => {}
    }
    let named = || NoPath::NamedOnly { label: crs.label() };
    let code = crs.epsg.ok_or_else(named)?;
    if let Some(known) = ProjectCrs::from_epsg(code) {
        let projection = known.projection().ok_or_else(named)?;
        let geog = geographic_crs(known.geographic_epsg().ok_or_else(named)?).ok_or_else(named)?;
        return Ok(Endpoint { geog, projection: Some(projection), epsg: code, name: known.name.clone(), area: known.area_of_use().ok_or_else(named)? });
    }
    let geog = geographic_crs(code).ok_or_else(named)?;
    Ok(Endpoint { geog, projection: None, epsg: code, name: geog.name.to_string(), area: geog.area })
}

type Chain = Vec<(Transformation, bool)>;

fn chains(from: u32, to: u32) -> Vec<Chain> {
    if from == to {
        return vec![Vec::new()];
    }
    let direct: Vec<Chain> = transformations_between(from, to).into_iter().map(|x| vec![x]).collect();
    if !direct.is_empty() || from == 4326 || to == 4326 {
        return direct;
    }
    // Through WGS 84, the target of every recorded transformation.
    let first = transformations_between(from, 4326);
    let second = transformations_between(4326, to);
    let mut out = Vec::new();
    for a in &first {
        for b in &second {
            out.push(vec![*a, *b]);
        }
    }
    out
}

/// A way to change one coordinate system into another, with everything the user should see before
/// agreeing to it.
#[derive(Debug, Clone)]
pub struct TransformPlan {
    pub from: CrsRef,
    pub to: CrsRef,
    pub kind: PlanKind,
    pub steps: Vec<StepRecord>,
    /// The sum of the stated accuracies of the datum steps, metres. The registry gives no way to
    /// combine them statistically, so the sum is the cautious figure. `Some(0.0)` for a plan with no
    /// datum step: the projection step has no datum uncertainty and the series error is below a
    /// millimetre. See `accuracy_basis`.
    pub accuracy_m: Option<f64>,
    pub accuracy_basis: String,
    /// Things to look at before applying the plan: the extent leaves an area of use, a transformation
    /// is superseded, and so on.
    pub warnings: Vec<String>,
    /// Facts that are not warnings.
    pub notes: Vec<String>,
    pub height: HeightAssumption,
    src: Endpoint,
    dst: Endpoint,
    chain: Chain,
}

fn geographic_corners(src: &Endpoint, e: &DataExtent) -> Option<(f64, f64, f64, f64)> {
    let (mut lo_lat, mut lo_lon, mut hi_lat, mut hi_lon) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for i in 0..3 {
        for j in 0..3 {
            let a = e.min.0 + (e.max.0 - e.min.0) * f64::from(i) / 2.0;
            let b = e.min.1 + (e.max.1 - e.min.1) * f64::from(j) / 2.0;
            let (lat, lon) = match &src.projection {
                Some(p) => p.inverse(a, b)?,
                None => (a, b),
            };
            lo_lat = lo_lat.min(lat);
            hi_lat = hi_lat.max(lat);
            lo_lon = lo_lon.min(lon);
            hi_lon = hi_lon.max(lon);
        }
    }
    Some((lo_lat, lo_lon, hi_lat, hi_lon))
}

fn area_sentence(a: &AreaOfUse) -> String {
    format!("latitude {} to {}, longitude {} to {}", a.south, a.north, a.west, a.east)
}

impl TransformPlan {
    fn build(src: Endpoint, dst: Endpoint, chain: Chain, extent: Option<&DataExtent>, height: HeightAssumption) -> TransformPlan {
        let from = CrsRef { epsg: Some(src.epsg), name: src.name.clone() };
        let to = CrsRef { epsg: Some(dst.epsg), name: dst.name.clone() };
        let kind = if src.epsg == dst.epsg {
            PlanKind::Identity
        } else if chain.is_empty() {
            PlanKind::ProjectionOnly
        } else {
            PlanKind::DatumTransformation
        };
        let mut steps = Vec::new();
        if kind != PlanKind::Identity {
            if src.projection.is_some() {
                steps.push(StepRecord {
                    kind: StepKind::InverseProjection,
                    epsg: Some(src.epsg),
                    name: src.name.clone(),
                    method: "Transverse Mercator inverse (Krueger series, third order; GN7-2 3.2.3.1)".to_string(),
                    accuracy_m: None,
                    reversed: false,
                    parameters: String::new(),
                    area_text: area_sentence(&src.area),
                    revision_date: String::new(),
                    source_url: format!("https://epsg.io/{}", src.epsg),
                });
            }
            for (t, rev) in &chain {
                steps.push(StepRecord {
                    kind: StepKind::DatumTransformation,
                    epsg: Some(t.epsg),
                    name: if *rev { format!("{} (reversed)", t.name) } else { t.name.to_string() },
                    method: t.method.label(),
                    accuracy_m: Some(t.accuracy_m),
                    reversed: *rev,
                    parameters: if *rev { format!("{} (signs reversed on use)", t.method.parameter_text()) } else { t.method.parameter_text() },
                    area_text: format!("{} ({})", t.area_text, area_sentence(&t.area)),
                    revision_date: t.revision_date.to_string(),
                    source_url: t.source_url.to_string(),
                });
            }
            if dst.projection.is_some() {
                steps.push(StepRecord {
                    kind: StepKind::ForwardProjection,
                    epsg: Some(dst.epsg),
                    name: dst.name.clone(),
                    method: "Transverse Mercator forward (Krueger series, third order; GN7-2 3.2.3.1)".to_string(),
                    accuracy_m: None,
                    reversed: false,
                    parameters: String::new(),
                    area_text: area_sentence(&dst.area),
                    revision_date: String::new(),
                    source_url: format!("https://epsg.io/{}", dst.epsg),
                });
            }
        }
        let accuracy_m = Some(chain.iter().map(|(t, _)| t.accuracy_m).sum::<f64>());
        let accuracy_basis = match kind {
            PlanKind::Identity => "Same system at both ends: no change.".to_string(),
            PlanKind::ProjectionOnly => "Same datum, projection change only: no datum transformation, so no datum accuracy figure applies. The projection series agree with the inverse to under a millimetre.".to_string(),
            PlanKind::DatumTransformation => {
                let parts: Vec<String> = chain.iter().map(|(t, _)| format!("EPSG:{} {} m", t.epsg, t.accuracy_m)).collect();
                format!(
                    "The registry's stated accuracy of the datum transformation{}: {}{}. This is the figure the registry gives for the transformation over its area of use, not a measured error of these points. The source of stated accuracy for EPSG:4326 itself is a datum ensemble of 2.0 as the registry page shows it (no unit shown on the page); it is not added.",
                    if chain.len() > 1 { "s, summed (the registry gives no rule for combining them)" } else { "" },
                    parts.join(" + "),
                    if chain.len() > 1 { format!(" = {} m", accuracy_m.unwrap_or(0.0)) } else { String::new() }
                )
            }
        };
        let mut plan = TransformPlan { from, to, kind, steps, accuracy_m, accuracy_basis, warnings: Vec::new(), notes: Vec::new(), height, src, dst, chain };
        plan.check_extent(extent);
        plan
    }

    fn check_extent(&mut self, extent: Option<&DataExtent>) {
        let Some(extent) = extent else {
            self.notes.push("No data extent was given, so no area of use was checked.".to_string());
            return;
        };
        if ![extent.min.0, extent.min.1, extent.max.0, extent.max.1].iter().all(|v| v.is_finite()) {
            self.warnings.push("The data extent is not finite; no area of use was checked.".to_string());
            return;
        }
        let Some((lo_lat, lo_lon, hi_lat, hi_lon)) = geographic_corners(&self.src, extent) else {
            self.warnings.push("The data extent cannot be placed on the Earth in the source system (outside what the projection can compute); no area of use was checked.".to_string());
            return;
        };
        let place = format!("latitude {lo_lat:.4} to {hi_lat:.4}, longitude {lo_lon:.4} to {hi_lon:.4}");
        let mut warnings = Vec::new();
        let check = |what: String, area: &AreaOfUse, text: &str, tail: &str| -> Option<String> {
            match coverage_of(area, lo_lat, lo_lon, hi_lat, hi_lon) {
                Coverage::Full => None,
                Coverage::Partial => Some(format!("The data ({place}) reaches outside the area of use of {what} ({text}, {}). {tail}", area_sentence(area))),
                Coverage::Outside => Some(format!("The data ({place}) is entirely outside the area of use of {what} ({text}, {}). {tail}", area_sentence(area))),
            }
        };
        if self.src.projection.is_some() {
            warnings.extend(check(format!("the source system {} (EPSG:{})", self.src.name, self.src.epsg), &self.src.area, "its area of use", "Check the zone or datum of the data."));
        }
        for (t, rev) in &self.chain {
            let tail = format!("The stated accuracy of {} m for EPSG:{} does not apply there.", t.accuracy_m, t.epsg);
            warnings.extend(check(format!("transformation EPSG:{} {}{}", t.epsg, t.name, if *rev { " (applied in reverse)" } else { "" }), &t.area, t.area_text, &tail));
            if let Some(newer) = t.superseded_by {
                warnings.push(format!("The registry records that EPSG:{} is replaced by EPSG:{}.", t.epsg, newer));
            }
        }
        // The destination, judged where the data lands.
        if self.dst.projection.is_some() {
            let mut lo = (f64::MAX, f64::MAX);
            let mut hi = (f64::MIN, f64::MIN);
            for i in 0..3 {
                for j in 0..3 {
                    let a = extent.min.0 + (extent.max.0 - extent.min.0) * f64::from(i) / 2.0;
                    let b = extent.min.1 + (extent.max.1 - extent.min.1) * f64::from(j) / 2.0;
                    let (lat, lon) = match &self.src.projection {
                        Some(p) => match p.inverse(a, b) {
                            Some(x) => x,
                            None => continue,
                        },
                        None => (a, b),
                    };
                    let (lat2, lon2, _) = self.datum_step(lat, lon, 0.0);
                    lo = (lo.0.min(lat2), lo.1.min(lon2));
                    hi = (hi.0.max(lat2), hi.1.max(lon2));
                }
            }
            if lo.0 <= hi.0 {
                match coverage_of(&self.dst.area, lo.0, lo.1, hi.0, hi.1) {
                    Coverage::Full => {}
                    c => warnings.push(format!(
                        "The data lands {} the area of use of the target system {} (EPSG:{}; {}). Distortion may exceed what the system is meant for.",
                        if c == Coverage::Outside { "entirely outside" } else { "partly outside" },
                        self.dst.name,
                        self.dst.epsg,
                        area_sentence(&self.dst.area)
                    )),
                }
            }
        }
        self.warnings.extend(warnings);
    }

    /// Latitude, longitude, height on the source datum to the target datum.
    fn datum_step(&self, lat: f64, lon: f64, h: f64) -> (f64, f64, f64) {
        if self.chain.is_empty() {
            return (lat, lon, h);
        }
        let mut v = geographic_to_geocentric(&self.src.geog.ellipsoid, lat, lon, h);
        for (t, rev) in &self.chain {
            v = t.method.apply_geocentric(v, *rev);
        }
        geocentric_to_geographic(&self.dst.geog.ellipsoid, v)
    }

    /// The plan with another assumption for the ellipsoidal height of the input.
    pub fn with_height_assumption(mut self, h: HeightAssumption) -> TransformPlan {
        self.height = h;
        self
    }

    /// The three-dimensional operation: the horizontal position and the ellipsoidal height of the
    /// input in, the horizontal position and the ellipsoidal height on the target datum out. Use it
    /// for an exact round trip (the 2D [`apply`](Self::apply) drops the height). The height out is
    /// a height above the target ellipsoid, not an elevation.
    pub fn apply_3d(&self, first: f64, second: f64, ellipsoidal_height_m: f64) -> Result<(f64, f64, f64), TransformError> {
        if ![first, second, ellipsoidal_height_m].iter().all(|v| v.is_finite()) {
            return Err(TransformError::NotFinite);
        }
        if self.kind == PlanKind::Identity {
            return Ok((first, second, ellipsoidal_height_m));
        }
        let (lat, lon) = match &self.src.projection {
            Some(p) => p.inverse(first, second).ok_or(TransformError::OutsideProjection)?,
            None => {
                if first.abs() > 90.0 {
                    return Err(TransformError::OutsideProjection);
                }
                (first, second)
            }
        };
        let (lat2, lon2, h2) = self.datum_step(lat, lon, ellipsoidal_height_m);
        let lon2 = wrap_longitude(lon2);
        match &self.dst.projection {
            Some(p) => {
                let (a, b) = p.forward(lat2, lon2).ok_or(TransformError::OutsideProjection)?;
                Ok((a, b, h2))
            }
            None => Ok((lat2, lon2, h2)),
        }
    }

    /// One position. `height_m` is the elevation or height the caller has: it is used as the
    /// ellipsoidal height of the input in the datum step (or zero, per [`HeightAssumption`]) and is
    /// returned unchanged. Coordinates in the axis order of the source system (see [`DataExtent`]);
    /// the result is in the axis order of the target system.
    pub fn apply(&self, first: f64, second: f64, height_m: f64) -> Result<TransformedPoint, TransformError> {
        let h = match self.height {
            HeightAssumption::AsGiven => height_m,
            HeightAssumption::Zero => 0.0,
        };
        let (a, b, _) = self.apply_3d(first, second, if h.is_finite() { h } else { 0.0 })?;
        if !height_m.is_finite() {
            return Err(TransformError::NotFinite);
        }
        Ok(TransformedPoint { first: a, second: b, height_m })
    }

    /// Many positions; a position that fails is `None` and counted.
    pub fn apply_batch(&self, points: &[(f64, f64, f64)]) -> BatchResult {
        let out: Vec<Option<TransformedPoint>> = points.iter().map(|&(a, b, h)| self.apply(a, b, h).ok()).collect();
        let failed = out.iter().filter(|p| p.is_none()).count();
        BatchResult { transformed: out.len() - failed, failed, points: out }
    }

    /// How far, in metres in the target system, the horizontal result moves when the assumed
    /// ellipsoidal height of the input changes by `delta_h_m`: the size of the effect of the height
    /// assumption at this position. Zero for a plan with no datum step. `None` when a position
    /// cannot be computed.
    pub fn height_sensitivity(&self, first: f64, second: f64, base_height_m: f64, delta_h_m: f64) -> Option<f64> {
        let a = self.apply_3d(first, second, base_height_m).ok()?;
        let b = self.apply_3d(first, second, base_height_m + delta_h_m).ok()?;
        match self.dst.projection {
            Some(_) => Some((a.0 - b.0).hypot(a.1 - b.1)),
            None => {
                let lat = a.0.to_radians();
                let (n, e) = ((a.0 - b.0).to_radians() * 6_371_000.0, (a.1 - b.1).to_radians() * lat.cos() * 6_371_000.0);
                Some(n.hypot(e))
            }
        }
    }

    /// The same operation the other way, with the registry's transformations reversed. No data
    /// extent is known for it, so no area is checked.
    pub fn reversed(&self) -> TransformPlan {
        let chain: Chain = self.chain.iter().rev().map(|(t, r)| (*t, !*r)).collect();
        TransformPlan::build(self.dst.clone(), self.src.clone(), chain, None, self.height)
    }

    /// The EPSG codes of the datum transformations used, in order.
    pub fn transformation_codes(&self) -> Vec<u32> {
        self.chain.iter().map(|(t, _)| t.epsg).collect()
    }

    /// What the plan does to bearings and distances at a position of the source system.
    ///
    /// Bearings: a line has grid bearing `theta` (clockwise from grid north) and true azimuth
    /// `alpha = theta + gamma`, with gamma the grid convergence (the angle from grid north to true
    /// north, counterclockwise positive, as [`TransverseMercator::scale_and_convergence`] returns it). Moving
    /// to another grid changes the grid bearing of the same line by `gamma_source - gamma_target`.
    /// The change of true azimuth caused by the datum step is not included; the tests measure it
    /// and it is far smaller (see the README). Distances: the ratio of the two point scale factors
    /// `k_target / k_source`; see [`baseline_effect`](Self::baseline_effect) for the measured value.
    pub fn point_effects(&self, first: f64, second: f64, height_m: f64) -> Option<PointEffects> {
        let moved = self.apply(first, second, height_m).ok()?;
        let src = self.src.projection.as_ref().filter(|p| !p.is_south_oriented());
        let dst = self.dst.projection.as_ref().filter(|p| !p.is_south_oriented());
        let sc = match (&self.src.projection, src) {
            (Some(p), Some(_)) => {
                let (lat, lon) = p.inverse(first, second)?;
                p.scale_and_convergence(lat, lon)
            }
            _ => None,
        };
        let tc = match (&self.dst.projection, dst) {
            (Some(p), Some(_)) => {
                let (lat, lon) = p.inverse(moved.first, moved.second)?;
                p.scale_and_convergence(lat, lon)
            }
            _ => None,
        };
        let mut notes = Vec::new();
        if self.src.projection.is_some() && sc.is_none() || self.dst.projection.is_some() && tc.is_none() {
            notes.push("A south-orientated grid (westing, southing) has no grid convergence defined here, so no bearing change is given for this plan.".to_string());
        }
        if self.src.projection.is_none() || self.dst.projection.is_none() {
            notes.push("A geographic system has no grid: bearings and distances on the other side are grid values and are not compared.".to_string());
        }
        Some(PointEffects {
            source_scale: sc.map(|x| x.0),
            target_scale: tc.map(|x| x.0),
            scale_ratio: sc.zip(tc).map(|(a, b)| b.0 / a.0),
            source_convergence_deg: sc.map(|x| x.1),
            target_convergence_deg: tc.map(|x| x.1),
            grid_bearing_change_deg: sc.zip(tc).map(|(a, b)| a.1 - b.1),
            notes,
        })
    }

    /// The same effects measured: a baseline of `length_m` at grid bearing `grid_bearing_deg`
    /// (clockwise from grid north) from a position of the source system is transformed at both
    /// ends, and its length and grid bearing compared. Only for plans between projected systems
    /// that are not south-orientated.
    pub fn baseline_effect(&self, first: f64, second: f64, height_m: f64, grid_bearing_deg: f64, length_m: f64) -> Option<BaselineEffect> {
        if self.src.projection.as_ref().is_none_or(|p| p.is_south_oriented()) || self.dst.projection.as_ref().is_none_or(|p| p.is_south_oriented()) {
            return None;
        }
        let (s, c) = grid_bearing_deg.to_radians().sin_cos();
        let a = self.apply(first, second, height_m).ok()?;
        let b = self.apply(first + length_m * s, second + length_m * c, height_m).ok()?;
        let (de, dn) = (b.first - a.first, b.second - a.second);
        let target_length = de.hypot(dn);
        let target_bearing = de.atan2(dn).to_degrees();
        Some(BaselineEffect {
            source_length_m: length_m,
            target_length_m: target_length,
            length_ratio: target_length / length_m,
            source_grid_bearing_deg: grid_bearing_deg,
            target_grid_bearing_deg: target_bearing,
            grid_bearing_change_deg: wrap_degrees(target_bearing - grid_bearing_deg),
        })
    }
}

fn wrap_longitude(lon: f64) -> f64 {
    wrap_degrees(lon)
}

fn wrap_degrees(x: f64) -> f64 {
    let mut v = x % 360.0;
    if v > 180.0 {
        v -= 360.0;
    } else if v <= -180.0 {
        v += 360.0;
    }
    v
}

/// What a plan does to bearings and distances at a position; see [`TransformPlan::point_effects`].
#[derive(Debug, Clone, PartialEq)]
pub struct PointEffects {
    pub source_scale: Option<f64>,
    pub target_scale: Option<f64>,
    /// `target_scale / source_scale`: multiply a source grid distance by this for the target grid distance.
    pub scale_ratio: Option<f64>,
    pub source_convergence_deg: Option<f64>,
    pub target_convergence_deg: Option<f64>,
    /// Add this to a source grid bearing for the target grid bearing of the same line.
    pub grid_bearing_change_deg: Option<f64>,
    pub notes: Vec<String>,
}

impl PointEffects {
    /// The plain sentence for the user.
    pub fn sentence(&self) -> String {
        match (self.grid_bearing_change_deg, self.scale_ratio) {
            (Some(b), Some(r)) => format!(
                "At this position a grid bearing changes by {b:+.5} degrees ({:+.1} arc-seconds) and a grid distance is multiplied by {r:.8} ({:+.1} mm per km).",
                b * 3600.0,
                (r - 1.0) * 1e6
            ),
            _ => self.notes.join(" "),
        }
    }
}

/// A measured baseline; see [`TransformPlan::baseline_effect`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineEffect {
    pub source_length_m: f64,
    pub target_length_m: f64,
    pub length_ratio: f64,
    pub source_grid_bearing_deg: f64,
    pub target_grid_bearing_deg: f64,
    pub grid_bearing_change_deg: f64,
}

/// The candidate plans from one system to another, best coverage and accuracy first. The caller
/// chooses; nothing is applied. `extent` is the data's extent in the source system (see
/// [`DataExtent`]); with it each plan carries warnings where the data leaves an area of use.
///
/// * Same system at both ends: one plan of kind [`PlanKind::Identity`].
/// * Same datum: one plan of kind [`PlanKind::ProjectionOnly`].
/// * Different datums: one plan for every registry transformation (or pair, through WGS 84) that
///   links them, whether or not it covers the data; those that do not carry a warning. Plans that
///   cover the extent come first, then by stated accuracy.
/// * A local grid, a named-only system, or datums with no recorded transformation: [`NoPath`].
pub fn plan_transform(from: &ProjectCrs, to: &ProjectCrs, extent: Option<DataExtent>) -> Result<Vec<TransformPlan>, NoPath> {
    let src = resolve(from)?;
    let dst = resolve(to)?;
    let options = chains(src.geog.epsg, dst.geog.epsg);
    if options.is_empty() {
        return Err(NoPath::NoTransformation { from_datum: src.geog.datum.to_string(), to_datum: dst.geog.datum.to_string() });
    }
    let mut plans: Vec<TransformPlan> = options.into_iter().map(|c| TransformPlan::build(src.clone(), dst.clone(), c, extent.as_ref(), HeightAssumption::default())).collect();
    plans.sort_by(|a, b| {
        let na = a.warnings.len();
        let nb = b.warnings.len();
        na.cmp(&nb).then(a.accuracy_m.unwrap_or(0.0).total_cmp(&b.accuracy_m.unwrap_or(0.0))).then(a.transformation_codes().cmp(&b.transformation_codes()))
    });
    Ok(plans)
}

// ---------------------------------------------------------------------------------------------
// The log

/// The record of one transformation, kept in the document. Serialises with serde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformLog {
    /// What was transformed, in the caller's words (a layer, an import, a surface).
    pub what: String,
    pub from: CrsRef,
    pub to: CrsRef,
    pub kind: PlanKind,
    /// The steps with their EPSG codes, parameters, areas and stated accuracies.
    pub steps: Vec<StepRecord>,
    /// The stated accuracy of the plan, metres; see `accuracy_basis`.
    pub accuracy_m: Option<f64>,
    pub accuracy_basis: String,
    pub warnings: Vec<String>,
    /// What happened to heights.
    pub height_treatment: String,
    /// When, as the caller gives it (for example an ISO 8601 text). The kernel reads no clock.
    pub when: Option<String>,
    pub points_in: usize,
    pub points_transformed: usize,
    pub points_failed: usize,
}

impl TransformLog {
    /// The record of applying `plan` to a batch.
    pub fn from_batch(what: &str, plan: &TransformPlan, when: Option<String>, batch: &BatchResult) -> TransformLog {
        TransformLog::with_counts(what, plan, when, batch.points.len(), batch.transformed, batch.failed)
    }

    pub fn with_counts(what: &str, plan: &TransformPlan, when: Option<String>, points_in: usize, transformed: usize, failed: usize) -> TransformLog {
        TransformLog {
            what: what.to_string(),
            from: plan.from.clone(),
            to: plan.to.clone(),
            kind: plan.kind,
            steps: plan.steps.clone(),
            accuracy_m: plan.accuracy_m,
            accuracy_basis: plan.accuracy_basis.clone(),
            warnings: plan.warnings.clone(),
            height_treatment: if plan.kind == PlanKind::DatumTransformation { plan.height.sentence().to_string() } else { "Heights were not changed.".to_string() },
            when,
            points_in,
            points_transformed: transformed,
            points_failed: failed,
        }
    }

    /// One line for a list.
    pub fn summary(&self) -> String {
        let codes: Vec<String> = self.steps.iter().filter(|s| s.kind == StepKind::DatumTransformation).filter_map(|s| s.epsg).map(|c| format!("EPSG:{c}")).collect();
        format!(
            "{}: {} to {}{}; {} points transformed{}{}.",
            self.what,
            self.from.name,
            self.to.name,
            if codes.is_empty() { ", projection change only".to_string() } else { format!(", datum transformation {}", codes.join(" + ")) },
            self.points_transformed,
            if self.points_failed > 0 { format!(", {} failed", self.points_failed) } else { String::new() },
            match self.accuracy_m {
                Some(a) if !codes.is_empty() => format!(", stated accuracy {a} m"),
                _ => String::new(),
            }
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Combined scale factor

/// A mean radius of the Earth, "approximately 6,371,000 m", as one open surveying reference gives
/// it for the elevation factor. Not from the registry. The caller may pass another radius.
pub const MEAN_EARTH_RADIUS_M: f64 = 6_371_000.0;

/// The elevation factor `R / (R + h)`, h the ellipsoidal height and R the radius of the Earth.
///
/// Sources (not GN7-2, which has no such formula; both fetched 2026-11-16): "Scale Factors -
/// Geodesy for Surveyors", <https://www.surveybible.com/learn/geodesy/scale-factors> (no author or
/// date shown; cites Ghilani and Wolf, Elementary Surveying 13th ed.): "Elevation Factor = R / (R +
/// h)", R the mean radius of the Earth (about 6,371,000 m), h the ellipsoid height, "not the
/// orthometric elevation"; and S. Billings, "Ground versus Grid - Low Distortion Projections - Part
/// 2", The American Surveyor, 28 September 2013,
/// <https://amerisurv.com/2013/09/28/ground-versus-grid-low-distortion-projections-part-2/>:
/// "Elevation Factor = 20,906,000 / [20,906,000 + Ellipsoid Height]".
/// `None` when R + h is not positive or an input is not finite.
pub fn elevation_factor(ellipsoidal_height_m: f64, radius_m: f64) -> Option<f64> {
    let d = radius_m + ellipsoidal_height_m;
    (d > 0.0 && d.is_finite() && radius_m > 0.0).then(|| radius_m / d)
}

/// The combined scale factor, the point scale factor times the elevation factor: `CF = k R / (R +
/// h)`; a ground distance times CF is the grid distance. Same two sources as
/// [`elevation_factor`] (surveybible: "CF = k x R / (R + h)", "Dgrid = Dground x CF"; the American
/// Surveyor: "Combined Factor = Elevation Factor x Scale Factor"). `h` must be an ellipsoidal
/// height: an orthometric elevation used in its place makes the result wrong by the geoid
/// separation over R, about 1.6 parts per million per 10 m. The radius is a mean value, so the
/// result carries an error of the order of the variation of the radius with latitude over R (a
/// few tenths of a part per million of h/R times 0.3 per cent; negligible at engineering heights).
pub fn combined_scale_factor(point_scale: f64, ellipsoidal_height_m: f64, radius_m: f64) -> Option<f64> {
    elevation_factor(ellipsoidal_height_m, radius_m).map(|ef| point_scale * ef).filter(|v| v.is_finite())
}

// ---------------------------------------------------------------------------------------------
// Site calibration: a local grid to a projected system

/// One control point seen in both systems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlPair {
    pub label: String,
    /// Local grid (x, y) in metres.
    pub local: (f64, f64),
    /// The same point in the target system: easting, northing in metres.
    pub target: (f64, f64),
}

/// The misfit of one control point after the fit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlResidual {
    pub label: String,
    /// Observed target minus fitted target, metres.
    pub d_east: f64,
    pub d_north: f64,
    pub length: f64,
}

/// A site calibration: the similarity transformation from a local grid to a projected system,
/// fitted by least squares from control points, with its residuals. Serialises with serde.
///
/// The model is GN7-2 section 4.7.3 (Similarity Transformation, EPSG method 9621):
/// `XT = XTO + XS M cos(theta) + YS M sin(theta)`, `YT = YTO - XS M sin(theta) + YS M cos(theta)`,
/// `theta` counter-clockwise positive (the bearing of the local Y axis relative to target north).
/// The note gives the model, not the fit; the least-squares solution is derived here. Put
/// `a = M cos(theta)` and `b = M sin(theta)`; then the model is linear in `(a, b, XTO, YTO)`. The sum
/// of squared residuals is minimised by the centroids, `XTO = mean(XT) - a mean(XS) - b mean(YS)`,
/// `YTO = mean(YT) + b mean(XS) - a mean(YS)`, and, with deviations `x, y` of the local points and
/// `u, v` of the target points from their centroids, `a = sum(x u + y v) / sum(x^2 + y^2)` and
/// `b = sum(y u - x v) / sum(x^2 + y^2)` (set the derivatives of the sum of squares to zero). The
/// tests check the result against the note's own worked example and that no small change of the
/// parameters lowers the sum of squares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteCalibration {
    pub name: String,
    /// The target system the control coordinates are in.
    pub target: CrsRef,
    /// Coordinates of the local origin in the target system (XTO, YTO), metres.
    pub origin_east: f64,
    pub origin_north: f64,
    /// M: multiply local coordinates by this to get target scale. Near 1; a value that differs from 1 by
    /// more than the grid-to-ground factor of the site deserves a look.
    pub scale: f64,
    /// theta, radians and degrees, counter-clockwise positive.
    pub rotation_rad: f64,
    pub rotation_deg: f64,
    pub residuals: Vec<ControlResidual>,
    /// Root mean square and largest of the residual lengths, metres.
    pub rms_m: f64,
    pub max_m: f64,
    pub points: usize,
    pub warnings: Vec<String>,
    /// When, as the caller gives it.
    pub when: Option<String>,
    pub method: String,
}

/// Why a calibration could not be fitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    /// A similarity transformation has four parameters; two points give four equations.
    TooFewPoints,
    /// All local points coincide, so no scale or rotation can be found.
    Degenerate,
    NotFinite,
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalibrationError::TooFewPoints => f.write_str("at least two control points are needed"),
            CalibrationError::Degenerate => f.write_str("the local control points coincide, so no scale or rotation can be found"),
            CalibrationError::NotFinite => f.write_str("a control coordinate is not a finite number"),
        }
    }
}

impl std::error::Error for CalibrationError {}

impl SiteCalibration {
    /// Fit by least squares. `target` is the system of the control coordinates (its code and name
    /// are recorded; a local grid is refused by the type of the caller's choice, not here).
    pub fn fit(name: &str, target: &ProjectCrs, pairs: &[ControlPair], when: Option<String>) -> Result<SiteCalibration, CalibrationError> {
        if pairs.len() < 2 {
            return Err(CalibrationError::TooFewPoints);
        }
        if !pairs.iter().all(|p| [p.local.0, p.local.1, p.target.0, p.target.1].iter().all(|v| v.is_finite())) {
            return Err(CalibrationError::NotFinite);
        }
        let n = pairs.len() as f64;
        let (mx, my) = (pairs.iter().map(|p| p.local.0).sum::<f64>() / n, pairs.iter().map(|p| p.local.1).sum::<f64>() / n);
        let (mu, mv) = (pairs.iter().map(|p| p.target.0).sum::<f64>() / n, pairs.iter().map(|p| p.target.1).sum::<f64>() / n);
        let (mut sxx, mut num_a, mut num_b) = (0.0, 0.0, 0.0);
        for p in pairs {
            let (x, y) = (p.local.0 - mx, p.local.1 - my);
            let (u, v) = (p.target.0 - mu, p.target.1 - mv);
            sxx += x * x + y * y;
            num_a += x * u + y * v;
            num_b += y * u - x * v;
        }
        if sxx <= 0.0 || !sxx.is_finite() {
            return Err(CalibrationError::Degenerate);
        }
        let (a, b) = (num_a / sxx, num_b / sxx);
        let (x0, y0) = (mu - a * mx - b * my, mv + b * mx - a * my);
        let scale = a.hypot(b);
        let theta = b.atan2(a);
        let mut cal = SiteCalibration {
            name: name.to_string(),
            target: CrsRef { epsg: target.epsg, name: target.name.clone() },
            origin_east: x0,
            origin_north: y0,
            scale,
            rotation_rad: theta,
            rotation_deg: theta.to_degrees(),
            residuals: Vec::new(),
            rms_m: 0.0,
            max_m: 0.0,
            points: pairs.len(),
            warnings: Vec::new(),
            when,
            method: "Least-squares similarity transformation (GN7-2 section 4.7.3, EPSG method 9621), four parameters".to_string(),
        };
        let mut sumsq = 0.0;
        for p in pairs {
            let (e, n_) = cal.to_target(p.local.0, p.local.1);
            let (de, dn) = (p.target.0 - e, p.target.1 - n_);
            let len = de.hypot(dn);
            sumsq += len * len;
            cal.max_m = cal.max_m.max(len);
            cal.residuals.push(ControlResidual { label: p.label.clone(), d_east: de, d_north: dn, length: len });
        }
        cal.rms_m = (sumsq / n).sqrt();
        if pairs.len() == 2 {
            cal.warnings.push("Two control points determine the four parameters exactly: the residuals are zero by construction and say nothing about the quality of the fit. Use at least three, and four or more to see a misfit.".to_string());
        }
        cal.warnings.push("The calibration is two-dimensional. Heights are not calibrated and are returned unchanged. It is valid among and near the control points; extrapolation beyond them grows the error of the rotation and scale.".to_string());
        if (scale - 1.0).abs() > 1e-3 {
            cal.warnings.push(format!("The fitted scale is {scale:.8}, {:+.0} parts per million from 1. Check the units and the control coordinates: a site grid at ground level differs from a projection grid by the combined factor, which is usually within a few hundred ppm.", (scale - 1.0) * 1e6));
        }
        Ok(cal)
    }

    /// Local grid to the target system, metres.
    pub fn to_target(&self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.rotation_rad.sin_cos();
        (self.origin_east + self.scale * (x * c + y * s), self.origin_north + self.scale * (-x * s + y * c))
    }

    /// Target system back to the local grid (GN7-2 4.7.3, "Reversibility": the exact reverse).
    pub fn to_local(&self, e: f64, n: f64) -> (f64, f64) {
        let (s, c) = self.rotation_rad.sin_cos();
        let (de, dn) = (e - self.origin_east, n - self.origin_north);
        ((de * c - dn * s) / self.scale, (de * s + dn * c) / self.scale)
    }

    /// The parameters as a similarity record: the origin of the local grid in the target system, M and theta.
    pub fn sentence(&self) -> String {
        format!(
            "Site calibration '{}' to {}: local origin at E {:.4} N {:.4}, scale {:.8}, rotation {:.6} degrees counter-clockwise; {} control points, residual RMS {:.4} m, largest {:.4} m.",
            self.name,
            self.target.name,
            self.origin_east,
            self.origin_north,
            self.scale,
            self.rotation_deg,
            self.points,
            self.rms_m,
            self.max_m
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crs::ProjectCrs;

    fn crs(code: u32) -> ProjectCrs {
        ProjectCrs::from_epsg(code).unwrap()
    }

    fn geo(code: u32) -> ProjectCrs {
        ProjectCrs::parse(&format!("EPSG:{code}")).unwrap()
    }

    fn close(a: f64, b: f64, tol: f64, what: &str) {
        assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
    }

    /// A point in Dodoma, Tanzania, in Arc 1960 / UTM zone 36S, built from a latitude and longitude
    /// on the Arc 1960 datum.
    fn dodoma() -> (f64, f64) {
        crs(21036).from_geographic(-6.17, 35.74).unwrap()
    }

    #[test]
    fn a_datum_change_lists_every_candidate_and_picks_none() {
        let (e, n) = dodoma();
        let ext = DataExtent::new((e - 2000.0, n - 2000.0), (e + 2000.0, n + 2000.0));
        let plans = plan_transform(&crs(21036), &crs(32736), Some(ext)).unwrap();
        // The four Arc 1960 transformations, each a candidate.
        let mut codes: Vec<u32> = plans.iter().map(|p| p.transformation_codes()[0]).collect();
        codes.sort_unstable();
        assert_eq!(codes, vec![1122, 1284, 1285, 3998]);
        // Those that cover Dodoma come first, with no warning, best accuracy first.
        assert_eq!(plans[0].transformation_codes(), vec![1285]);
        assert_eq!(plans[1].transformation_codes(), vec![1122]);
        assert!(plans[0].warnings.is_empty() && plans[1].warnings.is_empty(), "{:?}", plans[0].warnings);
        // The Kenya and Burundi ones are still offered, with a sentence saying why not.
        for p in &plans[2..] {
            assert!(p.warnings.iter().any(|w| w.contains("outside the area of use of transformation")), "{:?}", p.warnings);
        }
        assert_eq!(plans[0].accuracy_m, Some(15.0));
        assert_eq!(plans[1].accuracy_m, Some(35.0));
        // Steps: inverse projection, datum transformation with its code, forward projection.
        let kinds: Vec<StepKind> = plans[0].steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, vec![StepKind::InverseProjection, StepKind::DatumTransformation, StepKind::ForwardProjection]);
        assert_eq!(plans[0].steps[1].epsg, Some(1285));
        assert_eq!(plans[0].steps[1].source_url, "https://epsg.io/1285");
        assert_eq!(plans[0].kind, PlanKind::DatumTransformation);
    }

    #[test]
    fn data_outside_the_source_system_area_is_warned_about() {
        // A point of zone 36S moved 600 km east is in another zone entirely.
        let (e, n) = dodoma();
        let ext = DataExtent::new((e + 300_000.0, n), (e + 310_000.0, n + 1000.0));
        let plans = plan_transform(&crs(21036), &crs(32736), Some(ext)).unwrap();
        assert!(plans[0].warnings.iter().any(|w| w.contains("source system Arc 1960 / UTM zone 36S")), "{:?}", plans[0].warnings);
        // No extent: a note, not a silent pass.
        let plans = plan_transform(&crs(21036), &crs(32736), None).unwrap();
        assert!(plans[0].notes.iter().any(|w| w.contains("No data extent")));
    }

    #[test]
    fn the_shift_is_the_transformation_and_its_size_is_the_translation() {
        let (e, n) = dodoma();
        let p = plan_transform(&crs(21036), &crs(32736), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1122]).unwrap();
        let q = p.apply(e, n, 0.0).unwrap();
        let shift = (q.first - e).hypot(q.second - n);
        // The translation has length sqrt(160^2 + 6^2 + 302^2) = 341.7 m; a horizontal shift of that
        // order, and not more than the whole vector.
        assert!(shift > 100.0 && shift < 342.0, "{shift}");
        // Two transformations differ by about the difference of their translations (22.6 m between 1122 and 1285).
        let r = plan_transform(&crs(21036), &crs(32736), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1285]).unwrap().apply(e, n, 0.0).unwrap();
        let d = (q.first - r.first).hypot(q.second - r.second);
        assert!(d > 5.0 && d < 22.7, "{d}");
        // Heights come back unchanged.
        assert_eq!(p.apply(e, n, 1234.5).unwrap().height_m, 1234.5);
    }

    #[test]
    fn a_three_dimensional_round_trip_is_exact_and_the_two_dimensional_one_is_to_centimetres() {
        let (e, n) = dodoma();
        let plan = plan_transform(&crs(21036), &crs(32736), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1122]).unwrap();
        let back = plan.reversed();
        let (e2, n2, h2) = plan.apply_3d(e, n, 1100.0).unwrap();
        let (e3, n3, h3) = back.apply_3d(e2, n2, h2).unwrap();
        close(e3, e, 1e-5, "E");
        close(n3, n, 1e-5, "N");
        close(h3, 1100.0, 1e-5, "h");
        // Two-dimensional: the height out is dropped and the reverse is given the same input height,
        // so the round trip is off by the effect of the height difference (about 340 m of translation
        // times the height change, over the Earth radius).
        let q = plan.apply(e, n, 1100.0).unwrap();
        let r = back.apply(q.first, q.second, 1100.0).unwrap();
        let miss = (r.first - e).hypot(r.second - n);
        assert!(miss < 0.1, "{miss}");
        assert!(miss > 1e-4, "the 2D round trip is not exact, and the test knows it: {miss}");
    }

    #[test]
    fn the_effect_of_the_height_assumption_is_small_and_measured() {
        let (e, n) = dodoma();
        let plan = plan_transform(&crs(21036), &crs(32736), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1122]).unwrap();
        // Translation 341.7 m; the part of it across the local vertical moves the horizontal result by
        // (t_perp / R) per metre of height, at most 341.7 / 6.38e6 = 5.4e-5 m per metre.
        let per_km = plan.height_sensitivity(e, n, 0.0, 1000.0).unwrap();
        assert!(per_km > 0.0 && per_km < 0.0537, "{per_km}");
        let per_100 = plan.height_sensitivity(e, n, 0.0, 100.0).unwrap();
        close(per_100 * 10.0, per_km, 0.002, "linear in height");
        // A plan with no datum step has no such sensitivity.
        let utm = plan_transform(&crs(21036), &crs(21037), None).unwrap().remove(0);
        assert_eq!(utm.height_sensitivity(e, n, 0.0, 1000.0), Some(0.0));
    }

    #[test]
    fn zero_height_assumption_ignores_the_input_height() {
        let (e, n) = dodoma();
        let plan = plan_transform(&crs(21036), &crs(32736), None).unwrap().remove(0);
        let a = plan.clone().with_height_assumption(HeightAssumption::Zero).apply(e, n, 3000.0).unwrap();
        let b = plan.clone().with_height_assumption(HeightAssumption::Zero).apply(e, n, 0.0).unwrap();
        assert_eq!((a.first, a.second), (b.first, b.second));
        assert_eq!(a.height_m, 3000.0);
        let c = plan.apply(e, n, 3000.0).unwrap();
        assert!((c.first - a.first).hypot(c.second - a.second) > 1e-4);
    }

    #[test]
    fn same_datum_is_recognised_as_projection_change_only() {
        // WGS 84 / UTM 36S to WGS 84 / UTM 37S, and to geographic WGS 84.
        let (e, n) = crs(32736).from_geographic(-3.0, 35.9).unwrap();
        let plans = plan_transform(&crs(32736), &crs(32737), None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].kind, PlanKind::ProjectionOnly);
        assert!(plans[0].steps.iter().all(|s| s.kind != StepKind::DatumTransformation));
        assert_eq!(plans[0].accuracy_m, Some(0.0));
        let q = plans[0].apply(e, n, 10.0).unwrap();
        let (lat, lon) = crs(32737).to_geographic(q.first, q.second).unwrap();
        close(lat, -3.0, 1e-9, "lat");
        close(lon, 35.9, 1e-9, "lon");
        // Cape and Hartebeesthoek94 Lo systems on the same ellipsoid and datum: Lo27 to Lo29.
        let lo = plan_transform(&crs(2052), &crs(2053), None).unwrap();
        assert_eq!(lo[0].kind, PlanKind::ProjectionOnly);
        // Same code: identity.
        let id = plan_transform(&crs(32736), &crs(32736), None).unwrap();
        assert_eq!(id[0].kind, PlanKind::Identity);
        assert_eq!(id[0].apply(1.0, 2.0, 3.0).unwrap(), TransformedPoint { first: 1.0, second: 2.0, height_m: 3.0 });
    }

    #[test]
    fn geographic_systems_are_first_class() {
        // Arc 1960 / UTM 36S to WGS 84 latitude and longitude, latitude first.
        let (e, n) = dodoma();
        let plan = plan_transform(&crs(21036), &geo(4326), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1285]).unwrap();
        let q = plan.apply(e, n, 0.0).unwrap();
        assert!(q.first < -5.9 && q.first > -6.4 && q.second > 35.5 && q.second < 36.0, "{q:?}");
        // The shift in latitude and longitude is the geographic form of the same operation.
        let back = plan.reversed();
        assert_eq!(back.from.epsg, Some(4326));
        let (e2, n2, _) = back.apply_3d(q.first, q.second, plan.apply_3d(e, n, 0.0).unwrap().2).unwrap();
        close(e2, e, 1e-5, "E");
        close(n2, n, 1e-5, "N");
        // Latitude beyond 90 is refused.
        assert_eq!(back.apply(95.0, 0.0, 0.0), Err(TransformError::OutsideProjection));
        assert_eq!(back.apply(f64::NAN, 0.0, 0.0), Err(TransformError::NotFinite));
        // Geographic to geographic.
        let g = plan_transform(&geo(4210), &geo(4326), Some(DataExtent::new((-6.3, 35.6), (-6.0, 35.9)))).unwrap();
        assert_eq!(g[0].transformation_codes(), vec![1285]);
    }

    #[test]
    fn two_datum_steps_go_through_wgs_84_and_sum_their_accuracy() {
        // Arc 1960 to Adindan (Tanzania data is not in Adindan's area, so there are warnings), by 1285 + reversed 1100.
        let plans = plan_transform(&crs(21036), &crs(20136), None).unwrap();
        assert_eq!(plans.len(), 4 * 3);
        let p = plans.iter().find(|p| p.transformation_codes() == vec![1285, 1100]).unwrap();
        assert_eq!(p.accuracy_m, Some(15.0 + 9.0));
        let datum_steps: Vec<&StepRecord> = p.steps.iter().filter(|s| s.kind == StepKind::DatumTransformation).collect();
        assert_eq!(datum_steps.len(), 2);
        assert!(!datum_steps[0].reversed && datum_steps[1].reversed);
        // Round trip through both is exact in 3D.
        let (e, n) = crs(21036).from_geographic(5.0, 34.0).unwrap();
        let (e2, n2, h2) = p.apply_3d(e, n, 500.0).unwrap();
        let (e3, n3, h3) = p.reversed().apply_3d(e2, n2, h2).unwrap();
        close(e3, e, 1e-5, "E");
        close(n3, n, 1e-5, "N");
        close(h3, 500.0, 1e-5, "h");
    }

    #[test]
    fn no_path_is_stated_for_named_only_local_and_unconnected_systems() {
        let named = ProjectCrs::named_only(Some(2039), "Israeli Grid");
        assert!(matches!(plan_transform(&named, &crs(32736), None), Err(NoPath::NamedOnly { .. })));
        assert!(matches!(plan_transform(&crs(32736), &named, None), Err(NoPath::NamedOnly { .. })));
        let local = ProjectCrs::local_grid("Site A");
        let err = plan_transform(&local, &crs(32736), None).unwrap_err();
        assert!(matches!(err, NoPath::LocalGrid { .. }));
        assert!(err.to_string().contains("site calibration"));
        // No code at all.
        assert!(matches!(plan_transform(&ProjectCrs::named_only(None, "Unknown"), &crs(32736), None), Err(NoPath::NamedOnly { .. })));
        // Hartebeesthoek94 has no recorded transformation to WGS 84: no path, and the sentence says nothing is invented.
        let err = plan_transform(&crs(2053), &crs(32736), None).unwrap_err();
        assert!(matches!(err, NoPath::NoTransformation { .. }));
        assert!(err.to_string().contains("Nothing is invented"));
        // Arc 1950 to Arc 1960: both connect to WGS 84, so a path exists.
        assert!(plan_transform(&crs(20936), &crs(21036), None).is_ok());
    }

    #[test]
    fn warnings_for_the_target_area_and_superseded_transformations() {
        // Arc 1950 in Zimbabwe: 1121 is superseded by 6906 and says so.
        let (e, n) = crs(20936).from_geographic(-17.8, 31.0).unwrap();
        let ext = DataExtent::new((e - 100.0, n - 100.0), (e + 100.0, n + 100.0));
        let plans = plan_transform(&crs(20936), &crs(32736), Some(ext)).unwrap();
        let p1121 = plans.iter().find(|p| p.transformation_codes() == vec![1121]).unwrap();
        assert!(p1121.warnings.iter().any(|w| w.contains("replaced by EPSG:6906")), "{:?}", p1121.warnings);
        let p6906 = plans.iter().find(|p| p.transformation_codes() == vec![6906]).unwrap();
        assert!(p6906.warnings.is_empty(), "{:?}", p6906.warnings);
        // 36S Arc 1950 area includes Zimbabwe east of 30E; the best-listed plans cover it.
        assert!(plans[0].warnings.is_empty());
    }

    /// The bearing and distance change the plan reports is what a measured baseline shows.
    #[test]
    fn bearing_and_scale_effects_match_a_transformed_baseline() {
        // Zone change on the same datum: 36S to 37S, with the point near the zone boundary, where the
        // convergence of the two grids differs a lot.
        let (e, n) = crs(32736).from_geographic(-3.0, 35.9).unwrap();
        let plan = plan_transform(&crs(32736), &crs(32737), None).unwrap().remove(0);
        let fx = plan.point_effects(e, n, 0.0).unwrap();
        let (ks, kt) = (fx.source_scale.unwrap(), fx.target_scale.unwrap());
        assert!(ks > 0.9996 && kt > 0.9996, "{ks} {kt}");
        for bearing in [0.0, 37.0, 90.0, 215.0] {
            let m = plan.baseline_effect(e, n, 0.0, bearing, 200.0).unwrap();
            // A baseline of 200 m: the bearing change and the length ratio of the plan agree with the measured one.
            close(m.grid_bearing_change_deg, fx.grid_bearing_change_deg.unwrap(), 2e-4, "bearing change (degrees)");
            close(m.length_ratio, fx.scale_ratio.unwrap(), 3e-6, "length ratio");
        }
        // The bearing change is the difference of the two convergences, about 0.3 degrees here (+0.16 and -0.15).
        assert!(fx.grid_bearing_change_deg.unwrap().abs() > 0.2, "{fx:?}");
        assert!(fx.sentence().contains("degrees"));
    }

    /// With a datum step as well: the measured change differs from the grid-only prediction only by
    /// the (small) rotation and scale effect of the datum shift, and the test states how small.
    #[test]
    fn the_datum_step_barely_changes_bearings_and_distances_in_the_measured_baseline() {
        let (e, n) = dodoma();
        let plan = plan_transform(&crs(21036), &crs(32736), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1285]).unwrap();
        let fx = plan.point_effects(e, n, 0.0).unwrap();
        for bearing in [0.0, 45.0, 135.0, 300.0] {
            let m = plan.baseline_effect(e, n, 0.0, bearing, 1000.0).unwrap();
            let dbear = (m.grid_bearing_change_deg - fx.grid_bearing_change_deg.unwrap()).abs();
            let dlen = (m.length_ratio - fx.scale_ratio.unwrap()).abs();
            // Measured at this point for 1285 and 1122: 0.29 to 0.31 arc-seconds and 1.2 to 2.2 parts per
            // million (the datum step turns lines by about 0.3 arc-second and changes lengths by a few ppm,
            // which the grid-only prediction does not contain); bounds set with a margin. See the README.
            assert!(dbear < 1.5e-4, "bearing {bearing}: {dbear} degrees"); // 0.54 arc-second
            assert!(dlen < 3.0e-6, "bearing {bearing}: {dlen}");
        }
    }

    #[test]
    fn effects_are_not_invented_for_geographic_or_south_orientated_ends() {
        let plan = plan_transform(&crs(21036), &geo(4326), None).unwrap().remove(0);
        let (e, n) = dodoma();
        let fx = plan.point_effects(e, n, 0.0).unwrap();
        assert!(fx.scale_ratio.is_none() && fx.grid_bearing_change_deg.is_none());
        assert!(fx.source_scale.is_some() && fx.target_scale.is_none());
        assert!(fx.notes.iter().any(|n| n.contains("geographic")));
        assert!(plan.baseline_effect(e, n, 0.0, 0.0, 100.0).is_none());
        let lo = plan_transform(&crs(2052), &crs(2053), None).unwrap().remove(0);
        let fx = lo.point_effects(40_000.0, 3_000_000.0, 0.0);
        assert!(fx.is_some_and(|f| f.grid_bearing_change_deg.is_none()));
    }

    #[test]
    fn batch_counts_failures_and_the_log_keeps_the_plan() {
        let (e, n) = dodoma();
        let plan = plan_transform(&crs(21036), &crs(32736), Some(DataExtent::new((e - 10.0, n - 10.0), (e + 10.0, n + 10.0)))).unwrap().remove(0);
        let batch = plan.apply_batch(&[(e, n, 1000.0), (e + 5.0, n, 1001.0), (f64::NAN, n, 0.0), (e, n, 5.0)]);
        assert_eq!((batch.transformed, batch.failed), (3, 1));
        assert!(batch.points[2].is_none());
        let log = TransformLog::from_batch("Survey points of the Dodoma bypass", &plan, Some("2026-11-16T09:30:00Z".to_string()), &batch);
        assert_eq!((log.points_in, log.points_transformed, log.points_failed), (4, 3, 1));
        assert_eq!(log.from.epsg, Some(21036));
        assert_eq!(log.to.epsg, Some(32736));
        assert_eq!(log.accuracy_m, Some(15.0));
        assert!(log.steps.iter().any(|s| s.epsg == Some(1285) && s.accuracy_m == Some(15.0)));
        assert!(log.height_treatment.contains("unchanged"));
        assert!(log.summary().contains("EPSG:1285") && log.summary().contains("3 points transformed, 1 failed") && log.summary().contains("15 m"), "{}", log.summary());
        // Serde round trip.
        let json = serde_json::to_string(&log).unwrap();
        let back: TransformLog = serde_json::from_str(&json).unwrap();
        assert_eq!(back, log);
        // A projection-only log says so.
        let p2 = plan_transform(&crs(32736), &crs(32737), None).unwrap().remove(0);
        let l2 = TransformLog::with_counts("x", &p2, None, 10, 10, 0);
        assert!(l2.summary().contains("projection change only") && !l2.summary().contains("stated accuracy"), "{}", l2.summary());
        assert_eq!(l2.when, None);
    }

    #[test]
    fn reversed_plan_has_the_steps_in_the_other_order() {
        let plan = plan_transform(&crs(21036), &crs(20136), None).unwrap().into_iter().find(|p| p.transformation_codes() == vec![1285, 1100]).unwrap();
        let rev = plan.reversed();
        assert_eq!(rev.transformation_codes(), vec![1100, 1285]);
        assert_eq!(rev.from.epsg, Some(20136));
        assert_eq!(rev.to.epsg, Some(21036));
        assert_eq!(rev.accuracy_m, plan.accuracy_m);
    }

    // ----- combined scale factor

    /// The worked example of the surveybible page: R = 20,906,000 ft, 2,500 ft above the ellipsoid, k = 0.99990.
    #[test]
    fn combined_scale_factor_reproduces_the_published_example() {
        let ef = elevation_factor(2500.0, 20_906_000.0).unwrap();
        close(ef, 0.999880, 0.5e-6, "elevation factor");
        let cf = combined_scale_factor(0.99990, 2500.0, 20_906_000.0).unwrap();
        close(cf, 0.999780, 0.5e-6, "combined factor");
        close(1000.000 * cf, 999.780, 0.0005, "grid distance");
        // The American Surveyor example: height 293 ft gives 0.999986.
        close(elevation_factor(293.0, 20_906_000.0).unwrap(), 0.999986, 0.5e-6, "amerisurv");
        assert!(elevation_factor(-7_000_000.0, MEAN_EARTH_RADIUS_M).is_none());
        assert!(combined_scale_factor(f64::NAN, 0.0, MEAN_EARTH_RADIUS_M).is_none());
    }

    #[test]
    fn combined_scale_factor_at_a_projected_point() {
        let (e, n) = dodoma();
        let (k, _) = crs(21036).scale_and_convergence_at(e, n).unwrap();
        let cf = combined_scale_factor(k, 1120.0, MEAN_EARTH_RADIUS_M).unwrap();
        close(cf, k * (1.0 - 1120.0 / MEAN_EARTH_RADIUS_M), 1e-6, "first order in h/R");
    }

    // ----- site calibration

    fn gn7_2_similarity() -> SiteCalibration {
        // GN7-2 4.7.3: ED50 / UTM zone 31N to ETRS89 / UTM zone 31N similarity, XTO -129.549, YTO -208.185,
        // M 1.00000155, theta 1.56504 arc-seconds (0.000007588 rad). The calibration is built from control
        // pairs generated with those parameters.
        let (x0, y0, m, th) = (-129.549, -208.185, 1.000_001_55, 1.56504 * std::f64::consts::PI / 648_000.0);
        let (s, c) = th.sin_cos();
        let pairs: Vec<ControlPair> = [(300_000.0, 4_500_000.0), (310_000.0, 4_520_000.0), (290_000.0, 4_480_000.0), (305_000.0, 4_470_000.0)]
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| ControlPair { label: format!("C{i}"), local: (x, y), target: (x0 + m * (x * c + y * s), y0 + m * (-x * s + y * c)) })
            .collect();
        SiteCalibration::fit("ED50 to ETRS89", &crs(32631), &pairs, None).unwrap()
    }

    /// GN7-2 4.7.3 worked example: the forward and reverse computation of the similarity transformation.
    #[test]
    fn gn7_2_4_7_3_similarity_example_forward_and_reverse() {
        let cal = SiteCalibration {
            name: "GN7-2 4.7.3".to_string(),
            target: CrsRef { epsg: None, name: "ETRS89 / UTM zone 31N".to_string() },
            origin_east: -129.549,
            origin_north: -208.185,
            scale: 1.000_001_55,
            rotation_rad: 1.56504 * std::f64::consts::PI / 648_000.0,
            rotation_deg: 1.56504 / 3600.0,
            residuals: vec![],
            rms_m: 0.0,
            max_m: 0.0,
            points: 0,
            warnings: vec![],
            when: None,
            method: String::new(),
        };
        let (e, n) = cal.to_target(300_000.0, 4_500_000.0);
        close(e, 299_905.060, 0.002, "E");
        close(n, 4_499_796.515, 0.002, "N");
        let (x, y) = cal.to_local(299_905.060, 4_499_796.515);
        close(x, 300_000.0, 0.002, "x");
        close(y, 4_500_000.0, 0.002, "y");
    }

    #[test]
    fn a_fit_recovers_exact_parameters_with_zero_residuals() {
        let cal = gn7_2_similarity();
        close(cal.origin_east, -129.549, 1e-4, "XTO");
        close(cal.origin_north, -208.185, 1e-4, "YTO");
        close(cal.scale, 1.000_001_55, 1e-12, "M");
        close(cal.rotation_deg * 3600.0, 1.56504, 1e-6, "theta arc-seconds");
        assert!(cal.max_m < 1e-6, "{}", cal.max_m);
        assert_eq!(cal.points, 4);
        let (x, y) = cal.to_local(299_905.060, 4_499_796.515);
        close(x, 300_000.0, 0.002, "x");
        close(y, 4_500_000.0, 0.002, "y");
    }

    #[test]
    fn a_fit_with_noise_is_least_squares() {
        let target = crs(32736);
        let truth = |x: f64, y: f64| (500_100.0 + 0.9997 * (x * 0.8 + y * 0.6), 9_800_200.0 + 0.9997 * (-x * 0.6 + y * 0.8));
        let noise = [(0.012, -0.008), (-0.010, 0.015), (0.004, 0.003), (-0.009, -0.011), (0.006, 0.002)];
        let locals = [(0.0, 0.0), (400.0, 20.0), (380.0, 310.0), (10.0, 300.0), (200.0, 150.0)];
        let pairs: Vec<ControlPair> = locals
            .iter()
            .zip(noise.iter())
            .enumerate()
            .map(|(i, (&(x, y), &(dx, dy)))| {
                let (e, n) = truth(x, y);
                ControlPair { label: format!("P{i}"), local: (x, y), target: (e + dx, n + dy) }
            })
            .collect();
        let cal = SiteCalibration::fit("Site", &target, &pairs, Some("2026-11-16".to_string())).unwrap();
        // Close to the truth, residuals of the size of the noise.
        close(cal.scale, 0.9997, 5e-5, "scale");
        assert!(cal.rms_m > 0.001 && cal.rms_m < 0.02, "{}", cal.rms_m);
        // Residuals sum to zero (the normal equations for the origin).
        let (se, sn): (f64, f64) = cal.residuals.iter().fold((0.0, 0.0), |a, r| (a.0 + r.d_east, a.1 + r.d_north));
        close(se, 0.0, 1e-7, "sum de");
        close(sn, 0.0, 1e-7, "sum dn");
        // No nearby parameter set has a smaller sum of squares.
        let ssq = |x0: f64, y0: f64, m: f64, th: f64| -> f64 {
            let (s, c) = th.sin_cos();
            pairs
                .iter()
                .map(|p| {
                    let (e, n) = (x0 + m * (p.local.0 * c + p.local.1 * s), y0 + m * (-p.local.0 * s + p.local.1 * c));
                    (p.target.0 - e).powi(2) + (p.target.1 - n).powi(2)
                })
                .sum()
        };
        let best = ssq(cal.origin_east, cal.origin_north, cal.scale, cal.rotation_rad);
        close(best, cal.rms_m * cal.rms_m * 5.0, 1e-9, "ssq consistent with rms");
        for (dx, dy, dm, dt) in [(1e-3, 0.0, 0.0, 0.0), (0.0, -1e-3, 0.0, 0.0), (0.0, 0.0, 1e-6, 0.0), (0.0, 0.0, -1e-6, 0.0), (0.0, 0.0, 0.0, 1e-6), (0.0, 0.0, 0.0, -1e-6), (1e-3, 1e-3, 1e-6, -1e-6)] {
            assert!(ssq(cal.origin_east + dx, cal.origin_north + dy, cal.scale + dm, cal.rotation_rad + dt) > best, "a nearby set is better: {dx} {dy} {dm} {dt}");
        }
        assert_eq!(cal.when.as_deref(), Some("2026-11-16"));
        // Serde.
        let back: SiteCalibration = serde_json::from_str(&serde_json::to_string(&cal).unwrap()).unwrap();
        // serde_json without its float_roundtrip feature may differ in the last bit of a float.
        assert_eq!((&back.name, &back.target, back.points, &back.when), (&cal.name, &cal.target, cal.points, &cal.when));
        close(back.scale, cal.scale, 1e-14, "scale after serde");
        close(back.origin_east, cal.origin_east, 1e-8, "origin after serde");
        assert_eq!(back.residuals.len(), cal.residuals.len());
        assert!(cal.sentence().contains("5 control points"));
    }

    #[test]
    fn calibration_refuses_what_it_cannot_fit_and_warns_about_two_points() {
        let t = crs(32736);
        let p = |x, y, e, n| ControlPair { label: "p".into(), local: (x, y), target: (e, n) };
        assert_eq!(SiteCalibration::fit("s", &t, &[p(0.0, 0.0, 1.0, 1.0)], None), Err(CalibrationError::TooFewPoints));
        assert_eq!(SiteCalibration::fit("s", &t, &[p(1.0, 1.0, 1.0, 1.0), p(1.0, 1.0, 2.0, 2.0)], None), Err(CalibrationError::Degenerate));
        assert_eq!(SiteCalibration::fit("s", &t, &[p(f64::NAN, 1.0, 1.0, 1.0), p(1.0, 1.0, 2.0, 2.0)], None), Err(CalibrationError::NotFinite));
        let two = SiteCalibration::fit("s", &t, &[p(0.0, 0.0, 500_000.0, 9_800_000.0), p(100.0, 0.0, 500_100.0, 9_800_000.0)], None).unwrap();
        assert!(two.warnings.iter().any(|w| w.contains("Two control points")));
        close(two.scale, 1.0, 1e-12, "scale");
        close(two.rotation_deg, 0.0, 1e-12, "rotation");
        // A silly scale is flagged.
        let odd = SiteCalibration::fit("s", &t, &[p(0.0, 0.0, 0.0, 0.0), p(100.0, 0.0, 100.5, 0.0), p(0.0, 100.0, 0.0, 100.5)], None).unwrap();
        assert!(odd.warnings.iter().any(|w| w.contains("parts per million")), "{:?}", odd.warnings);
    }
}
