// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! LandXML: the civil content of a whole-file import that the geometry readers do not hold, kept as
//! the file states it. Plain data in metres (stations, offsets and levels take the file's linear
//! unit; percentages and counts are as written).
//!
//! What lives here, all of it reachable from [`LandXmlFile`] and from the per-alignment view
//! [`LandXmlAlignmentView`]:
//!
//! - [`LandXmlAlignmentInfo`]: the `Alignment` attributes (`desc`, `state`, `oID`, the stated `length`
//!   and `staStart`) and the `CoordGeom` **exactly as the file states it** (the points of every
//!   `Start`, `Center`, `PI`, `End`), next to the engine [`Alignment`] that is built from it;
//! - [`LandXmlDesignProfile`]: every `ProfAlign` as written (`PVI`, `ParaCurve`, `UnsymParaCurve`,
//!   `CircCurve`, in order, with their lengths and radii). The engine [`Profile`] cannot hold two
//!   PVIs at one station (a vertical step); such a profile is left off the [`Alignment`] with a
//!   warning and is still here in full, and [`LandXmlDesignProfile::segments`] cuts it at its steps;
//! - [`LandXmlSuperelevationTable`]: every `Superelevation` element as written: `staStart`, `staEnd`,
//!   each critical station (begin runout, begin runoff, full super, runoff, start of runout, end of
//!   runout) in file order, every `FullSuperelev` (percent, **as written, with its sign**) and
//!   `AdverseSE`. Nothing is reduced to a single rate and nothing is interpolated or assigned to a
//!   side; the direction of the horizontal curve there is added from the geometry as `turn`;
//! - [`LandXmlCrossSectGroup`] and [`LandXmlCrossSectDetail`]: the attributes of `CrossSects` and of
//!   each `CrossSect` beyond `sta`, `name` and `desc` (skew, areas, volumes, section type);
//! - [`LandXmlObjectFeatures`]: every `Feature` element (with its `Property`, `DocFileRef` and
//!   nested `Feature`) as a generic property bag, with the object it hangs on;
//! - [`LandXmlFieldNote`]: every `FieldNote` (Civil 3D writes the local-to-grid transformation of the
//!   drawing there);
//! - [`LandXmlSourceData::point_files`](crate::LandXmlSourceData): the external point files a surface
//!   was built from (`SourceData/PointFiles/PointFile`).
//!
//! The superelevation `turn` and the sign convention are derived from the horizontal geometry of the
//! file itself; see [`crate::parse_landxml`] for how and when a sign is read as a direction.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::landxml::{children, LocalName};
use crate::landxml_ext::{LandXmlCoordGeom, LandXmlCrossSection, LandXmlCrossSectSurf, LandXmlFile, LandXmlProfSurf, LandXmlSurface, Rd};
use crate::{Alignment, CriticalStation, Profile, ProfileError, Pvi, Superelevation, Turn, VerticalCurve};

// ----------------------------------------------------------------------------------------------
// Alignment attributes and the geometry as stated
// ----------------------------------------------------------------------------------------------

/// What the engine [`Alignment`] was built from: the geometry exactly as stated, so that it can be
/// written back and compared. `fingerprint` says whether the engine alignment still matches.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlGeometryFingerprint {
    pub elements: usize,
    pub length_m: f64,
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
}

impl LandXmlGeometryFingerprint {
    /// The fingerprint of an engine alignment (`None` when it has no elements).
    pub fn of(alignment: &Alignment) -> Option<Self> {
        let first = alignment.elements.first()?;
        let last = alignment.elements.last()?;
        let (ex, ey, _) = last.end();
        Some(LandXmlGeometryFingerprint { elements: alignment.elements.len(), length_m: alignment.length(), start_x: first.x, start_y: first.y, end_x: ex, end_y: ey })
    }
}

/// `Alignments/Alignment`: its attributes and its geometry as stated.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlAlignmentInfo {
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub oid: Option<String>,
    /// `Alignment@length`, as stated (metres). The geometry's own length is [`Alignment::length`].
    pub declared_length_m: Option<f64>,
    /// `Alignment@staStart`, as stated (metres).
    pub sta_start_m: Option<f64>,
    /// False when the alignment could not be built (see the warnings); only this record is left.
    pub imported: bool,
    /// Every `CoordGeom` of the alignment: the points as the file states them (`Curve@radius` and
    /// `length` as stated; the redundant direction and angle attributes are not kept).
    pub coord_geoms: Vec<LandXmlCoordGeom>,
    /// The engine alignment as it was built from `coord_geoms`; a writer compares it with the
    /// alignment it is given to see whether `coord_geoms` is still current.
    pub fingerprint: Option<LandXmlGeometryFingerprint>,
}

// ----------------------------------------------------------------------------------------------
// Design profile
// ----------------------------------------------------------------------------------------------

/// The kind of one `ProfAlign` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandXmlProfEntryKind {
    Pvi,
    ParaCurve,
    UnsymParaCurve,
    CircCurve,
}

