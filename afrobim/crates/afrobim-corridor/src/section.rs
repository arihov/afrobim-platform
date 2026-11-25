// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Cross-sections: the template set on the profile at a station, the ground under it, the
//! daylight points and the cut and fill areas.
//!
//! A section is drawn looking up-station. **Offsets are metres square to the alignment, positive
//! to the right and negative to the left**; elevations are metres. Every polyline in a section
//! is listed from left to right.

use afrobim_alignment::Alignment;

use crate::template::{CorridorDefinition, Daylight, Template, TemplateSide};
use crate::tin::Tin;

/// Why a corridor could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CorridorError {
    #[error("the corridor definition is not valid: {0}")]
    Definition(String),
    #[error("alignment '{0}' has no vertical profile; a corridor needs one for its centreline levels")]
    NoProfile(String),
    #[error("the profile of alignment '{0}' is not valid: {1}")]
    Profile(String, String),
    #[error("there is no length to build: the alignment, its profile and the entered limits overlap from {start_m:.3} to {end_m:.3}")]
    NoLength { start_m: f64, end_m: f64 },
    #[error("station {0:.3} is not on the alignment and its profile")]
    Station(f64),
    #[error("{0}")]
    Document(String),
}

/// A point of the finished surface.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionPoint {
    pub offset_m: f64,
    pub elevation_m: f64,
    /// The code of the link that ends here; `CL` on the centreline.
    pub code: String,
}

/// Which side slope reached the ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlopeKind {
    Cut,
    Fill,
    /// The outer end of the template is on the ground: there is no slope.
    AtGround,
}

/// Where a side slope meets the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DaylightPoint {
    pub offset_m: f64,
    pub elevation_m: f64,
    pub kind: SlopeKind,
}

/// One cross-section. See the module documentation for the conventions.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Internal station, m.
    pub station_m: f64,
    /// The displayed station, as `1+250.000`.
    pub label: String,
    /// Index of the template region used.
    pub region: usize,
    /// Plan position of the centreline point, m, and the heading there (radians
    /// counter-clockwise from east).
    pub x: f64,
    pub y: f64,
    pub heading_rad: f64,
    /// Horizontal curvature just behind and just ahead of the station, 1/m, positive turning left.
    pub curvature_back: f64,
    pub curvature_ahead: f64,
    /// The finished surface from the left outer end, through the centreline, to the right outer end.
    pub finished: Vec<SectionPoint>,
    /// The line earthworks are measured to: left daylight point, left side slope, the formation
    /// (the finished surface, lowered by the formation depth under paved links, with a vertical
    /// step where paved meets unpaved), right side slope, right daylight point. On a side with
    /// no daylight it stops at the outer end of the template.
    pub earthworks: Vec<(f64, f64)>,
    /// The ground along the section as (offset, elevation), over the width of the section and a
    /// margin of a quarter of it (at least 1 m) each side. Empty when the centreline is off the
    /// surface.
    pub ground: Vec<(f64, f64)>,
    pub left_daylight: Option<DaylightPoint>,
    pub right_daylight: Option<DaylightPoint>,
    /// Area where the ground is above the earthworks line, m². `None` unless both sides daylight.
    pub cut_area_m2: Option<f64>,
    /// Area where the ground is below the earthworks line, m². `None` unless both sides daylight.
    pub fill_area_m2: Option<f64>,
    /// Offset of the centroid of the cut area, m; `None` when there is none (under 1e-9 m²).
    pub cut_centroid_offset_m: Option<f64>,
    pub fill_centroid_offset_m: Option<f64>,
    /// What went wrong at this section: no ground, no daylight.
    pub notes: Vec<String>,
}

impl Section {
    /// The plan position of a point of the section.
    pub fn plan_point(&self, offset_m: f64) -> (f64, f64) {
        (self.x + offset_m * self.heading_rad.sin(), self.y - offset_m * self.heading_rad.cos())
    }

    /// Elevation of the finished surface on the centreline.
    pub fn centre_elevation_m(&self) -> f64 {
        self.finished.iter().find(|p| p.code == "CL").map_or(f64::NAN, |p| p.elevation_m)
    }

    /// Ground elevation at an offset, inside the stored ground line.
    pub fn ground_at(&self, offset_m: f64) -> Option<f64> {
        interpolate(&self.ground, offset_m)
    }
}

