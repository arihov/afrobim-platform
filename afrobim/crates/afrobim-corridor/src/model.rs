// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The corridor model: baselines, regions with their own assembly and frequency, corridor
//! surfaces from link codes, feature lines from point codes.
//!
//! This is the Civil 3D shaped counterpart of [`crate::Corridor::build`], which keeps working for
//! old definitions (see [`crate::Assembly::from_template`]). Every frequency is entered.

use afrobim_alignment::{Alignment, CrossFall, CrossFallPoint, ElementKind, Superelevation};
use serde::{Deserialize, Serialize};

use crate::assembly::{assemble, Assembly, DesignGeometry, EvalContext, Side};
use crate::quantities::{earthwork_table, EndArea, QuantityOptions, QuantityTable, VolumeMethod};
use crate::section::{cut_fill_areas, ground_run, trim_ground, CorridorError, Section, SectionPoint};
use crate::template::VolumeOptions;
use crate::tin::Tin;

/// Where a region has sections. Each spacing is **entered**; a kind of element with no spacing
/// gets no regular sections (its ends still do, if `at_geometry_points`). Regular sections stand
/// on whole multiples of the spacing (internal stations), as in the existing builder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frequency {
    /// Spacing on tangents (straight elements), m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangent_m: Option<f64>,
    /// Spacing on circular curves, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_m: Option<f64>,
    /// Spacing on spirals, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spiral_m: Option<f64>,
    /// A section at the start of every horizontal element, every PVI and the start and end of
    /// every vertical curve.
    #[serde(default)]
    pub at_geometry_points: bool,
    /// A section at every station where cross-fall is stored (the alignment's superelevation and
    /// the baseline's crossfall table): the critical stations.
    #[serde(default)]
    pub at_superelevation_critical: bool,
    /// Further stations, internal, m.
    #[serde(default)]
    pub extra_stations: Vec<f64>,
}

impl Frequency {
    /// The same spacing on tangents, curves and spirals, with geometry points and critical
    /// stations.
    pub fn uniform(spacing_m: f64) -> Frequency {
        Frequency { tangent_m: Some(spacing_m), curve_m: Some(spacing_m), spiral_m: Some(spacing_m), at_geometry_points: true, at_superelevation_critical: true, extra_stations: vec![] }
    }
}

/// A length of the baseline built with one assembly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssemblyRegion {
    pub name: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub assembly: Assembly,
    pub frequency: Frequency,
}

/// A baseline: an alignment (by name) and its regions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub name: String,
    /// Name of the alignment (with its profile) this baseline follows.
    pub alignment: String,
    /// At least one. Regions follow one another with no gap and no overlap; at a boundary there
    /// are two sections, the assembly behind and the assembly ahead.
    pub regions: Vec<AssemblyRegion>,
    /// Cross-fall against station, used for any side and station where the alignment stores
    /// none (see [`CrossFallPoint`]; stations increasing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crossfall_table: Vec<CrossFallPoint>,
}

/// A corridor: baselines and the volume factors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorridorModel {
    pub name: String,
    pub baselines: Vec<Baseline>,
    #[serde(default)]
    pub volumes: VolumeOptions,
}

impl Baseline {
    pub fn check(&self) -> Result<(), String> {
        if self.regions.is_empty() {
            return Err(format!("baseline '{}' needs at least one region", self.name));
        }
        for (i, r) in self.regions.iter().enumerate() {
            let at = format!("baseline '{}', region '{}'", self.name, r.name);
            if !(r.start_station_m.is_finite() && r.end_station_m.is_finite() && r.end_station_m > r.start_station_m) {
                return Err(format!("{at}: the end station must be after the start station"));
            }
            if i > 0 && (r.start_station_m - self.regions[i - 1].end_station_m).abs() > 1e-6 {
                return Err(format!("{at}: regions must follow one another with no gap or overlap (starts at {}, the region before ends at {})", r.start_station_m, self.regions[i - 1].end_station_m));
            }
            for (what, v) in [("tangent", r.frequency.tangent_m), ("curve", r.frequency.curve_m), ("spiral", r.frequency.spiral_m)] {
                if v.is_some_and(|v| !(v.is_finite() && v > 0.0)) {
                    return Err(format!("{at}: the {what} spacing must be greater than zero"));
                }
            }
            r.assembly.check().map_err(|e| format!("{at}: {e}"))?;
        }
        if !self.crossfall_table.is_empty() {
            Superelevation::from_points(self.crossfall_table.clone()).map_err(|e| format!("baseline '{}': the crossfall table: {e}", self.name))?;
        }
        Ok(())
    }