/// One child of `ProfAlign`: the text `station elevation` and the lengths and radius it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandXmlProfEntry {
    pub kind: LandXmlProfEntryKind,
    pub station_m: f64,
    pub elevation_m: f64,
    /// `ParaCurve@length`, `CircCurve@length`.
    pub length_m: Option<f64>,
    /// `UnsymParaCurve@lengthIn`.
    pub length_in_m: Option<f64>,
    /// `UnsymParaCurve@lengthOut`.
    pub length_out_m: Option<f64>,
    /// `CircCurve@radius`.
    pub radius_m: Option<f64>,
    pub desc: Option<String>,
}

/// `Profile/ProfAlign` as written: the design profile (the proposed centreline levels).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlDesignProfile {
    /// `name` of the `Alignment` it belongs to.
    pub alignment: String,
    /// The enclosing `Profile`: `name`, `desc`, `state`, `staStart`.
    pub profile: Option<String>,
    pub profile_desc: Option<String>,
    pub profile_state: Option<String>,
    pub profile_sta_start_m: Option<f64>,
    pub name: String,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub entries: Vec<LandXmlProfEntry>,
}

impl LandXmlProfEntry {
    fn pvi(&self, index: usize) -> Result<Pvi, ProfileError> {
        let missing = |message: &str| ProfileError::BadPvi { index, message: message.to_string() };
        let curve = match self.kind {
            LandXmlProfEntryKind::Pvi => VerticalCurve::None,
            LandXmlProfEntryKind::ParaCurve => VerticalCurve::Parabola { length_m: self.length_m.ok_or_else(|| missing("a ParaCurve has no length"))? },
            LandXmlProfEntryKind::UnsymParaCurve => VerticalCurve::AsymmetricParabola {
                length_in_m: self.length_in_m.ok_or_else(|| missing("an UnsymParaCurve has no lengthIn"))?,
                length_out_m: self.length_out_m.ok_or_else(|| missing("an UnsymParaCurve has no lengthOut"))?,
            },
            LandXmlProfEntryKind::CircCurve => VerticalCurve::Circular { radius_m: self.radius_m.ok_or_else(|| missing("a CircCurve has no radius"))?.abs() },
        };
        Ok(Pvi { station_m: self.station_m, elevation_m: self.elevation_m, curve })
    }
}

impl LandXmlDesignProfile {
    /// The number of `PVI`, `ParaCurve`, `UnsymParaCurve` and `CircCurve` entries.
    pub fn counts(&self) -> [usize; 4] {
        let n = |k| self.entries.iter().filter(|e| e.kind == k).count();
        [n(LandXmlProfEntryKind::Pvi), n(LandXmlProfEntryKind::ParaCurve), n(LandXmlProfEntryKind::UnsymParaCurve), n(LandXmlProfEntryKind::CircCurve)]
    }

    /// First and last entry as `(station, elevation)`.
    pub fn ends(&self) -> Option<((f64, f64), (f64, f64))> {
        let (a, b) = (self.entries.first()?, self.entries.last()?);
        Some(((a.station_m, a.elevation_m), (b.station_m, b.elevation_m)))
    }

    /// Where two consecutive entries sit at one station: `(station, level_before, level_after)`. The
    /// engine [`Profile`] has one level at a station, so a profile with a step cannot be one.
    pub fn steps(&self) -> Vec<(f64, f64, f64)> {
        self.entries.windows(2).filter(|w| w[0].station_m == w[1].station_m).map(|w| (w[0].station_m, w[0].elevation_m, w[1].elevation_m)).collect()
    }

    /// The whole profile as an engine [`Profile`], when it has no step and the engine accepts it.
    pub fn to_profile(&self) -> Result<Profile, ProfileError> {
        let pvis = self.entries.iter().enumerate().map(|(i, e)| e.pvi(i)).collect::<Result<Vec<_>, _>>()?;
        Profile::new(&self.name, pvis)
    }

    /// The profile cut at each of its [`steps`](Self::steps) into engine [`Profile`]s: each piece
    /// ends at the level before the step and the next begins at the level after it, so the steps are
    /// the gaps between the pieces. A profile with no step is one piece. Fails when a piece is not a
    /// valid profile (for example a vertical curve sits on the step itself). A step at the very
    /// start or end of the profile leaves a piece of one PVI, which is a point and not a profile; it
    /// is left out here and [`steps`](Self::steps) still lists the step.
    pub fn segments(&self) -> Result<Vec<Profile>, ProfileError> {
        let mut pieces: Vec<Vec<Pvi>> = vec![Vec::new()];
        for (i, e) in self.entries.iter().enumerate() {
            if i > 0 && self.entries[i - 1].station_m == e.station_m {
                pieces.push(Vec::new());
            }
            pieces.last_mut().expect("one piece at least").push(e.pvi(i)?);
        }
        let many = pieces.len() > 1;
        pieces
            .into_iter()
            .enumerate()
            .filter(|(_, p)| !many || p.len() > 1)
            .map(|(i, p)| Profile::new(&if many { format!("{} (part {})", self.name, i + 1) } else { self.name.clone() }, p))
            .collect()
    }
}