/// Elevation of a polyline (ascending offsets) at an offset; `None` outside it.
pub(crate) fn interpolate(line: &[(f64, f64)], o: f64) -> Option<f64> {
    const TOL: f64 = 1e-7;
    let (first, last) = (line.first()?, line.last()?);
    if o < first.0 - TOL || o > last.0 + TOL {
        return None;
    }
    let i = line.partition_point(|p| p.0 <= o).clamp(1, line.len() - 1);
    let (p, q) = (line[i - 1], line[i]);
    if q.0 == p.0 {
        return Some(p.1);
    }
    Some(p.1 + (q.1 - p.1) * ((o - p.0) / (q.0 - p.0)).clamp(0.0, 1.0))
}

/// The ground along the line square to the baseline through (x, y), as (offset, level) from the
/// surface's far left to its far right, for the run that holds the centreline point. `None` when
/// the centreline point is off the surface.
pub(crate) fn ground_run(ground: &Tin, x: f64, y: f64, heading_rad: f64) -> Option<Vec<(f64, f64)>> {
    let plan_point = |o: f64| (x + o * heading_rad.sin(), y - o * heading_rad.cos());
    // Far enough each way to reach past the surface from wherever the station is.
    let b = ground.bounds();
    let reach = [(b.min_x, b.min_y), (b.min_x, b.max_y), (b.max_x, b.min_y), (b.max_x, b.max_y)]
        .iter()
        .map(|c| (c.0 - x).hypot(c.1 - y))
        .fold(0.0, f64::max)
        + 1.0;
    let runs = ground.section(plan_point(-reach), plan_point(reach));
    let run = runs.into_iter().find(|r| r[0].0 - reach <= 1e-7 && r[r.len() - 1].0 - reach >= -1e-7)?;
    Some(run.into_iter().map(|(d, z)| (d - reach, z)).collect())
}

/// Keeps the ground over the earthworks line and a margin of a quarter of its width (at least 1 m)
/// each side.
pub(crate) fn trim_ground(run: &[(f64, f64)], earthworks: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (Some(first), Some(last)) = (earthworks.first(), earthworks.last()) else { return out };
    let (lo, hi) = (first.0, last.0);
    let margin = ((hi - lo) / 4.0).max(1.0);
    let (lo, hi) = ((lo - margin).max(run[0].0), (hi + margin).min(run[run.len() - 1].0));
    if let (Some(z_lo), Some(z_hi)) = (interpolate(run, lo), interpolate(run, hi)) {
        out.push((lo, z_lo));
        out.extend(run.iter().filter(|p| p.0 > lo && p.0 < hi));
        out.push((hi, z_hi));
    }
    out
}

/// Area of a closed polygon by the shoelace formula, `½ |Σ (xᵢ yᵢ₊₁ − xᵢ₊₁ yᵢ)|`.
pub fn polygon_area(points: &[(f64, f64)]) -> f64 {
    let n = points.len();
    (0..n).map(|i| points[i].0 * points[(i + 1) % n].1 - points[(i + 1) % n].0 * points[i].1).sum::<f64>().abs() / 2.0
}

/// Cut and fill between two lines of a section.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CutFill {
    /// Area where the ground is above the earthworks line, m².
    pub cut_m2: f64,
    /// Area where the ground is below it, m².
    pub fill_m2: f64,
    /// First moments of the two areas about offset 0, m³ (area × centroid offset).
    pub cut_moment_m3: f64,
    pub fill_moment_m3: f64,
}

