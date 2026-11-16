//! Quantities: earthworks between sections by average end area or by the prismoidal formula,
//! material volumes per shape code, and the mass haul with a free haul distance.
//!
//! The existing [`crate::earthworks`] keeps working unchanged; this module is the general form
//! (any list of end areas, either method).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::assembly::{Classification, DesignGeometry};
use crate::section::Section;
use crate::template::VolumeOptions;

/// How the volume between two sections is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VolumeMethod {
    /// `V = L (A₁ + A₂) / 2`.
    #[default]
    AverageEndArea,
    /// `V = L (A₁ + 4 Aₘ + A₂) / 6`, with `Aₘ` the area of a section at the middle station of the
    /// interval. Exact when the area varies at most as a quadratic along the interval.
    Prismoidal,
}

/// Factors and the method for [`earthwork_table`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct QuantityOptions {
    /// Cut and fill factors (shrink and swell, entered) and the curvature (Pappus) correction.
    #[serde(default)]
    pub volumes: VolumeOptions,
    #[serde(default)]
    pub method: VolumeMethod,
}

/// The cut and fill of one section: all [`earthwork_table`] needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndArea {
    pub station_m: f64,
    pub label: String,
    /// `None`: not computed (no ground or no daylight).
    pub cut_m2: Option<f64>,
    pub fill_m2: Option<f64>,
    /// Offsets of the centroids, m, for the curvature correction.
    pub cut_centroid_offset_m: Option<f64>,
    pub fill_centroid_offset_m: Option<f64>,
    /// Horizontal curvature just behind and ahead of the station, 1/m, positive turning left.
    pub curvature_back: f64,
    pub curvature_ahead: f64,
}

impl EndArea {
    pub fn from_section(s: &Section) -> EndArea {
        EndArea {
            station_m: s.station_m,
            label: s.label.clone(),
            cut_m2: s.cut_area_m2,
            fill_m2: s.fill_area_m2,
            cut_centroid_offset_m: s.cut_centroid_offset_m,
            fill_centroid_offset_m: s.fill_centroid_offset_m,
            curvature_back: s.curvature_back,
            curvature_ahead: s.curvature_ahead,
        }
    }
}

/// One line of the table: the section and the volumes from the one before it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityRow {
    pub station_m: f64,
    pub label: String,
    pub cut_area_m2: Option<f64>,
    pub fill_area_m2: Option<f64>,
    pub cut_volume_m3: Option<f64>,
    pub fill_volume_m3: Option<f64>,
    pub cumulative_cut_m3: f64,
    pub cumulative_fill_m3: f64,
    /// Running sum of `cut × cut factor − fill × fill factor`.
    pub mass_ordinate_m3: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityTable {
    pub rows: Vec<QuantityRow>,
    pub total_cut_m3: f64,
    pub total_fill_m3: f64,
    /// Length for which a volume could not be computed, m.
    pub unmeasured_length_m: f64,
    pub options: QuantityOptions,
}

impl QuantityTable {
    /// The stations and mass ordinates, for [`mass_haul`].
    pub fn mass_curve(&self) -> (Vec<f64>, Vec<f64>) {
        (self.rows.iter().map(|r| r.station_m).collect(), self.rows.iter().map(|r| r.mass_ordinate_m3).collect())
    }
}