// ----------------------------------------------------------------------------------------------
// Superelevation
// ----------------------------------------------------------------------------------------------

/// One critical station of a `Superelevation` element.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LandXmlSuperStation {
    pub kind: CriticalStation,
    pub station_m: f64,
}

/// The LandXML element name of a critical station.
pub fn critical_station_element(kind: CriticalStation) -> &'static str {
    match kind {
        CriticalStation::BeginRunout => "BeginRunoutSta",
        CriticalStation::BeginRunoff => "BeginRunoffSta",
        CriticalStation::FullSuper => "FullSuperSta",
        CriticalStation::Runoff => "RunoffSta",
        CriticalStation::StartOfRunout => "StartofRunoutSta",
        CriticalStation::EndOfRunout => "EndofRunoutSta",
    }
}

pub(crate) const SUPER_STATIONS: [CriticalStation; 6] = [
    CriticalStation::BeginRunout,
    CriticalStation::BeginRunoff,
    CriticalStation::FullSuper,
    CriticalStation::Runoff,
    CriticalStation::StartOfRunout,
    CriticalStation::EndOfRunout,
];

/// One `Superelevation` element as the file writes it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSuperelevationRecord {
    /// `staStart` and `staEnd` of the element (metres), as stated.
    pub sta_start_m: Option<f64>,
    pub sta_end_m: Option<f64>,
    /// The critical stations in the order the file gives them. Stations are internal stations, by
    /// the schema's `station` type, and may lie outside the alignment.
    pub stations: Vec<LandXmlSuperStation>,
    /// Every `FullSuperelev`, percent, as written. The schema defines no sign; see [`Self::turn`].
    pub full_super_rates_pct: Vec<f64>,
    /// Every `AdverseSE` text, as written.
    pub adverse: Vec<String>,
    /// Which way the horizontal alignment turns midway through the stretch of full superelevation
    /// (or, with no such stations, between `staStart` and `staEnd`), from the geometry. `None` when
    /// the alignment is straight there, off the alignment, or not imported.
    pub turn: Option<Turn>,
}

impl LandXmlSuperelevationRecord {
    /// The first station of a kind.
    pub fn station(&self, kind: CriticalStation) -> Option<f64> {
        self.stations.iter().find(|s| s.kind == kind).map(|s| s.station_m)
    }

    /// The one rate, when there is exactly one `FullSuperelev`.
    pub fn rate_pct(&self) -> Option<f64> {
        match self.full_super_rates_pct.as_slice() {
            [r] => Some(*r),
            _ => None,
        }
    }

    /// True when the element has no station and no rate (only `staStart` and `staEnd`, or nothing).
    pub fn is_empty(&self) -> bool {
        self.stations.is_empty() && self.full_super_rates_pct.is_empty()
    }

    /// True when the stations, taken in the order of [`SUPER_STATIONS`]'s names (begin runout ...
    /// end of runout), never decrease. Civil 3D writes a curve that the alignment runs through
    /// backwards with its stations in descending order.
    pub fn stations_ascending(&self) -> bool {
        let mut ordered: Vec<f64> = SUPER_STATIONS.iter().filter_map(|k| self.station(*k)).collect();
        let before = ordered.clone();
        ordered.sort_by(f64::total_cmp);
        ordered == before
    }

    /// The middle of the stretch of full superelevation, or of `staStart`..`staEnd`.
    pub(crate) fn middle(&self) -> Option<f64> {
        let full: Vec<f64> = self.stations.iter().filter(|s| matches!(s.kind, CriticalStation::FullSuper | CriticalStation::Runoff)).map(|s| s.station_m).collect();
        match (full.first(), full.last(), self.sta_start_m, self.sta_end_m) {
            (Some(a), Some(b), _, _) => Some((a + b) / 2.0),
            (_, _, Some(a), Some(b)) => Some((a + b) / 2.0),
            _ => None,
        }
    }
}

/// One row of the flattened table: a critical station with the rate that holds there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LandXmlSuperelevationRow {
    pub record: usize,
    pub kind: CriticalStation,
    pub station_m: f64,
    /// The `FullSuperelev` of the record, only on `FullSuper` and `Runoff` rows (between them the
    /// surface is at full superelevation). The file states no rate at the other stations.
    pub full_rate_pct: Option<f64>,
}

/// All `Superelevation` elements of one alignment, as written.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlSuperelevationTable {
    pub alignment: String,
    pub records: Vec<LandXmlSuperelevationRecord>,
    /// The engine superelevation that was derived from these records when the file was read (the
    /// one on the [`Alignment`]), so a writer can tell whether it was edited since.
    pub derived: Option<Superelevation>,
}