/// Cut and fill areas between an earthworks line and a ground line, over the width they share.
///
/// Both lines are (offset, elevation) with offsets that never decrease; the earthworks line may
/// have vertical steps (two points at one offset). The width is cut into strips at every vertex
/// of either line, and again wherever the lines cross inside a strip, so each piece is a
/// trapezium or a triangle lying wholly in cut or wholly in fill. With `d = ground − earthworks`
/// at the two ends of a piece of width `w`, its area is `w (d₀ + d₁) / 2`, which is the shoelace
/// formula applied to its four corners. However many times the lines cross, every crossing
/// starts a new piece.
pub fn cut_fill_areas(earthworks: &[(f64, f64)], ground: &[(f64, f64)]) -> CutFill {
    let mut out = CutFill::default();
    let (Some(e0), Some(e1), Some(g0), Some(g1)) = (earthworks.first(), earthworks.last(), ground.first(), ground.last()) else {
        return out;
    };
    let (lo, hi) = (e0.0.max(g0.0), e1.0.min(g1.0));
    let mut cuts: Vec<f64> = earthworks.iter().chain(ground).map(|p| p.0).filter(|o| *o > lo && *o < hi).collect();
    cuts.push(lo);
    cuts.push(hi);
    cuts.sort_by(f64::total_cmp);
    // The line through the piece of `line` over the strip (a, b), evaluated at a and b.
    let ends = |line: &[(f64, f64)], a: f64, b: f64| {
        let mid = (a + b) / 2.0;
        let i = line.partition_point(|p| p.0 <= mid).clamp(1, line.len() - 1);
        let (p, q) = (line[i - 1], line[i]);
        let k = (q.1 - p.1) / (q.0 - p.0);
        (p.1 + k * (a - p.0), p.1 + k * (b - p.0))
    };
    let mut add = |a: f64, b: f64, d0: f64, d1: f64| {
        let w = b - a;
        let area = w * (d0 + d1) / 2.0;
        let moment = w * (a * (2.0 * d0 + d1) + b * (d0 + 2.0 * d1)) / 6.0;
        if area > 0.0 {
            out.cut_m2 += area;
            out.cut_moment_m3 += moment;
        } else {
            out.fill_m2 -= area;
            out.fill_moment_m3 -= moment;
        }
    };
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a <= 1e-12 {
            continue;
        }
        let (ea, eb) = ends(earthworks, a, b);
        let (ga, gb) = ends(ground, a, b);
        let (d0, d1) = (ga - ea, gb - eb);
        if d0 * d1 < 0.0 {
            let c = a + (b - a) * d0 / (d0 - d1);
            add(a, c, d0, 0.0);
            add(c, b, 0.0, d1);
        } else {
            add(a, b, d0, d1);
        }
    }
    out
}

/// Where the side slope from `hinge` meets the ground, going outward (`side` −1 left, +1 right).
fn daylight(ground: &[(f64, f64)], hinge: (f64, f64), side: f64, rule: &Daylight) -> Result<DaylightPoint, String> {
    let g = interpolate(ground, hinge.0).ok_or("the outer end of the template is off the ground surface")?;
    let d = hinge.1 - g;
    if d.abs() <= 1e-9 {
        return Ok(DaylightPoint { offset_m: hinge.0, elevation_m: hinge.1, kind: SlopeKind::AtGround });
    }
    // Above the ground: fill, falling outward. Below it: cut, rising outward.
    let (kind, rate) = if d > 0.0 { (SlopeKind::Fill, -1.0 / rule.fill_run_per_rise) } else { (SlopeKind::Cut, 1.0 / rule.cut_run_per_rise) };
    let gap = |p: (f64, f64)| hinge.1 + rate * (p.0 - hinge.0).abs() - p.1;
    let mut prev = (hinge.0, g);
    let outward: Box<dyn Iterator<Item = &(f64, f64)>> =
        if side > 0.0 { Box::new(ground.iter().filter(|p| p.0 > hinge.0)) } else { Box::new(ground.iter().rev().filter(|p| p.0 < hinge.0)) };
    for &v in outward {
        let (f0, f1) = (gap(prev), gap(v));
        if f1 == 0.0 || (f1 > 0.0) != (d > 0.0) {
            let t = f0 / (f0 - f1);
            return Ok(DaylightPoint { offset_m: prev.0 + t * (v.0 - prev.0), elevation_m: prev.1 + t * (v.1 - prev.1), kind });
        }
        prev = v;
    }
    Err(format!("the {} slope leaves the ground surface before meeting it", if kind == SlopeKind::Fill { "fill" } else { "cut" }))
}