    fn cross_fall(&self, alignment: &Alignment, station_m: f64) -> CrossFall {
        let mut f = alignment.superelevation.as_ref().map(|s| s.cross_fall_at(station_m)).unwrap_or_default();
        if !self.crossfall_table.is_empty() && (f.left_pct.is_none() || f.right_pct.is_none()) {
            if let Ok(t) = Superelevation::from_points(self.crossfall_table.clone()) {
                let g = t.cross_fall_at(station_m);
                f.left_pct = f.left_pct.or(g.left_pct);
                f.right_pct = f.right_pct.or(g.right_pct);
            }
        }
        f
    }

    /// The range the baseline can be built over: its regions, the alignment and its profile.
    fn range(&self, alignment: &Alignment) -> Result<(f64, f64), CorridorError> {
        let profile = alignment.profile.as_ref().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
        let (p0, p1) = profile.range().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
        let start = self.regions[0].start_station_m.max(alignment.start_station_m).max(p0);
        let end = self.regions[self.regions.len() - 1].end_station_m.min(alignment.end_station_m()).min(p1);
        if end - start <= 1e-3 || (end - start).is_nan() {
            return Err(CorridorError::NoLength { start_m: start, end_m: end });
        }
        Ok((start, end))
    }

    /// The stations of the sections, each with the index of its region. A station on a boundary
    /// between regions appears twice, behind first.
    pub fn stations(&self, alignment: &Alignment) -> Result<Vec<(f64, usize)>, CorridorError> {
        self.check().map_err(CorridorError::Definition)?;
        let profile = alignment.profile.as_ref().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
        let curves = profile.curve_extents().map_err(|e| CorridorError::Profile(alignment.name.clone(), e.to_string()))?;
        let (start, end) = self.range(alignment)?;
        let starts = alignment.element_stations();
        let mut out = Vec::new();
        for (ri, r) in self.regions.iter().enumerate() {
            let (a, b) = (r.start_station_m.max(start), r.end_station_m.min(end));
            if b - a <= 1e-3 {
                continue;
            }
            let f = &r.frequency;
            let mut wanted: Vec<f64> = Vec::new();
            for (e, &s0) in alignment.elements.iter().zip(&starts) {
                let step = match e.kind() {
                    ElementKind::Line => f.tangent_m,
                    ElementKind::Arc => f.curve_m,
                    ElementKind::Spiral => f.spiral_m,
                };
                if let Some(step) = step {
                    let (lo, hi) = (s0.max(a), (s0 + e.length).min(b));
                    let mut k = (lo / step).floor() + 1.0;
                    while k * step < hi {
                        wanted.push(k * step);
                        k += 1.0;
                    }
                }
            }
            if f.at_geometry_points {
                wanted.extend(&starts);
                wanted.extend(profile.pvis.iter().map(|p| p.station_m));
                wanted.extend(curves.iter().flat_map(|c| [c.start_station_m, c.end_station_m]));
            }
            if f.at_superelevation_critical {
                if let Some(s) = &alignment.superelevation {
                    wanted.extend(s.sections.iter().flat_map(|x| x.points.iter().map(|p| p.station_m)));
                }
                wanted.extend(self.crossfall_table.iter().map(|p| p.station_m));
            }
            wanted.extend(&f.extra_stations);
            let mut stations = vec![a, b];
            for s in wanted {
                if s > a && s < b && stations.iter().all(|t| (t - s).abs() > 1e-3) {
                    stations.push(s);
                }
            }
            stations.sort_by(f64::total_cmp);
            out.extend(stations.into_iter().map(|s| (s, ri)));
        }
        Ok(out)
    }
}

/// A section of a corridor built from an assembly: the existing [`Section`] (finished surface,
/// earthworks line, ground, daylight, areas) and the design geometry with its codes.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSection {
    pub section: Section,
    pub design: DesignGeometry,
    /// Index of the region.
    pub region: usize,
}