impl LandXmlSuperelevationTable {
    /// Every critical station of every record, in file order, with the full rate on the rows where
    /// the schema's names make it apply.
    pub fn rows(&self) -> Vec<LandXmlSuperelevationRow> {
        let mut out = Vec::new();
        for (i, r) in self.records.iter().enumerate() {
            for s in &r.stations {
                let full = matches!(s.kind, CriticalStation::FullSuper | CriticalStation::Runoff);
                out.push(LandXmlSuperelevationRow { record: i, kind: s.kind, station_m: s.station_m, full_rate_pct: if full { r.rate_pct() } else { None } });
            }
        }
        out
    }

    /// Records that state at least one station or rate.
    pub fn non_empty(&self) -> usize {
        self.records.iter().filter(|r| !r.is_empty()).count()
    }
}

// ----------------------------------------------------------------------------------------------
// Cross-sections: group and per-section attributes
// ----------------------------------------------------------------------------------------------

/// `CrossSects` of one alignment: its attributes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCrossSectGroup {
    pub alignment: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub state: Option<String>,
    pub calc_method: Option<String>,
    pub curve_correction: Option<bool>,
    pub swell_factor: Option<f64>,
    pub shrink_factor: Option<f64>,
}

/// The attributes of a `CrossSect` beyond `sta`, `name` and `desc`, as stated (areas in square
/// metres and volumes in cubic metres when the file's units are ones the reader knows, otherwise
/// as written, with a warning; centroids in metres). Only sections that carry any, or that are
/// not in the first `CrossSects` of their alignment, have one.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlCrossSectDetail {
    pub alignment: String,
    /// The `CrossSect` it belongs to: its station (metres) and name.
    pub station_m: f64,
    pub name: Option<String>,
    /// Index among the `CrossSects` elements of the alignment.
    pub group: usize,
    pub angle_skew: Option<f64>,
    pub area_cut: Option<f64>,
    pub area_fill: Option<f64>,
    pub centroid_cut: Option<f64>,
    pub centroid_fill: Option<f64>,
    pub sect_type: Option<String>,
    pub volume_cut: Option<f64>,
    pub volume_fill: Option<f64>,
}

impl LandXmlCrossSection {
    /// The section surface of that name (existing ground, a design layer, ...).
    pub fn surface(&self, name: &str) -> Option<&LandXmlCrossSectSurf> {
        self.surfaces.iter().find(|s| s.name == name)
    }

    /// The names of the section surfaces, in file order.
    pub fn surface_names(&self) -> Vec<&str> {
        self.surfaces.iter().map(|s| s.name.as_str()).collect()
    }
}

impl LandXmlCrossSectSurf {
    /// All points as `[offset, elevation]`, runs joined in file order.
    pub fn points(&self) -> Vec<[f64; 2]> {
        self.point_lists.iter().flatten().copied().collect()
    }

    /// Lowest and highest offset, when there are points.
    pub fn offset_range(&self) -> Option<(f64, f64)> {
        let mut it = self.point_lists.iter().flatten().map(|p| p[0]);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
    }

    /// The elevation at an offset by straight-line interpolation along a run; `None` outside every
    /// run (gaps between runs are gaps, not interpolated). Offsets are expected to increase along a run.
    pub fn elevation_at(&self, offset_m: f64) -> Option<f64> {
        for run in &self.point_lists {
            for w in run.windows(2) {
                let (a, b) = (w[0], w[1]);
                if (a[0] <= offset_m && offset_m <= b[0]) || (b[0] <= offset_m && offset_m <= a[0]) {
                    if a[0] == b[0] {
                        return Some(a[1]);
                    }
                    return Some(a[1] + (b[1] - a[1]) * (offset_m - a[0]) / (b[0] - a[0]));
                }
            }
            if run.len() == 1 && run[0][0] == offset_m {
                return Some(run[0][1]);
            }
        }
        None
    }
}

impl LandXmlProfSurf {
    /// All points as `[station, elevation]`, runs joined.
    pub fn points(&self) -> Vec<[f64; 2]> {
        self.point_lists.iter().flatten().copied().collect()
    }

    /// First and last station.
    pub fn station_range(&self) -> Option<(f64, f64)> {
        let mut it = self.point_lists.iter().flatten().map(|p| p[0]);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
    }

    /// The ground level at a station by straight-line interpolation along a run; `None` in a gap.
    pub fn elevation_at(&self, station_m: f64) -> Option<f64> {
        for run in &self.point_lists {
            for w in run.windows(2) {
                let (a, b) = (w[0], w[1]);
                if a[0] <= station_m && station_m <= b[0] {
                    if a[0] == b[0] {
                        return Some(a[1]);
                    }
                    return Some(a[1] + (b[1] - a[1]) * (station_m - a[0]) / (b[0] - a[0]));
                }
            }
        }
        None
    }
}

// ----------------------------------------------------------------------------------------------
// Features, field notes
// ----------------------------------------------------------------------------------------------

