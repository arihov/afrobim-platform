//! Sample lines and section views: lines square to an alignment at chosen stations, each with its
//! own swath widths, sampling any number of surfaces and a corridor.
//!
//! A [`SectionView`] holds the data to draw a section view or a sheet: per sample line the ground
//! lines, the design points, links and shapes, offsets and elevations. Offsets are positive to
//! the right looking up-station.

use afrobim_alignment::Alignment;
use serde::{Deserialize, Serialize};

use crate::assembly::DesignGeometry;
use crate::model::{design_section_at, Baseline};
use crate::quantities::{earthwork_table, material_volumes, EndArea, MaterialStation, MaterialTable, QuantityOptions, QuantityTable, VolumeMethod};
use crate::section::{cut_fill_areas, CorridorError, Section};
use crate::tin::Tin;

/// One sample line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleLine {
    pub name: String,
    /// Internal station, m.
    pub station_m: f64,
    /// Swath to the left and right of the alignment, m (zero or more; not both zero).
    pub left_swath_m: f64,
    pub right_swath_m: f64,
}

/// Where sample lines are added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleStations {
    /// From the start (or the entered start) every `interval_m`, and at the end.
    Interval { interval_m: f64, start_station_m: Option<f64>, end_station_m: Option<f64> },
    /// These internal stations (each must be on the alignment).
    Listed(Vec<f64>),
    /// The start of every horizontal element and the end of the alignment.
    GeometryPoints,
    /// Every station where the alignment stores cross-fall.
    SuperelevationStations,
}

/// A named group of sample lines on one alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleLineGroup {
    pub name: String,
    /// Name of the alignment.
    pub alignment: String,
    /// In station order.
    pub lines: Vec<SampleLine>,
}

fn check_swath(left: f64, right: f64) -> Result<(), String> {
    if !(left.is_finite() && right.is_finite() && left >= 0.0 && right >= 0.0 && left + right > 0.0) {
        return Err(format!("the swath widths must not be negative and not both zero (got {left} and {right})"));
    }
    Ok(())
}

impl SampleLineGroup {
    pub fn new(name: &str, alignment: &Alignment) -> SampleLineGroup {
        SampleLineGroup { name: name.into(), alignment: alignment.name.clone(), lines: vec![] }
    }

    /// Adds one named line. Stations within 1 mm of an existing line are refused.
    pub fn add_line(&mut self, name: &str, station_m: f64, left_swath_m: f64, right_swath_m: f64) -> Result<(), String> {
        check_swath(left_swath_m, right_swath_m)?;
        if !station_m.is_finite() || self.lines.iter().any(|l| (l.station_m - station_m).abs() <= 1e-3) {
            return Err(format!("there is already a sample line at station {station_m:.3}"));
        }
        self.lines.push(SampleLine { name: name.into(), station_m, left_swath_m, right_swath_m });
        self.lines.sort_by(|a, b| a.station_m.total_cmp(&b.station_m));
        Ok(())
    }

    /// Adds lines at stations chosen by `spec`, all with the same swaths, named `"{prefix} {label}"`
    /// with the displayed station. Stations that already have a line are skipped. Returns how
    /// many were added.
    pub fn add_stations(&mut self, alignment: &Alignment, spec: &SampleStations, left_swath_m: f64, right_swath_m: f64, prefix: &str) -> Result<usize, String> {
        check_swath(left_swath_m, right_swath_m)?;
        let (a, b) = (alignment.start_station_m, alignment.end_station_m());
        let mut stations: Vec<f64> = match spec {
            SampleStations::Interval { interval_m, start_station_m, end_station_m } => {
                if !(interval_m.is_finite() && *interval_m > 0.0) {
                    return Err(format!("the interval must be greater than zero (got {interval_m})"));
                }
                let (s0, s1) = (start_station_m.unwrap_or(a).max(a), end_station_m.unwrap_or(b).min(b));
                let mut v = Vec::new();
                let mut k = 0.0;
                while s0 + k * interval_m < s1 - 1e-9 {
                    v.push(s0 + k * interval_m);
                    k += 1.0;
                }
                v.push(s1);
                v
            }
            SampleStations::Listed(v) => {
                if let Some(s) = v.iter().find(|s| !(**s >= a - 1e-9 && **s <= b + 1e-9)) {
                    return Err(format!("station {s:.3} is not on the alignment ({a:.3} to {b:.3})"));
                }
                v.clone()
            }
            SampleStations::GeometryPoints => {
                let mut v = alignment.element_stations();
                v.push(b);
                v
            }
            SampleStations::SuperelevationStations => alignment.superelevation.as_ref().map(|s| s.sections.iter().flat_map(|x| x.points.iter().map(|p| p.station_m)).collect()).unwrap_or_default(),
        };
        stations.sort_by(f64::total_cmp);
        let mut added = 0;
        for s in stations {
            if self.lines.iter().any(|l| (l.station_m - s).abs() <= 1e-3) {
                continue;
            }
            let name = format!("{prefix} {}", alignment.station_label(s, 3));
            self.add_line(&name, s, left_swath_m, right_swath_m)?;
            added += 1;
        }
        Ok(added)
    }