/// The section of a baseline's region at an internal station.
pub fn design_section_at(baseline: &Baseline, region: usize, alignment: &Alignment, ground: &Tin, station_m: f64) -> Result<ModelSection, CorridorError> {
    let r = baseline.regions.get(region).ok_or_else(|| CorridorError::Definition(format!("baseline '{}' has no region {region}", baseline.name)))?;
    let at = alignment.point_at(station_m).ok_or(CorridorError::Station(station_m))?;
    let profile = alignment.profile.as_ref().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
    let centre_z = profile.elevation_at(station_m).map_err(|_| CorridorError::Station(station_m))?;
    let (start, end) = (alignment.start_station_m, alignment.end_station_m());
    let nudge = ((end - start) / 2.0).min(1e-6);
    let curvature = |s: f64| alignment.curvature_at(s.clamp(start, end)).unwrap_or(0.0);

    let run = ground_run(ground, at.x, at.y, at.heading_rad);
    let ctx = EvalContext {
        station_m,
        x: at.x,
        y: at.y,
        heading_rad: at.heading_rad,
        centre_z,
        cross_fall: baseline.cross_fall(alignment, station_m),
        ground: run.as_deref(),
    };
    let a = assemble(&r.assembly, &ctx).map_err(|e| CorridorError::Definition(format!("at station {station_m:.3}: {e}")))?;
    let design = a.design;

    let finished: Vec<SectionPoint> = design
        .points
        .iter()
        .filter(|p| !p.codes.iter().any(|c| c == "Daylight"))
        .map(|p| SectionPoint { offset_m: p.offset_m, elevation_m: p.elevation_m, code: p.codes.first().cloned().unwrap_or_default() })
        .collect();
    let mut section = Section {
        station_m,
        label: alignment.station_label(station_m, 3),
        region,
        x: at.x,
        y: at.y,
        heading_rad: at.heading_rad,
        curvature_back: curvature(station_m - nudge),
        curvature_ahead: curvature(station_m + nudge),
        finished,
        earthworks: design.earthworks.clone(),
        ground: Vec::new(),
        left_daylight: a.left_daylight,
        right_daylight: a.right_daylight,
        cut_area_m2: None,
        fill_area_m2: None,
        cut_centroid_offset_m: None,
        fill_centroid_offset_m: None,
        notes: a.notes,
    };
    match &run {
        None => {
            if !section.notes.iter().any(|n| n.contains("off the ground")) {
                section.notes.push("The centreline is off the ground surface.".to_string());
            }
        }
        Some(run) => {
            section.ground = trim_ground(run, &section.earthworks);
            let closed = |open: bool, d: Option<crate::section::DaylightPoint>| !open || d.is_some();
            if closed(a.left_open, a.left_daylight) && closed(a.right_open, a.right_daylight) && !section.ground.is_empty() {
                let areas = cut_fill_areas(&section.earthworks, &section.ground);
                section.cut_area_m2 = Some(areas.cut_m2);
                section.fill_area_m2 = Some(areas.fill_m2);
                section.cut_centroid_offset_m = (areas.cut_m2 > 1e-9).then(|| areas.cut_moment_m3 / areas.cut_m2);
                section.fill_centroid_offset_m = (areas.fill_m2 > 1e-9).then(|| areas.fill_moment_m3 / areas.fill_m2);
            }
        }
    }
    Ok(ModelSection { section, design, region })
}

/// A baseline as built.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineResult {
    pub name: String,
    pub alignment: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub sections: Vec<ModelSection>,
    pub notes: Vec<String>,
}

/// A corridor as built.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCorridor {
    pub name: String,
    pub baselines: Vec<BaselineResult>,
}

/// Builds every baseline of a model. Each baseline finds its alignment by name in `alignments`.
pub fn build_model(model: &CorridorModel, alignments: &[Alignment], ground: &Tin) -> Result<ModelCorridor, CorridorError> {
    if model.baselines.is_empty() {
        return Err(CorridorError::Definition("a corridor needs at least one baseline".into()));
    }
    for (what, v) in [("cut", model.volumes.cut_factor), ("fill", model.volumes.fill_factor)] {
        if !(v.is_finite() && v > 0.0) {
            return Err(CorridorError::Definition(format!("the {what} factor must be greater than zero (got {v})")));
        }
    }
    let mut out = Vec::new();
    for b in &model.baselines {
        let alignment = alignments
            .iter()
            .find(|a| a.name == b.alignment)
            .ok_or_else(|| CorridorError::Definition(format!("baseline '{}': there is no alignment named '{}'", b.name, b.alignment)))?;
        out.push(build_baseline(b, alignment, ground)?);
    }
    Ok(ModelCorridor { name: model.name.clone(), baselines: out })
}