/// `Property`: one piece of data the schema has no element for. `value` is the text as written.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlProperty {
    pub label: String,
    pub value: String,
}

/// `Feature`: a generic property bag (`name`, `code`, `source`, `Property`, `DocFileRef`, nested `Feature`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlFeature {
    pub name: Option<String>,
    pub code: Option<String>,
    pub source: Option<String>,
    pub properties: Vec<LandXmlProperty>,
    /// The attributes of each `DocFileRef`.
    pub doc_file_refs: Vec<BTreeMap<String, String>>,
    pub features: Vec<LandXmlFeature>,
}

impl LandXmlFeature {
    /// The value of the first property of that label.
    pub fn property(&self, label: &str) -> Option<&str> {
        self.properties.iter().find(|p| p.label == label).map(|p| p.value.as_str())
    }

    /// The first property of that label read as a number (`None` when absent or not one).
    pub fn number(&self, label: &str) -> Option<f64> {
        self.property(label)?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    }
}

/// The object a `Feature` or a `FieldNote` hangs on.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlOwner {
    /// The local name of the parent element: `Alignment`, `Surface`, `Profile`, `ProfAlign`,
    /// `ProfSurf`, `CrossSects`, `CrossSect`, `CoordinateSystem`, ...
    pub kind: String,
    /// Its `name` attribute (for a `CrossSect`, its `name`; see `station_m`).
    pub name: Option<String>,
    /// The `name` of the enclosing `Alignment`, when there is one (also for an `Alignment` itself).
    pub alignment: Option<String>,
    /// `sta` of a `CrossSect`, metres.
    pub station_m: Option<f64>,
    /// Which element of that kind it is, counting from 0 in document order through the whole file
    /// (the 3rd `PlanFeature` has ordinal 2). Tells apart objects that have no name.
    #[serde(default)]
    pub ordinal: usize,
}

/// The `Feature` children of one element.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlObjectFeatures {
    pub owner: LandXmlOwner,
    pub features: Vec<LandXmlFeature>,
}

/// `FieldNote`: text placed by the exporter. `items` holds `key=value` pairs when the whole text is
/// a `;`-separated list of them (Civil 3D's `Transformation:LocalPoint=...;GridPoint=...;...`);
/// the first item keeps its prefix up to the colon in `prefix`. The values are text: nothing is
/// applied to any coordinate.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LandXmlFieldNote {
    pub owner: LandXmlOwner,
    pub text: String,
    pub prefix: Option<String>,
    pub items: Vec<(String, String)>,
}

fn parse_field_note(text: &str) -> (Option<String>, Vec<(String, String)>) {
    let t = text.trim();
    let (prefix, body) = match t.split_once(':') {
        Some((p, rest)) if !p.contains('=') && !p.contains(';') && !p.trim().is_empty() => (Some(p.trim().to_string()), rest),
        _ => (None, t),
    };
    let mut items = Vec::new();
    for chunk in body.split(';').map(str::trim).filter(|c| !c.is_empty()) {
        match chunk.split_once('=') {
            Some((k, v)) => items.push((k.trim().to_string(), v.trim().to_string())),
            None => return (None, Vec::new()),
        }
    }
    (prefix, items)
}

// ----------------------------------------------------------------------------------------------
// Accessors
// ----------------------------------------------------------------------------------------------

/// One alignment of a [`LandXmlFile`] with everything the file holds for it.
#[derive(Debug, Clone, Copy)]
pub struct LandXmlAlignmentView<'a> {
    file: &'a LandXmlFile,
    pub alignment: &'a Alignment,
}

impl<'a> LandXmlAlignmentView<'a> {
    pub fn name(&self) -> &'a str {
        &self.alignment.name
    }

    /// Its attributes and stated geometry.
    pub fn info(&self) -> Option<&'a LandXmlAlignmentInfo> {
        self.file.alignment_infos.iter().find(|i| i.name == self.alignment.name)
    }

    /// Its cross-sections in file order.
    pub fn cross_sections(&self) -> Vec<&'a LandXmlCrossSection> {
        self.file.cross_sections_of(&self.alignment.name)
    }

    /// The `CrossSects` attributes.
    pub fn cross_sect_groups(&self) -> Vec<&'a LandXmlCrossSectGroup> {
        self.file.cross_sect_groups.iter().filter(|g| g.alignment == self.alignment.name).collect()
    }

    /// The first existing-ground profile (`ProfSurf`).
    pub fn ground_profile(&self) -> Option<&'a LandXmlProfSurf> {
        self.file.ground_profile(&self.alignment.name)
    }

    /// Every existing-ground profile.
    pub fn ground_profiles(&self) -> Vec<&'a LandXmlProfSurf> {
        self.file.ground_profiles.iter().filter(|g| g.alignment == self.alignment.name).collect()
    }

    /// The first design profile (`ProfAlign`) as written.
    pub fn design_profile(&self) -> Option<&'a LandXmlDesignProfile> {
        self.file.design_profile(&self.alignment.name)
    }

    /// Every design profile.
    pub fn design_profiles(&self) -> Vec<&'a LandXmlDesignProfile> {
        self.file.design_profiles.iter().filter(|g| g.alignment == self.alignment.name).collect()
    }

    /// Its superelevation as the file writes it.
    pub fn superelevation_table(&self) -> Option<&'a LandXmlSuperelevationTable> {
        self.file.superelevation_table(&self.alignment.name)
    }

    /// The `Feature` elements that hang on the alignment itself.
    pub fn features(&self) -> Vec<&'a LandXmlFeature> {
        self.file.features_of("Alignment", &self.alignment.name)
    }
}

