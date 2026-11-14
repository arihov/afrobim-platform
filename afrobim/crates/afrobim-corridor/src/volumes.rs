//! Earthwork volumes by average end area, and the mass ordinate.

use crate::section::Section;
use crate::template::VolumeOptions;

/// One line of the earthworks table: a section, and the volumes between the section before it
/// and this one.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeRow {
    /// Internal station, m.
    pub station_m: f64,
    /// Displayed station.
    pub label: String,
    /// `None` where the section has no areas (no ground or no daylight).
    pub cut_area_m2: Option<f64>,
    pub fill_area_m2: Option<f64>,
    /// Volume from the section before to this one, m³. Zero on the first row. `None` when either
    /// section has no areas.
    pub cut_volume_m3: Option<f64>,
    pub fill_volume_m3: Option<f64>,
    /// Sum of the cut volumes so far (unfactored), m³. Lengths with no volume add nothing.
    pub cumulative_cut_m3: f64,
    pub cumulative_fill_m3: f64,
    /// Sum so far of `cut × cut factor − fill × fill factor`, m³.
    pub mass_ordinate_m3: f64,
}

/// The earthworks of a corridor.
#[derive(Debug, Clone, PartialEq)]
pub struct Earthworks {
    pub rows: Vec<VolumeRow>,
    pub total_cut_m3: f64,
    pub total_fill_m3: f64,
    /// Length along the centreline for which no volume could be computed, m.
    pub unmeasured_length_m: f64,
    pub options: VolumeOptions,
}

/// Volumes between consecutive sections by the average end area method:
///
/// ```text
///   V = L (A₁ + A₂) / 2        L = difference of the internal stations
/// ```
///
/// for cut and for fill separately. The two sections at a template region boundary are at the
/// same station, so nothing is counted between them.
///
/// With `curvature_correction` each end area is first multiplied by `1 + κ e`, where `κ` is the
/// horizontal curvature at the section on the side of the interval (1/m, positive turning left)
/// and `e` the offset of the centroid of that area (m, positive right). This is Pappus's
/// theorem: an area whose centroid is `e` outside the centreline travels `1 + κ e` times as far
/// as the centreline does. It is exact for a constant section on a circular arc and is otherwise
/// the usual approximation. Without it, volumes on curves are those of the centreline length.
///
/// No prismoidal correction is applied.
///
/// The mass ordinate is the running sum of `cut × cut_factor − fill × fill_factor`. The factors
/// are entered; at their default of 1.0 the ordinate is cumulative cut minus cumulative fill.
/// Cumulative cut and fill are never factored.
pub fn earthworks(sections: &[Section], options: &VolumeOptions) -> Earthworks {
    let mut rows: Vec<VolumeRow> = Vec::with_capacity(sections.len());
    let (mut cut_sum, mut fill_sum, mut mass, mut unmeasured) = (0.0, 0.0, 0.0, 0.0);
    let effective = |area: Option<f64>, centroid: Option<f64>, curvature: f64| {
        area.map(|a| if options.curvature_correction { a * (1.0 + curvature * centroid.unwrap_or(0.0)) } else { a })
    };
    for (i, s) in sections.iter().enumerate() {
        let (mut cut, mut fill) = (Some(0.0), Some(0.0));
        if i > 0 {
            let p = &sections[i - 1];
            let length = s.station_m - p.station_m;
            let between = |a: Option<f64>, b: Option<f64>| Some(length * (a? + b?) / 2.0);
            cut = between(
                effective(p.cut_area_m2, p.cut_centroid_offset_m, p.curvature_ahead),
                effective(s.cut_area_m2, s.cut_centroid_offset_m, s.curvature_back),
            );
            fill = between(
                effective(p.fill_area_m2, p.fill_centroid_offset_m, p.curvature_ahead),
                effective(s.fill_area_m2, s.fill_centroid_offset_m, s.curvature_back),
            );
            if cut.is_none() || fill.is_none() {
                unmeasured += length;
            }
        }
        cut_sum += cut.unwrap_or(0.0);
        fill_sum += fill.unwrap_or(0.0);
        mass += cut.unwrap_or(0.0) * options.cut_factor - fill.unwrap_or(0.0) * options.fill_factor;
        rows.push(VolumeRow {
            station_m: s.station_m,
            label: s.label.clone(),
            cut_area_m2: s.cut_area_m2,
            fill_area_m2: s.fill_area_m2,
            cut_volume_m3: cut,
            fill_volume_m3: fill,
            cumulative_cut_m3: cut_sum,
            cumulative_fill_m3: fill_sum,
            mass_ordinate_m3: mass,
        });
    }
    Earthworks { rows, total_cut_m3: cut_sum, total_fill_m3: fill_sum, unmeasured_length_m: unmeasured, options: *options }
}

/// The header line of [`earthworks_csv`].
pub const EARTHWORKS_CSV_HEADER: &str =
    "Station,Cut area (m2),Fill area (m2),Cut volume (m3),Fill volume (m3),Cumulative cut (m3),Cumulative fill (m3),Mass ordinate (m3)";

/// The earthworks table as CSV: the header, then one line per section. The station is the
/// displayed label. Areas and volumes have three decimals; a figure that could not be computed
/// is left empty. Lines end with `\n`.
pub fn earthworks_csv(earthworks: &Earthworks) -> String {
    let number = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.3}"));
    let mut out = String::from(EARTHWORKS_CSV_HEADER);
    out.push('\n');
    for r in &earthworks.rows {
        out.push_str(&format!(
            "{},{},{},{},{},{:.3},{:.3},{:.3}\n",
            r.label,
            number(r.cut_area_m2),
            number(r.fill_area_m2),
            number(r.cut_volume_m3),
            number(r.fill_volume_m3),
            r.cumulative_cut_m3,
            r.cumulative_fill_m3,
            r.mass_ordinate_m3
        ));
    }
    out
}