    /// Changes the swaths of one line.
    pub fn set_swath(&mut self, index: usize, left_swath_m: f64, right_swath_m: f64) -> Result<(), String> {
        check_swath(left_swath_m, right_swath_m)?;
        let l = self.lines.get_mut(index).ok_or_else(|| format!("there is no sample line {index}"))?;
        l.left_swath_m = left_swath_m;
        l.right_swath_m = right_swath_m;
        Ok(())
    }

    pub fn rename_line(&mut self, index: usize, name: &str) -> Result<(), String> {
        self.lines.get_mut(index).ok_or_else(|| format!("there is no sample line {index}"))?.name = name.into();
        Ok(())
    }

    pub fn remove_line(&mut self, index: usize) -> Option<SampleLine> {
        (index < self.lines.len()).then(|| self.lines.remove(index))
    }
}

/// A surface sampled along a line: one run per piece of the line the surface covers, each
/// (offset, level) ascending in offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceLine {
    pub surface: String,
    pub runs: Vec<Vec<(f64, f64)>>,
}

/// What the corridor contributes at a sample line.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignView {
    pub region: usize,
    pub geometry: DesignGeometry,
    /// Finished surface, earthworks line, daylight and areas (against the corridor's ground).
    pub section: Section,
}

/// The data of a section view at one sample line.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionView {
    pub line: SampleLine,
    /// Displayed station.
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub heading_rad: f64,
    pub curvature_back: f64,
    pub curvature_ahead: f64,
    /// The profile level on the centreline, where the alignment has a profile.
    pub profile_elevation_m: Option<f64>,
    /// One per surface sampled, in the order given.
    pub ground_lines: Vec<SurfaceLine>,
    /// Present when a corridor was sampled and a region covers the station.
    pub design: Option<DesignView>,
    pub notes: Vec<String>,
}

impl SectionView {
    /// A sampled surface by name.
    pub fn surface(&self, name: &str) -> Option<&SurfaceLine> {
        self.ground_lines.iter().find(|s| s.surface == name)
    }
}

/// A corridor to sample: a baseline definition and the ground its side slopes meet.
#[derive(Debug, Clone, Copy)]
pub struct CorridorSampling<'a> {
    pub baseline: &'a Baseline,
    pub ground: &'a Tin,
}

fn sample_surface(tin: &Tin, alignment: &Alignment, line: &SampleLine) -> Result<Vec<Vec<(f64, f64)>>, CorridorError> {
    let p = alignment.point_at(line.station_m).ok_or(CorridorError::Station(line.station_m))?;
    let (a, b) = (p.offset(-line.left_swath_m), p.offset(line.right_swath_m));
    Ok(tin.section(a, b).into_iter().map(|r| r.into_iter().map(|(d, z)| (d - line.left_swath_m, z)).collect()).collect())
}

/// Samples the surfaces and the corridor at every line of the group, in order.
///
/// Ground lines are cut to the swath. The design is not: a part of the design outside the swath
/// is still reported, so the drawing can choose.
pub fn section_views(group: &SampleLineGroup, alignment: &Alignment, surfaces: &[(&str, &Tin)], corridor: Option<CorridorSampling>) -> Result<Vec<SectionView>, CorridorError> {
    if group.alignment != alignment.name {
        return Err(CorridorError::Definition(format!("sample line group '{}' is on alignment '{}', not '{}'", group.name, group.alignment, alignment.name)));
    }
    let (start, end) = (alignment.start_station_m, alignment.end_station_m());
    let nudge = ((end - start) / 2.0).min(1e-6);
    let curvature = |s: f64| alignment.curvature_at(s.clamp(start, end)).unwrap_or(0.0);
    let mut out = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        check_swath(line.left_swath_m, line.right_swath_m).map_err(|e| CorridorError::Definition(format!("sample line '{}': {e}", line.name)))?;
        let at = alignment.point_at(line.station_m).ok_or(CorridorError::Station(line.station_m))?;
        let mut notes = Vec::new();
        let mut ground_lines = Vec::new();
        for (name, tin) in surfaces {
            let runs = sample_surface(tin, alignment, line)?;
            if runs.is_empty() {
                notes.push(format!("Surface '{name}' does not cover this line."));
            }
            ground_lines.push(SurfaceLine { surface: name.to_string(), runs });
        }
        let design = match corridor {
            None => None,
            Some(c) => {
                let region = c.baseline.regions.iter().rposition(|r| r.start_station_m <= line.station_m + 1e-9 && line.station_m <= r.end_station_m + 1e-9);
                match region {
                    None => {
                        notes.push("No region of the corridor covers this station.".to_string());
                        None
                    }
                    Some(ri) => {
                        let m = design_section_at(c.baseline, ri, alignment, c.ground, line.station_m)?;
                        notes.extend(m.section.notes.iter().cloned());
                        Some(DesignView { region: ri, geometry: m.design, section: m.section })
                    }
                }
            }
        };
        out.push(SectionView {
            line: line.clone(),
            label: alignment.station_label(line.station_m, 3),
            x: at.x,
            y: at.y,
            heading_rad: at.heading_rad,
            curvature_back: curvature(line.station_m - nudge),
            curvature_ahead: curvature(line.station_m + nudge),
            profile_elevation_m: alignment.profile.as_ref().and_then(|p| p.elevation_at(line.station_m).ok()),
            ground_lines,
            design,
            notes,
        });
    }
    Ok(out)
}