impl LandXmlFile {
    /// The names of the alignments, in file order.
    pub fn alignment_names(&self) -> Vec<&str> {
        self.alignments.iter().map(|a| a.name.as_str()).collect()
    }

    /// The names of the surfaces, in file order.
    pub fn surface_names(&self) -> Vec<&str> {
        self.surfaces.iter().map(|s| s.name.as_str()).collect()
    }

    /// The first alignment of that name.
    pub fn alignment(&self, name: &str) -> Option<&Alignment> {
        self.alignments.iter().find(|a| a.name == name)
    }

    /// The surface of that name.
    pub fn surface(&self, name: &str) -> Option<&LandXmlSurface> {
        self.surfaces.iter().find(|s| s.name == name)
    }

    /// The alignment of that name with all that the file holds for it.
    pub fn alignment_view(&self, name: &str) -> Option<LandXmlAlignmentView<'_>> {
        self.alignment(name).map(|alignment| LandXmlAlignmentView { file: self, alignment })
    }

    /// Every alignment with all that the file holds for it, in file order.
    pub fn alignment_views(&self) -> Vec<LandXmlAlignmentView<'_>> {
        self.alignments.iter().map(|alignment| LandXmlAlignmentView { file: self, alignment }).collect()
    }

    /// The attributes and stated geometry of an alignment by name.
    pub fn alignment_info(&self, name: &str) -> Option<&LandXmlAlignmentInfo> {
        self.alignment_infos.iter().find(|i| i.name == name)
    }

    /// The cross-sections of an alignment, in file order.
    pub fn cross_sections_of(&self, alignment: &str) -> Vec<&LandXmlCrossSection> {
        self.cross_sections.iter().filter(|c| c.alignment == alignment).collect()
    }

    /// The first existing-ground profile of an alignment.
    pub fn ground_profile(&self, alignment: &str) -> Option<&LandXmlProfSurf> {
        self.ground_profiles.iter().find(|g| g.alignment == alignment)
    }

    /// The first design profile of an alignment, as written.
    pub fn design_profile(&self, alignment: &str) -> Option<&LandXmlDesignProfile> {
        self.design_profiles.iter().find(|g| g.alignment == alignment)
    }

    /// The superelevation of an alignment as written.
    pub fn superelevation_table(&self, alignment: &str) -> Option<&LandXmlSuperelevationTable> {
        self.superelevation_tables.iter().find(|t| t.alignment == alignment)
    }

    /// The `Feature` elements on the object of that kind (an element name) and `name`.
    pub fn features_of(&self, kind: &str, name: &str) -> Vec<&LandXmlFeature> {
        self.object_features.iter().filter(|o| o.owner.kind == kind && o.owner.name.as_deref() == Some(name)).flat_map(|o| o.features.iter()).collect()
    }

    /// The `FieldNote`s with the `key` among their items (for instance `Transformation`'s items).
    pub fn field_note_value(&self, key: &str) -> Option<&str> {
        self.field_notes.iter().flat_map(|n| n.items.iter()).find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

// ----------------------------------------------------------------------------------------------
// Reading
// ----------------------------------------------------------------------------------------------

fn text_number(node: roxmltree::Node) -> Option<f64> {
    node.text().and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn attr_number(node: roxmltree::Node, name: &str) -> Option<f64> {
    node.attribute(name).and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn attr_string(node: roxmltree::Node, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_string)
}

fn attr_bool(node: roxmltree::Node, name: &str) -> Option<bool> {
    match node.attribute(name)?.trim() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// The `Feature` elements anywhere in the document, grouped by the element they hang on, and every
/// `FieldNote`. `alignment_names` maps each `Alignment` node to the name used for it everywhere.
pub(crate) fn read_features_and_notes(
    root: roxmltree::Node,
    scale: f64,
    alignment_names: &std::collections::HashMap<roxmltree::NodeId, String>,
) -> (Vec<LandXmlObjectFeatures>, Vec<LandXmlFieldNote>) {
    fn feature(node: roxmltree::Node) -> LandXmlFeature {
        let mut f = LandXmlFeature { name: attr_string(node, "name"), code: attr_string(node, "code"), source: attr_string(node, "source"), ..Default::default() };
        for c in node.children().filter(|n| n.is_element()) {
            match c.tag_name().name() {
                "Property" => f.properties.push(LandXmlProperty { label: c.attribute("label").unwrap_or("").to_string(), value: c.attribute("value").unwrap_or("").to_string() }),
                "DocFileRef" => f.doc_file_refs.push(c.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect()),
                "Feature" => f.features.push(feature(c)),
                _ => {}
            }
        }
        f
    }
    let mut counters: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut ordinals: std::collections::HashMap<roxmltree::NodeId, usize> = std::collections::HashMap::new();
    for n in root.descendants().filter(|n| n.is_element()) {
        let c = counters.entry(n.tag_name().name()).or_insert(0);
        ordinals.insert(n.id(), *c);
        *c += 1;
    }
    let owner_of = |parent: roxmltree::Node| -> LandXmlOwner {
        let kind = parent.tag_name().name().to_string();
        let alignment = parent.ancestors().find(|a| a.has_tag_name_local("Alignment")).and_then(|a| alignment_names.get(&a.id()).cloned());
        let name = if kind == "Alignment" { alignment.clone() } else { attr_string(parent, "name") };
        let station_m = if kind == "CrossSect" { attr_number(parent, "sta").map(|s| s * scale) } else { None };
        LandXmlOwner { kind, name, alignment, station_m, ordinal: ordinals.get(&parent.id()).copied().unwrap_or(0) }
    };
    let mut features: Vec<LandXmlObjectFeatures> = Vec::new();
    let mut by_parent: std::collections::HashMap<roxmltree::NodeId, usize> = std::collections::HashMap::new();
    let mut notes = Vec::new();
    for n in root.descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "Feature" => {
                let Some(parent) = n.parent().filter(|p| p.is_element() && !p.has_tag_name_local("Feature")) else { continue };
                let f = feature(n);
                match by_parent.get(&parent.id()) {
                    Some(&i) => features[i].features.push(f),
                    None => {
                        by_parent.insert(parent.id(), features.len());
                        features.push(LandXmlObjectFeatures { owner: owner_of(parent), features: vec![f] });
                    }
                }
            }
            "FieldNote" => {
                let Some(parent) = n.parent().filter(|p| p.is_element()) else { continue };
                let text: String = n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<Vec<_>>().join("");
                let (prefix, items) = parse_field_note(&text);
                notes.push(LandXmlFieldNote { owner: owner_of(parent), text: text.trim().to_string(), prefix, items });
            }
            _ => {}
        }
    }
    (features, notes)
}

/// Raw `Superelevation` records of an alignment node, with the direction of the curve from the
/// geometry of `alignment` (when it was imported).
pub(crate) fn read_superelevation_records(node: roxmltree::Node, scale: f64, alignment: Option<&Alignment>, name: &str, warnings: &mut Vec<String>) -> Vec<LandXmlSuperelevationRecord> {
    let mut out = Vec::new();
    for (i, el) in children(node, "Superelevation").enumerate() {
        let n = i + 1;
        let mut rec = LandXmlSuperelevationRecord { sta_start_m: attr_number(el, "staStart").map(|v| v * scale), sta_end_m: attr_number(el, "staEnd").map(|v| v * scale), ..Default::default() };
        for c in el.children().filter(|c| c.is_element()) {
            let tag = c.tag_name().name();
            if let Some(kind) = SUPER_STATIONS.iter().find(|k| critical_station_element(**k) == tag) {
                match text_number(c) {
                    Some(v) => rec.stations.push(LandXmlSuperStation { kind: *kind, station_m: v * scale }),
                    None if c.text().map_or(true, |t| t.trim().is_empty()) => {}
                    None => warnings.push(format!("Alignment '{name}': superelevation {n}: {tag} is not a number ('{}'); it was left out.", c.text().unwrap_or("").trim())),
                }
            } else if tag == "FullSuperelev" {
                match text_number(c) {
                    Some(v) => rec.full_super_rates_pct.push(v),
                    None if c.text().map_or(true, |t| t.trim().is_empty()) => {}
                    None => warnings.push(format!("Alignment '{name}': superelevation {n}: FullSuperelev is not a number ('{}'); it was left out.", c.text().unwrap_or("").trim())),
                }
            } else if tag == "AdverseSE" {
                if let Some(t) = c.text().map(str::trim).filter(|t| !t.is_empty()) {
                    rec.adverse.push(t.to_string());
                }
            }
        }
        if let (Some(a), Some(mid)) = (alignment, rec.middle()) {
            rec.turn = a.curvature_at(mid).and_then(|k| if k > 0.0 { Some(Turn::Left) } else if k < 0.0 { Some(Turn::Right) } else { None });
        }
        out.push(rec);
    }
    out
}

impl Rd {
    /// Every `ProfAlign` of an alignment node.
    pub(crate) fn design_profiles(&mut self, node: roxmltree::Node, alignment: &str) -> Vec<LandXmlDesignProfile> {
        let mut out = Vec::new();
        for profile in children(node, "Profile") {
            for pa in children(profile, "ProfAlign") {
                let name = pa.attribute("name").unwrap_or("").to_string();
                let mut dp = LandXmlDesignProfile {
                    alignment: alignment.to_string(),
                    profile: attr_string(profile, "name"),
                    profile_desc: attr_string(profile, "desc"),
                    profile_state: attr_string(profile, "state"),
                    profile_sta_start_m: attr_number(profile, "staStart").map(|v| v * self.scale),
                    desc: attr_string(pa, "desc"),
                    state: attr_string(pa, "state"),
                    name: name.clone(),
                    entries: Vec::new(),
                };
                for item in pa.children().filter(|c| c.is_element()) {
                    let tag = item.tag_name().name();
                    let kind = match tag {
                        "PVI" => LandXmlProfEntryKind::Pvi,
                        "ParaCurve" => LandXmlProfEntryKind::ParaCurve,
                        "UnsymParaCurve" => LandXmlProfEntryKind::UnsymParaCurve,
                        "CircCurve" => LandXmlProfEntryKind::CircCurve,
                        _ => continue,
                    };
                    let v: Option<Vec<f64>> = item.text().unwrap_or("").split_whitespace().map(|t| t.parse::<f64>().ok().filter(|x| x.is_finite())).collect();
                    let Some([station, elevation]) = v.as_deref().map(|v| <[f64; 2]>::try_from(v).ok()).unwrap_or(None) else {
                        self.warnings.push(format!("Alignment '{alignment}': profile '{name}': the text of a {tag} is not 'station elevation'; the entry was left out."));
                        continue;
                    };
                    let len = |a: &str| attr_number(item, a).map(|l| l * self.scale);
                    dp.entries.push(LandXmlProfEntry {
                        kind,
                        station_m: station * self.scale,
                        elevation_m: elevation * self.scale,
                        length_m: len("length"),
                        length_in_m: len("lengthIn"),
                        length_out_m: len("lengthOut"),
                        radius_m: len("radius"),
                        desc: attr_string(item, "desc"),
                    });
                }
                out.push(dp);
            }
        }
        out
    }

    /// The `CrossSects` groups of an alignment node: their attributes, and the details of the
    /// sections that have attributes beyond `sta`, `name` and `desc`.
    pub(crate) fn cross_sect_attributes(&mut self, node: roxmltree::Node, alignment: &str, groups: &mut Vec<LandXmlCrossSectGroup>, details: &mut Vec<LandXmlCrossSectDetail>) {
        let (area, volume) = (self.area_scale.unwrap_or(1.0), self.volume_scale.unwrap_or(1.0));
        for (gi, g) in children(node, "CrossSects").enumerate() {
            groups.push(LandXmlCrossSectGroup {
                alignment: alignment.to_string(),
                name: attr_string(g, "name"),
                desc: attr_string(g, "desc"),
                state: attr_string(g, "state"),
                calc_method: attr_string(g, "calcMethod"),
                curve_correction: attr_bool(g, "curveCorrection"),
                swell_factor: attr_number(g, "swellFactor"),
                shrink_factor: attr_number(g, "shrinkFactor"),
            });
            let mut seen: Vec<u64> = Vec::new();
            for cs in children(g, "CrossSect") {
                let Some(sta) = attr_number(cs, "sta") else { continue };
                let key = sta.to_bits();
                if seen.contains(&key) {
                    self.warnings.push(format!("Alignment '{alignment}': two CrossSect elements are at station {sta}, which the schema does not allow (sta is unique); both were read."));
                } else {
                    seen.push(key);
                }
                let d = LandXmlCrossSectDetail {
                    alignment: alignment.to_string(),
                    station_m: sta * self.scale,
                    name: attr_string(cs, "name"),
                    group: gi,
                    angle_skew: attr_number(cs, "angleSkew"),
                    area_cut: attr_number(cs, "areaCut").map(|v| v * area),
                    area_fill: attr_number(cs, "areaFill").map(|v| v * area),
                    centroid_cut: attr_number(cs, "centroidCut").map(|v| v * self.scale),
                    centroid_fill: attr_number(cs, "centroidFill").map(|v| v * self.scale),
                    sect_type: attr_string(cs, "sectType"),
                    volume_cut: attr_number(cs, "volumeCut").map(|v| v * volume),
                    volume_fill: attr_number(cs, "volumeFill").map(|v| v * volume),
                };
                let extra = d.angle_skew.is_some()
                    || d.area_cut.is_some()
                    || d.area_fill.is_some()
                    || d.centroid_cut.is_some()
                    || d.centroid_fill.is_some()
                    || d.sect_type.is_some()
                    || d.volume_cut.is_some()
                    || d.volume_fill.is_some();
                if extra || gi > 0 {
                    details.push(d);
                }
            }
        }
    }
}