/// The finished points of one side going outward from the centreline (not including it), and the
/// formation line of that side going outward from the centreline (including it).
fn side_lines(side: &TemplateSide, sign: f64, centre_z: f64, cross_fall_pct: Option<f64>, depth: f64) -> (Vec<SectionPoint>, Vec<(f64, f64)>) {
    let mut finished = Vec::with_capacity(side.links.len());
    let mut formation = Vec::with_capacity(2 * side.links.len() + 1);
    let (mut o, mut z) = (0.0, centre_z);
    for l in &side.links {
        let slope = match cross_fall_pct {
            Some(p) if l.follows_superelevation => p / 100.0,
            _ => l.slope.rise_per_m(),
        };
        let drop = if l.paved { depth } else { 0.0 };
        formation.push((o, z - drop));
        o += sign * l.width_m;
        z += slope * l.width_m;
        formation.push((o, z - drop));
        finished.push(SectionPoint { offset_m: o, elevation_m: z, code: l.code.clone() });
    }
    // The side slope starts from the finished surface.
    formation.push((o, z));
    formation.dedup();
    (finished, formation)
}

/// The section of `template` at an internal station.
///
/// The centreline level is the alignment's profile at the station. A link that follows
/// superelevation takes the cross-fall stored on the alignment for its side, where one is
/// stored. The ground is cut along the line square to the alignment through the station, as far
/// as the surface goes. An error only when the station is off the alignment or its profile; a
/// section with no ground or no daylight is returned with `notes` saying so and no areas.
pub fn section_at(template: &Template, region: usize, alignment: &Alignment, ground: &Tin, station_m: f64) -> Result<Section, CorridorError> {
    let at = alignment.point_at(station_m).ok_or(CorridorError::Station(station_m))?;
    let profile = alignment.profile.as_ref().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
    let centre_z = profile.elevation_at(station_m).map_err(|_| CorridorError::Station(station_m))?;
    let fall = alignment.superelevation.as_ref().map(|s| s.cross_fall_at(station_m)).unwrap_or_default();
    let (start, end) = (alignment.start_station_m, alignment.end_station_m());
    let nudge = ((end - start) / 2.0).min(1e-6);
    let curvature = |s: f64| alignment.curvature_at(s.clamp(start, end)).unwrap_or(0.0);

    let (left, left_formation) = side_lines(&template.left, -1.0, centre_z, fall.left_pct, template.formation_depth_m);
    let (right, right_formation) = side_lines(&template.right, 1.0, centre_z, fall.right_pct, template.formation_depth_m);
    let mut finished: Vec<SectionPoint> = left.into_iter().rev().collect();
    finished.push(SectionPoint { offset_m: 0.0, elevation_m: centre_z, code: "CL".to_string() });
    finished.extend(right);
    let mut earthworks: Vec<(f64, f64)> = left_formation.into_iter().rev().collect();
    earthworks.extend(right_formation);
    earthworks.dedup();

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
        earthworks,
        ground: Vec::new(),
        left_daylight: None,
        right_daylight: None,
        cut_area_m2: None,
        fill_area_m2: None,
        cut_centroid_offset_m: None,
        fill_centroid_offset_m: None,
        notes: Vec::new(),
    };

    let Some(run) = ground_run(ground, at.x, at.y, at.heading_rad) else {
        section.notes.push("The centreline is off the ground surface.".to_string());
        return Ok(section);
    };

    let (hinge_l, hinge_r) = (section.finished[0].clone(), section.finished[section.finished.len() - 1].clone());
    match daylight(&run, (hinge_l.offset_m, hinge_l.elevation_m), -1.0, &template.left.daylight) {
        Ok(p) => section.left_daylight = Some(p),
        Err(why) => section.notes.push(format!("No daylight on the left: {why}.")),
    }
    match daylight(&run, (hinge_r.offset_m, hinge_r.elevation_m), 1.0, &template.right.daylight) {
        Ok(p) => section.right_daylight = Some(p),
        Err(why) => section.notes.push(format!("No daylight on the right: {why}.")),
    }
    if let Some(p) = section.left_daylight.filter(|p| p.kind != SlopeKind::AtGround) {
        section.earthworks.insert(0, (p.offset_m, p.elevation_m));
    }
    if let Some(p) = section.right_daylight.filter(|p| p.kind != SlopeKind::AtGround) {
        section.earthworks.push((p.offset_m, p.elevation_m));
    }

    section.ground = trim_ground(&run, &section.earthworks);

    if section.left_daylight.is_some() && section.right_daylight.is_some() {
        let a = cut_fill_areas(&section.earthworks, &section.ground);
        section.cut_area_m2 = Some(a.cut_m2);
        section.fill_area_m2 = Some(a.fill_m2);
        section.cut_centroid_offset_m = (a.cut_m2 > 1e-9).then(|| a.cut_moment_m3 / a.cut_m2);
        section.fill_centroid_offset_m = (a.fill_m2 > 1e-9).then(|| a.fill_moment_m3 / a.fill_m2);
    }
    Ok(section)
}