/// Earthwork volumes between consecutive end areas.
///
/// `mids[i]` is the area at the middle station between `ends[i]` and `ends[i + 1]`; it is read
/// only by the prismoidal method (pass `&[]` for the average end area). Where the method needs a
/// figure that is missing, the volume of that interval is `None` and its length is counted as
/// unmeasured. The curvature correction multiplies each area by `1 + κ e` (see
/// [`crate::earthworks`]); `κ` for a middle area is its `curvature_back`. Two sections at the
/// same station (a region boundary) enclose no volume.
pub fn earthwork_table(ends: &[EndArea], mids: &[Option<EndArea>], options: &QuantityOptions) -> QuantityTable {
    let v = &options.volumes;
    let eff = |area: Option<f64>, centroid: Option<f64>, curvature: f64| area.map(|a| if v.curvature_correction { a * (1.0 + curvature * centroid.unwrap_or(0.0)) } else { a });
    let mut rows = Vec::with_capacity(ends.len());
    let (mut cut_sum, mut fill_sum, mut mass, mut unmeasured) = (0.0, 0.0, 0.0, 0.0);
    for (i, s) in ends.iter().enumerate() {
        let (mut cut, mut fill) = (Some(0.0), Some(0.0));
        if i > 0 {
            let p = &ends[i - 1];
            let length = s.station_m - p.station_m;
            let m = mids.get(i - 1).and_then(|m| m.as_ref());
            let volume = |a: Option<f64>, b: Option<f64>, mid: Option<Option<f64>>| -> Option<f64> {
                if length == 0.0 {
                    return Some(0.0);
                }
                match options.method {
                    VolumeMethod::AverageEndArea => Some(length * (a? + b?) / 2.0),
                    VolumeMethod::Prismoidal => Some(length * (a? + 4.0 * mid?? + b?) / 6.0),
                }
            };
            cut = volume(
                eff(p.cut_m2, p.cut_centroid_offset_m, p.curvature_ahead),
                eff(s.cut_m2, s.cut_centroid_offset_m, s.curvature_back),
                m.map(|m| eff(m.cut_m2, m.cut_centroid_offset_m, m.curvature_back)),
            );
            fill = volume(
                eff(p.fill_m2, p.fill_centroid_offset_m, p.curvature_ahead),
                eff(s.fill_m2, s.fill_centroid_offset_m, s.curvature_back),
                m.map(|m| eff(m.fill_m2, m.fill_centroid_offset_m, m.curvature_back)),
            );
            if cut.is_none() || fill.is_none() {
                unmeasured += length;
            }
        }
        cut_sum += cut.unwrap_or(0.0);
        fill_sum += fill.unwrap_or(0.0);
        mass += cut.unwrap_or(0.0) * v.cut_factor - fill.unwrap_or(0.0) * v.fill_factor;
        rows.push(QuantityRow {
            station_m: s.station_m,
            label: s.label.clone(),
            cut_area_m2: s.cut_m2,
            fill_area_m2: s.fill_m2,
            cut_volume_m3: cut,
            fill_volume_m3: fill,
            cumulative_cut_m3: cut_sum,
            cumulative_fill_m3: fill_sum,
            mass_ordinate_m3: mass,
        });
    }
    QuantityTable { rows, total_cut_m3: cut_sum, total_fill_m3: fill_sum, unmeasured_length_m: unmeasured, options: *options }
}

pub const QUANTITY_CSV_HEADER: &str = "Station,Cut area (m2),Fill area (m2),Cut volume (m3),Fill volume (m3),Cumulative cut (m3),Cumulative fill (m3),Mass ordinate (m3)";

/// The table as CSV (three decimals; a figure not computed is left empty).
pub fn quantity_csv(t: &QuantityTable) -> String {
    let n = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.3}"));
    let mut out = String::from(QUANTITY_CSV_HEADER);
    out.push('\n');
    for r in &t.rows {
        out.push_str(&format!(
            "{},{},{},{},{},{:.3},{:.3},{:.3}\n",
            r.label,
            n(r.cut_area_m2),
            n(r.fill_area_m2),
            n(r.cut_volume_m3),
            n(r.fill_volume_m3),
            r.cumulative_cut_m3,
            r.cumulative_fill_m3,
            r.mass_ordinate_m3
        ));
    }
    out
}

// ─── mass haul ──────────────────────────────────────────────────────────────────────────────