/// Builds one baseline.
pub fn build_baseline(baseline: &Baseline, alignment: &Alignment, ground: &Tin) -> Result<BaselineResult, CorridorError> {
    let stations = baseline.stations(alignment)?;
    let (start, end) = baseline.range(alignment)?;
    let mut sections = Vec::with_capacity(stations.len());
    for (s, ri) in stations {
        sections.push(design_section_at(baseline, ri, alignment, ground, s)?);
    }
    let mut notes = Vec::new();
    let open = sections.iter().filter(|s| s.section.cut_area_m2.is_none()).count();
    if open > 0 {
        notes.push(format!("{open} of {} sections have no cut and fill areas because the ground is missing or a side slope does not reach it; see their notes.", sections.len()));
    }
    Ok(BaselineResult { name: baseline.name.clone(), alignment: alignment.name.clone(), start_station_m: start, end_station_m: end, sections, notes })
}

impl BaselineResult {
    /// The plain sections (for [`crate::section_drawing`] and the existing volume functions).
    pub fn plain_sections(&self) -> Vec<Section> {
        self.sections.iter().map(|s| s.section.clone()).collect()
    }
}

/// Earthworks of a built baseline against its ground. For [`VolumeMethod::Prismoidal`] a section
/// is built at the middle station of every interval.
pub fn baseline_earthworks(baseline: &Baseline, alignment: &Alignment, ground: &Tin, built: &BaselineResult, options: &QuantityOptions) -> Result<QuantityTable, CorridorError> {
    let ends: Vec<EndArea> = built.sections.iter().map(|s| EndArea::from_section(&s.section)).collect();
    let mut mids = Vec::new();
    if options.method == VolumeMethod::Prismoidal {
        for w in built.sections.windows(2) {
            let (a, b) = (&w[0].section, &w[1]);
            if b.section.station_m - a.station_m <= 0.0 {
                mids.push(None);
            } else {
                let mid = design_section_at(baseline, b.region, alignment, ground, (a.station_m + b.section.station_m) / 2.0)?;
                mids.push(Some(EndArea::from_section(&mid.section)));
            }
        }
    }
    Ok(earthwork_table(&ends, &mids, options))
}

// ─── surfaces and feature lines ─────────────────────────────────────────────────────────────

/// How a corridor surface is bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceBoundary {
    /// The surface runs between the outermost points with the codes.
    None,
    /// Triangles outside the ring of daylight points are removed (see [`daylight_boundary`]).
    Daylight,
}

fn plan_xyz(s: &Section, o: f64, z: f64) -> [f64; 3] {
    let (x, y) = s.plan_point(o);
    [x, y, z]
}

/// The points of the links with any of the codes in one section, ascending in offset, repeats
/// removed.
fn coded_points(s: &ModelSection, codes: &[&str]) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = s.design.links.iter().filter(|l| l.codes.iter().any(|c| codes.contains(&c.as_str()))).flat_map(|l| l.points.iter().copied()).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    v.dedup();
    v
}

fn area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
}

/// The plan ring of the daylight points: up the left side, down the right. Sections with no
/// daylight on a side are skipped on that side. Empty when fewer than three points.
pub fn daylight_boundary(built: &BaselineResult) -> Vec<(f64, f64)> {
    let side = |left: bool| -> Vec<(f64, f64)> {
        built
            .sections
            .iter()
            .filter_map(|m| {
                let d = if left { m.section.left_daylight } else { m.section.right_daylight }?;
                let p = m.section.plan_point(d.offset_m);
                Some(p)
            })
            .collect()
    };
    let mut ring = side(true);
    ring.extend(side(false).into_iter().rev());
    ring.dedup();
    if ring.len() < 3 {
        ring.clear();
    }
    ring
}

fn inside(ring: &[(f64, f64)], p: (f64, f64)) -> bool {
    let mut c = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            c = !c;
        }
    }
    c
}