/// Shape volumes between the sample lines of the views that have a design.
pub fn views_material_volumes(views: &[SectionView]) -> MaterialTable {
    let stations: Vec<MaterialStation> = views.iter().filter_map(|v| v.design.as_ref().map(|d| MaterialStation { station_m: v.line.station_m, label: &v.label, design: &d.geometry })).collect();
    material_volumes(&stations)
}

fn run_at_centre(runs: &[Vec<(f64, f64)>]) -> Option<&Vec<(f64, f64)>> {
    runs.iter().find(|r| r.len() >= 2 && r[0].0 <= 1e-9 && r[r.len() - 1].0 >= -1e-9)
}

fn end_area_between(alignment: &Alignment, line: &SampleLine, existing: &Tin, proposed: &Tin) -> Result<EndArea, CorridorError> {
    let (start, end) = (alignment.start_station_m, alignment.end_station_m());
    let nudge = ((end - start) / 2.0).min(1e-6);
    let curvature = |s: f64| alignment.curvature_at(s.clamp(start, end)).unwrap_or(0.0);
    let (ex, pr) = (sample_surface(existing, alignment, line)?, sample_surface(proposed, alignment, line)?);
    let mut e = EndArea {
        station_m: line.station_m,
        label: alignment.station_label(line.station_m, 3),
        cut_m2: None,
        fill_m2: None,
        cut_centroid_offset_m: None,
        fill_centroid_offset_m: None,
        curvature_back: curvature(line.station_m - nudge),
        curvature_ahead: curvature(line.station_m + nudge),
    };
    if let (Some(g), Some(p)) = (run_at_centre(&ex), run_at_centre(&pr)) {
        // Cut where the existing ground is above the proposed surface.
        let a = cut_fill_areas(p, g);
        e.cut_m2 = Some(a.cut_m2);
        e.fill_m2 = Some(a.fill_m2);
        e.cut_centroid_offset_m = (a.cut_m2 > 1e-9).then(|| a.cut_moment_m3 / a.cut_m2);
        e.fill_centroid_offset_m = (a.fill_m2 > 1e-9).then(|| a.fill_moment_m3 / a.fill_m2);
    }
    Ok(e)
}

/// Cut and fill between an existing and a proposed surface over the swaths of the sample lines
/// (cut where the existing surface is above the proposed one), as a table of volumes between
/// consecutive lines. Only the part of each surface inside the swath is compared, over the width
/// both cover. With [`VolumeMethod::Prismoidal`] the area at the middle of each interval is
/// sampled with the mean of the two swaths.
pub fn surface_earthworks(group: &SampleLineGroup, alignment: &Alignment, existing: &Tin, proposed: &Tin, options: &QuantityOptions) -> Result<QuantityTable, CorridorError> {
    let mut ends = Vec::new();
    for l in &group.lines {
        ends.push(end_area_between(alignment, l, existing, proposed)?);
    }
    let mut mids = Vec::new();
    if options.method == VolumeMethod::Prismoidal {
        for w in group.lines.windows(2) {
            let mid = SampleLine {
                name: String::new(),
                station_m: (w[0].station_m + w[1].station_m) / 2.0,
                left_swath_m: (w[0].left_swath_m + w[1].left_swath_m) / 2.0,
                right_swath_m: (w[0].right_swath_m + w[1].right_swath_m) / 2.0,
            };
            mids.push(Some(end_area_between(alignment, &mid, existing, proposed)?));
        }
    }
    Ok(earthwork_table(&ends, &mids, options))
}