/// The haul worked out from a mass curve and a free haul distance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MassHaul {
    pub free_haul_m: f64,
    /// Volume moved between a cut and a fill inside the curve (balanced), m³.
    pub balanced_volume_m3: f64,
    /// Part of it moved no further than the free haul distance, m³.
    pub free_haul_volume_m3: f64,
    /// Part of it moved further, m³.
    pub overhaul_volume_m3: f64,
    /// Total haul moment, m³·m: the area between the mass curve and its balance lines.
    pub total_haul_moment_m3m: f64,
    /// Overhaul moment, m³·m: `∫ max(L − free haul, 0) dV`, with `L` the haul distance of each
    /// slice of volume.
    pub overhaul_moment_m3m: f64,
    /// Last ordinate minus first: volume carried out of (negative: borrowed into) the length, m³.
    pub residual_m3: f64,
}

/// The mass haul of a piecewise linear mass curve.
///
/// A slice of volume `dc` at ordinate `c` is moved from where the curve crosses `c` to the next
/// crossing, which is the width of the loop of the curve at that level (the length of the
/// horizontal balance line). The crossings of one level alternate up and down, and are paired
/// in order from the start: the first with the second, the third with the fourth. A pair that
/// opens upward (cut then fill) is hauled forward, one that opens downward backward; the last
/// crossing of a level with an odd number is the net volume leaving or entering the length and
/// is not hauled. Between two vertex ordinates every crossing station is linear in `c`, so the
/// haul distance is linear and the integrals below are exact:
///
/// ```text
///   total moment   = Σ dc (L₀ + L₁) / 2
///   overhaul moment = Σ dc ∫₀¹ max(L₀ + (L₁ − L₀) t − F, 0) dt
/// ```
///
/// `free_haul_m` is the entered free haul distance `F`. Stations must not decrease; points at
/// one station are merged.
pub fn mass_haul(stations: &[f64], ordinates: &[f64], free_haul_m: f64) -> Result<MassHaul, String> {
    if stations.len() != ordinates.len() || stations.len() < 2 {
        return Err("the mass curve needs at least two points, one ordinate per station".into());
    }
    if !(free_haul_m.is_finite() && free_haul_m >= 0.0) {
        return Err(format!("the free haul distance must not be negative (got {free_haul_m})"));
    }
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for (s, m) in stations.iter().zip(ordinates) {
        if !(s.is_finite() && m.is_finite()) {
            return Err("a station or ordinate is not a finite number".into());
        }
        if pts.last().is_some_and(|p| *s < p.0) {
            return Err("stations must not decrease".into());
        }
        if pts.last().is_some_and(|p| p.0 == *s) {
            pts.pop();
        }
        pts.push((*s, *m));
    }
    let mut levels: Vec<f64> = pts.iter().map(|p| p.1).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let mut out = MassHaul {
        free_haul_m,
        balanced_volume_m3: 0.0,
        free_haul_volume_m3: 0.0,
        overhaul_volume_m3: 0.0,
        total_haul_moment_m3m: 0.0,
        overhaul_moment_m3m: 0.0,
        residual_m3: pts[pts.len() - 1].1 - pts[0].1,
    };
    for band in levels.windows(2) {
        let (c0, c1) = (band[0], band[1]);
        let cm = (c0 + c1) / 2.0;
        // The segments crossing the middle of the band, in station order, as station(c).
        let crossings: Vec<(f64, f64, f64)> = pts
            .windows(2)
            .filter(|w| (w[0].1 < cm && cm < w[1].1) || (w[1].1 < cm && cm < w[0].1))
            .map(|w| (w[0].0, w[0].1, (w[1].0 - w[0].0) / (w[1].1 - w[0].1)))
            .collect();
        let at = |c: &(f64, f64, f64), level: f64| c.0 + (level - c.1) * c.2;
        for pair in crossings.chunks_exact(2) {
            let (l0, l1) = (at(&pair[1], c0) - at(&pair[0], c0), at(&pair[1], c1) - at(&pair[0], c1));
            let (l0, l1) = (l0.abs(), l1.abs());
            let dc = c1 - c0;
            let f = free_haul_m;
            out.balanced_volume_m3 += dc;
            out.total_haul_moment_m3m += dc * (l0 + l1) / 2.0;
            let (free, over, over_moment) = if l0 <= f && l1 <= f {
                (1.0, 0.0, 0.0)
            } else if l0 > f && l1 > f {
                (0.0, 1.0, (l0 + l1) / 2.0 - f)
            } else {
                let t = (f - l0) / (l1 - l0);
                if l0 <= f {
                    (t, 1.0 - t, (l1 - f) * (1.0 - t) / 2.0)
                } else {
                    (1.0 - t, t, (l0 - f) * t / 2.0)
                }
            };
            out.free_haul_volume_m3 += dc * free;
            out.overhaul_volume_m3 += dc * over;
            out.overhaul_moment_m3m += dc * over_moment;
        }
    }
    Ok(out)
}