/// A surface from the points of the links with any of the `codes` (`["Top"]`, `["Datum"]`,
/// `["Top", "Daylight"]`). Between each pair of consecutive sections the two rows of points, in
/// order of offset, are joined in strips of triangles; two sections at one station enclose
/// nothing. Where a vertical link is among them, the strip bridges it with a sloping face.
/// Triangles with no plan area are dropped.
pub fn corridor_surface(built: &BaselineResult, codes: &[&str], boundary: SurfaceBoundary) -> Result<Tin, String> {
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let rows: Vec<(f64, Vec<[f64; 3]>)> = built
        .sections
        .iter()
        .map(|m| (m.section.station_m, coded_points(m, codes).into_iter().map(|(o, z)| plan_xyz(&m.section, o, z)).collect()))
        .collect();
    let mut row_index: Vec<Vec<u32>> = Vec::new();
    for (_, r) in &rows {
        let start = points.len() as u32;
        points.extend(r.iter().copied());
        row_index.push((start..start + r.len() as u32).collect());
    }
    for k in 1..rows.len() {
        if rows[k].0 - rows[k - 1].0 <= 0.0 {
            continue;
        }
        let (a, b) = (&row_index[k - 1], &row_index[k]);
        if a.len() < 2 || b.len() < 2 {
            continue;
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() - 1 || j < b.len() - 1 {
            let advance_a = if i == a.len() - 1 {
                false
            } else if j == b.len() - 1 {
                true
            } else {
                (i + 1) as f64 / (a.len() - 1) as f64 <= (j + 1) as f64 / (b.len() - 1) as f64
            };
            let t = if advance_a {
                let t = [a[i], b[j], a[i + 1]];
                i += 1;
                t
            } else {
                let t = [a[i], b[j], b[j + 1]];
                j += 1;
                t
            };
            if area2(points[t[0] as usize], points[t[1] as usize], points[t[2] as usize]).abs() > 1e-9 {
                tris.push(t);
            }
        }
    }
    if boundary == SurfaceBoundary::Daylight {
        let ring = daylight_boundary(built);
        if ring.is_empty() {
            return Err("there are not enough daylight points to bound the surface".into());
        }
        tris.retain(|t| {
            let c = t.map(|i| points[i as usize]);
            inside(&ring, ((c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0))
        });
    }
    if tris.is_empty() {
        return Err(format!("no surface: the links with codes {codes:?} give no strip between two sections"));
    }
    Tin::new(points, tris).map_err(|e| e.to_string())
}

/// A line through the points with one code, station by station, in plan and level.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureLine {
    pub code: String,
    /// `None` for a point on the baseline.
    pub side: Option<Side>,
    pub stations_m: Vec<f64>,
    pub points: Vec<[f64; 3]>,
}

/// Feature lines of a point code: one per side (and one for the baseline), broken wherever a
/// section lacks the point. When a section has the code twice on a side the first, nearest the
/// baseline, is taken. A line of a single point is dropped.
pub fn feature_lines(built: &BaselineResult, code: &str) -> Vec<FeatureLine> {
    let mut out = Vec::new();
    for side in [Some(Side::Left), None, Some(Side::Right)] {
        let mut current: Option<FeatureLine> = None;
        let flush = |c: &mut Option<FeatureLine>, out: &mut Vec<FeatureLine>| {
            if let Some(l) = c.take() {
                if l.points.len() >= 2 {
                    out.push(l);
                }
            }
        };
        for (i, m) in built.sections.iter().enumerate() {
            let mut hits: Vec<_> = m.design.points_with_code(code).filter(|p| p.side == side).collect();
            hits.sort_by(|a, b| a.offset_m.abs().total_cmp(&b.offset_m.abs()));
            match hits.first() {
                Some(p) => {
                    // Two sections at one station (a region boundary) give one point.
                    if i > 0 && built.sections[i - 1].section.station_m == m.section.station_m {
                        continue;
                    }
                    let l = current.get_or_insert_with(|| FeatureLine { code: code.into(), side, stations_m: vec![], points: vec![] });
                    l.stations_m.push(m.section.station_m);
                    l.points.push(plan_xyz(&m.section, p.offset_m, p.elevation_m));
                }
                None => flush(&mut current, &mut out),
            }
        }
        flush(&mut current, &mut out);
    }
    out
}

impl ModelCorridor {
    /// Total cut and fill by average end area over every baseline, with the options' factors.
    pub fn average_end_area_totals(&self, options: &QuantityOptions) -> (f64, f64) {
        let opts = QuantityOptions { method: VolumeMethod::AverageEndArea, ..*options };
        self.baselines.iter().fold((0.0, 0.0), |acc, b| {
            let ends: Vec<EndArea> = b.sections.iter().map(|s| EndArea::from_section(&s.section)).collect();
            let t = earthwork_table(&ends, &[], &opts);
            (acc.0 + t.total_cut_m3, acc.1 + t.total_fill_m3)
        })
    }
}