/// A corridor as built: its sections in station order.
#[derive(Debug, Clone, PartialEq)]
pub struct Corridor {
    pub name: String,
    /// The internal stations the corridor runs between.
    pub start_station_m: f64,
    pub end_station_m: f64,
    /// In station order. At a template region boundary there are **two** sections at the same
    /// station, the first with the template behind and the second with the template ahead.
    pub sections: Vec<Section>,
    /// What the builder has to say: limits applied, how many sections have no daylight.
    pub notes: Vec<String>,
}

impl Corridor {
    /// Builds the sections of a corridor.
    ///
    /// Sections stand at every whole multiple of the interval, at the start and end, at the
    /// start of every horizontal element, at every PVI and the start and end of every vertical
    /// curve, at every station where the alignment stores a cross-fall, and at every template
    /// region boundary. Stations within 1 mm of one already taken are left out.
    pub fn build(definition: &CorridorDefinition, alignment: &Alignment, ground: &Tin) -> Result<Corridor, CorridorError> {
        definition.check().map_err(CorridorError::Definition)?;
        let profile = alignment.profile.as_ref().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
        let curves = profile.curve_extents().map_err(|e| CorridorError::Profile(alignment.name.clone(), e.to_string()))?;
        let (p0, p1) = profile.range().ok_or_else(|| CorridorError::NoProfile(alignment.name.clone()))?;
        let mut notes = Vec::new();
        let (a0, a1) = (alignment.start_station_m.max(p0), alignment.end_station_m().min(p1));
        let start = definition.start_station_m.map_or(a0, |s| s.max(a0));
        let end = definition.end_station_m.map_or(a1, |s| s.min(a1));
        if definition.start_station_m.is_some_and(|s| s < a0) || definition.end_station_m.is_some_and(|s| s > a1) {
            notes.push(format!("The entered limits reach beyond the alignment and its profile; the corridor runs from {start:.3} to {end:.3}."));
        }
        if end - start <= 1e-3 || (end - start).is_nan() {
            return Err(CorridorError::NoLength { start_m: start, end_m: end });
        }

        // Region boundaries inside the corridor come first so that they win over near stations.
        let boundaries: Vec<f64> = definition.regions.iter().skip(1).map(|r| r.start_station_m).filter(|s| *s > start + 1e-3 && *s < end - 1e-3).collect();
        let mut stations = vec![start, end];
        stations.extend(&boundaries);
        let mut wanted: Vec<f64> = Vec::new();
        let step = definition.interval_m;
        let mut k = (start / step).floor() + 1.0;
        while k * step < end {
            wanted.push(k * step);
            k += 1.0;
        }
        wanted.extend(alignment.element_stations());
        wanted.extend(profile.pvis.iter().map(|p| p.station_m));
        wanted.extend(curves.iter().flat_map(|c| [c.start_station_m, c.end_station_m]));
        if let Some(s) = &alignment.superelevation {
            wanted.extend(s.sections.iter().flat_map(|x| x.points.iter().map(|p| p.station_m)));
        }
        for s in wanted {
            if s > start && s < end && stations.iter().all(|t| (t - s).abs() > 1e-3) {
                stations.push(s);
            }
        }
        stations.sort_by(f64::total_cmp);

        let region_of = |s: f64| definition.regions.iter().rposition(|r| r.start_station_m <= s).unwrap_or(0);
        let mut sections = Vec::with_capacity(stations.len() + boundaries.len());
        for s in stations {
            let region = region_of(s);
            if boundaries.contains(&s) {
                sections.push(section_at(&definition.regions[region - 1].template, region - 1, alignment, ground, s)?);
            }
            sections.push(section_at(&definition.regions[region].template, region, alignment, ground, s)?);
        }
        let open = sections.iter().filter(|s| s.cut_area_m2.is_none()).count();
        if open > 0 {
            notes.push(format!(
                "{open} of {} sections have no cut and fill areas because the ground is missing or a side slope does not reach it; see their notes.",
                sections.len()
            ));
        }
        Ok(Corridor { name: definition.name.clone(), start_station_m: start, end_station_m: end, sections, notes })
    }
}