// ─── materials ──────────────────────────────────────────────────────────────────────────────

/// A section's shapes at a station.
#[derive(Debug, Clone, Copy)]
pub struct MaterialStation<'a> {
    pub station_m: f64,
    pub label: &'a str,
    pub design: &'a DesignGeometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialRow {
    pub station_m: f64,
    pub label: String,
    /// Area of each shape code at the station, m² (both sides together).
    pub areas_m2: BTreeMap<String, f64>,
    /// Volume from the previous station to this one, by average end area, m³.
    pub volumes_m3: BTreeMap<String, f64>,
    pub cumulative_m3: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTotal {
    pub shape_code: String,
    pub volume_m3: f64,
    /// The classification of the first shape found with the code.
    pub classification: Classification,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTable {
    pub rows: Vec<MaterialRow>,
    pub totals: Vec<MaterialTotal>,
    pub notes: Vec<String>,
}

/// Volume of each shape code between consecutive stations, by average end area
/// `V = L (A₁ + A₂) / 2` on the plan length of the centreline (no curvature correction). A shape
/// code present at one end of an interval and not the other counts as zero area at the other
/// end, and the interval is noted. Shapes are totalled by code over both sides.
pub fn material_volumes(stations: &[MaterialStation]) -> MaterialTable {
    let area_of = |d: &DesignGeometry| {
        let mut m: BTreeMap<String, f64> = BTreeMap::new();
        for s in &d.shapes {
            *m.entry(s.code.clone()).or_default() += s.area_m2;
        }
        m
    };
    let mut rows: Vec<MaterialRow> = Vec::new();
    let mut totals: BTreeMap<String, MaterialTotal> = BTreeMap::new();
    let mut notes = Vec::new();
    for st in stations {
        for s in &st.design.shapes {
            totals.entry(s.code.clone()).or_insert_with(|| MaterialTotal { shape_code: s.code.clone(), volume_m3: 0.0, classification: s.classification.clone() });
        }
    }
    let mut cumulative: BTreeMap<String, f64> = BTreeMap::new();
    for (i, st) in stations.iter().enumerate() {
        let areas = area_of(st.design);
        let mut volumes = BTreeMap::new();
        if i > 0 {
            let prev = &rows[i - 1];
            let length = st.station_m - prev.station_m;
            let codes: Vec<String> = totals.keys().cloned().collect();
            for code in &codes {
                let (a, b) = (prev.areas_m2.get(code), areas.get(code));
                if length > 0.0 && a.is_some() != b.is_some() {
                    notes.push(format!("Shape '{code}' is present at only one of the stations {} and {}; counted as zero at the other.", prev.label, st.label));
                }
                let v = length * (a.copied().unwrap_or(0.0) + b.copied().unwrap_or(0.0)) / 2.0;
                if a.is_some() || b.is_some() {
                    volumes.insert(code.clone(), v);
                    *cumulative.entry(code.clone()).or_default() += v;
                    totals.get_mut(code).unwrap().volume_m3 += v;
                }
            }
        }
        rows.push(MaterialRow { station_m: st.station_m, label: st.label.to_string(), areas_m2: areas, volumes_m3: volumes, cumulative_m3: cumulative.clone() });
    }
    MaterialTable { rows, totals: totals.into_values().collect(), notes }
}
