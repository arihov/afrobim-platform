// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Geometry outputs: plan, profile of a run and section crossings at a station.
//!
//! All coordinates are as given (metres, true UTM magnitude). Plan geometry is plain data for a
//! drawing or model writer; this crate draws nothing itself.

use afrobim_alignment::Alignment;
use serde::{Deserialize, Serialize};

use crate::error::PipesError;
use crate::ground::Ground;
use crate::model::*;

// ----------------------------------------------------------------------------- plan

/// Plan footprint of a structure (outer size: inner size plus the wall each side).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Footprint {
    /// No inner size is set: drawn as a point.
    Point,
    Circle { radius_m: f64 },
    /// Axis-aligned about the centre.
    Rectangle { length_m: f64, width_m: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStructure {
    pub id: StructureId,
    pub name: String,
    pub kind: StructureKind,
    pub easting_m: f64,
    pub northing_m: f64,
    pub footprint: Footprint,
    pub rim_level_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanPipe {
    pub id: PipeId,
    pub name: String,
    pub start: StructureId,
    pub end: StructureId,
    /// Centreline vertices `[easting, northing]`; a curved pipe has many.
    pub centreline: Vec<[f64; 2]>,
    /// Outside width in plan: inside width plus a wall each side.
    pub outer_width_m: f64,
    pub size_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanGeometry {
    pub structures: Vec<PlanStructure>,
    pub pipes: Vec<PlanPipe>,
}

// ----------------------------------------------------------------------------- profile

/// What the horizontal axis of a profile is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "axis", rename_all = "snake_case")]
pub enum ProfileAxis {
    /// Distance along the run's own plan path from its first structure, m.
    OwnPath,
    /// Station along an alignment, m, as the alignment's `station_offset` gives it.
    Alignment { name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileStructure {
    pub id: StructureId,
    pub name: String,
    /// Distance or station, m.
    pub x_m: f64,
    /// Offset from the alignment, m (positive right), when projected to one.
    pub offset_m: Option<f64>,
    pub rim_level_m: f64,
    pub sump_level_m: Option<f64>,
    pub ground_level_m: Option<f64>,
    /// Lowest invert of the pipes connected to it, m.
    pub lowest_invert_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilePipe {
    pub id: PipeId,
    pub name: String,
    pub size_label: String,
    pub wall_thickness_m: f64,
    /// `[x, invert level]` along the pipe; two points for a straight pipe.
    pub invert_line: Vec<[f64; 2]>,
    /// `[x, crown level]` at the same x values.
    pub crown_line: Vec<[f64; 2]>,
    pub start_invert_m: f64,
    pub end_invert_m: f64,
    pub start_crown_m: f64,
    pub end_crown_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileView {
    pub axis: ProfileAxis,
    pub structures: Vec<ProfileStructure>,
    pub pipes: Vec<ProfilePipe>,
    /// `[x, ground level]` along the run at the sampling spacing (empty without a ground).
    pub ground_line: Vec<[f64; 2]>,
}

// ----------------------------------------------------------------------------- section

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionCrossing {
    pub pipe: PipeId,
    pub name: String,
    pub size_label: String,
    /// Offset from the alignment along the section line, m (positive right).
    pub offset_m: f64,
    pub invert_m: f64,
    pub crown_m: f64,
    /// Outside bottom and top (invert minus wall, crown plus wall).
    pub outer_bottom_m: f64,
    pub outer_top_m: f64,
    pub inside_width_m: f64,
    pub inside_height_m: f64,
    pub ground_level_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionStructure {
    pub id: StructureId,
    pub name: String,
    /// Offset of the structure's centre projected on the section line, m.
    pub offset_m: f64,
    /// Distance from the structure's centre to the section line, m.
    pub distance_to_line_m: f64,
    pub outer_radius_m: f64,
    pub rim_level_m: f64,
    pub sump_level_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionView {
    pub station_m: f64,
    pub half_width_m: f64,
    pub crossings: Vec<SectionCrossing>,
    pub structures: Vec<SectionStructure>,
}

impl PipeNetwork {
    /// Plan geometry of the whole network.
    pub fn plan_geometry(&self) -> PlanGeometry {
        let structures = self
            .structures()
            .map(|s| {
                let wall = s.wall_thickness_m.unwrap_or(0.0);
                let footprint = match s.inner {
                    None => Footprint::Point,
                    Some(StructureInner::Circular { diameter_m }) => Footprint::Circle { radius_m: diameter_m / 2.0 + wall },
                    Some(StructureInner::Rectangular { length_m, width_m }) => Footprint::Rectangle { length_m: length_m + 2.0 * wall, width_m: width_m + 2.0 * wall },
                };
                PlanStructure { id: s.id, name: s.name.clone(), kind: s.kind, easting_m: s.easting_m, northing_m: s.northing_m, footprint, rim_level_m: s.rim_level_m }
            })
            .collect();
        let pipes = self
            .pipes()
            .filter_map(|p| {
                let path = self.pipe_path(p.id).ok()?;
                Some(PlanPipe { id: p.id, name: p.name.clone(), start: p.start, end: p.end, centreline: path.points, outer_width_m: p.shape.width_m() + 2.0 * p.wall_thickness_m.unwrap_or(0.0), size_label: p.shape.size_label() })
            })
            .collect();
        PlanGeometry { structures, pipes }
    }

    /// The run downstream from a structure: follow the pipe leaving each structure to an outlet.
    /// At a branch the pipe with the lowest id is followed. Stops (without error) if a pipe would
    /// revisit a structure.
    pub fn trace_run(&self, start: StructureId) -> Result<Vec<PipeId>, PipesError> {
        self.structure(start)?;
        let mut out = Vec::new();
        let mut at = start;
        let mut seen = vec![start];
        while let Some(p) = self.outgoing(at).first().copied() {
            let next = self.pipe(p)?.end;
            if seen.contains(&next) {
                break;
            }
            out.push(p);
            seen.push(next);
            at = next;
        }
        Ok(out)
    }

    fn check_consecutive(&self, pipes: &[PipeId]) -> Result<(), PipesError> {
        if pipes.is_empty() {
            return Err(PipesError::Invalid("a profile needs at least one pipe".to_string()));
        }
        for w in pipes.windows(2) {
            if self.pipe(w[0])?.end != self.pipe(w[1])?.start {
                return Err(PipesError::Invalid(format!("pipes {} and {} do not join end to start", w[0], w[1])));
            }
        }
        Ok(())
    }

    /// Profile of a run along its own plan path. `pipes` must join end to start (see
    /// [`PipeNetwork::trace_run`]). The first structure is at x = 0 and x is the plan distance
    /// along the pipes. The ground line is sampled every `ground_spacing_m` if a ground is given.
    pub fn run_profile(&self, pipes: &[PipeId], ground: Option<&dyn Ground>, ground_spacing_m: f64) -> Result<ProfileView, PipesError> {
        self.build_profile(pipes, ground, ground_spacing_m, None)
    }

    /// Profile of a run projected to an alignment: x is the station of each point of the pipes'
    /// plan paths (the foot of the perpendicular on the alignment). Errors if a structure has no
    /// perpendicular foot on the alignment.
    pub fn profile_on_alignment(&self, pipes: &[PipeId], alignment: &Alignment, ground: Option<&dyn Ground>, ground_spacing_m: f64) -> Result<ProfileView, PipesError> {
        self.build_profile(pipes, ground, ground_spacing_m, Some(alignment))
    }

    fn build_profile(&self, pipes: &[PipeId], ground: Option<&dyn Ground>, spacing: f64, alignment: Option<&Alignment>) -> Result<ProfileView, PipesError> {
        self.check_consecutive(pipes)?;
        let project = |pt: [f64; 2], base: f64| -> Result<(f64, Option<f64>), PipesError> {
            match alignment {
                None => Ok((base, None)),
                Some(a) => a
                    .station_offset(pt[0], pt[1])
                    .map(|(s, o)| (s, Some(o)))
                    .ok_or_else(|| PipesError::Invalid(format!("the point {:.3}, {:.3} has no perpendicular foot on the alignment", pt[0], pt[1]))),
            }
        };
        let mut structures = Vec::new();
        let mut prof_pipes = Vec::new();
        let mut ground_line = Vec::new();
        let mut base = 0.0;
        let first = self.pipe(pipes[0])?.start;
        let mut sids = vec![(first, 0.0)];
        for pid in pipes {
            let p = self.pipe(*pid)?;
            let path = self.pipe_path(*pid)?;
            let l = path.length_m;
            let wall = p.wall_thickness_m.unwrap_or(0.0);
            let mut inv = Vec::new();
            let mut crown = Vec::new();
            for (pt, c) in path.points.iter().zip(&path.chainage_m) {
                let t = if l > 0.0 { c / l } else { 0.0 };
                let level = p.start_invert_m + (p.end_invert_m - p.start_invert_m) * t;
                let (x, _) = project(*pt, base + c)?;
                inv.push([x, level]);
                crown.push([x, level + p.shape.height_m()]);
            }
            if let Some(g) = ground {
                let mut c = 0.0;
                loop {
                    let pt = path.point_at(c);
                    if let Some(gl) = g.level_m(pt[0], pt[1]) {
                        let (x, _) = project(pt, base + c)?;
                        ground_line.push([x, gl]);
                    }
                    if c >= l {
                        break;
                    }
                    c = if spacing > 0.0 { (c + spacing).min(l) } else { l };
                }
            }
            prof_pipes.push(ProfilePipe {
                id: p.id,
                name: p.name.clone(),
                size_label: p.shape.size_label(),
                wall_thickness_m: wall,
                invert_line: inv,
                crown_line: crown,
                start_invert_m: p.start_invert_m,
                end_invert_m: p.end_invert_m,
                start_crown_m: p.start_invert_m + p.shape.height_m(),
                end_crown_m: p.end_invert_m + p.shape.height_m(),
            });
            base += l;
            sids.push((p.end, base));
        }
        for (sid, chain) in sids {
            let s = self.structure(sid)?;
            let (x, off) = project([s.easting_m, s.northing_m], chain)?;
            let mut connected = self.incoming(sid);
            connected.extend(self.outgoing(sid));
            let lowest = connected
                .iter()
                .filter_map(|p| self.pipe(*p).ok().map(|p| if p.start == sid { p.start_invert_m } else { p.end_invert_m }))
                .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
            structures.push(ProfileStructure {
                id: sid,
                name: s.name.clone(),
                x_m: x,
                offset_m: off,
                rim_level_m: s.rim_level_m,
                sump_level_m: s.sump_level_m,
                ground_level_m: ground.and_then(|g| g.level_m(s.easting_m, s.northing_m)),
                lowest_invert_m: lowest,
            });
        }
        let axis = match alignment {
            None => ProfileAxis::OwnPath,
            Some(a) => ProfileAxis::Alignment { name: a.name.clone() },
        };
        Ok(ProfileView { axis, structures, pipes: prof_pipes, ground_line })
    }

    /// Where pipes and structures cross the section line at a station of an alignment: the line
    /// through the alignment point at `station_m`, perpendicular to the alignment there, from
    /// `-half_width_m` (left) to `+half_width_m` (right). The section line is straight, so
    /// offsets are along it, not along an offset curve.
    pub fn section_at_station(&self, alignment: &Alignment, station_m: f64, half_width_m: f64, ground: Option<&dyn Ground>) -> Result<SectionView, PipesError> {
        if !(half_width_m > 0.0) || !half_width_m.is_finite() {
            return Err(PipesError::Invalid("the section half width must be greater than zero".to_string()));
        }
        let a = alignment.point_at_offset(station_m, -half_width_m).map(|p| [p.0, p.1]).ok_or_else(|| PipesError::Invalid(format!("station {station_m} is not on the alignment")))?;
        let b = alignment.point_at_offset(station_m, half_width_m).map(|p| [p.0, p.1]).ok_or_else(|| PipesError::Invalid(format!("station {station_m} is not on the alignment")))?;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        let mut crossings = Vec::new();
        for p in self.pipes() {
            let path = self.pipe_path(p.id)?;
            let wall = p.wall_thickness_m.unwrap_or(0.0);
            for i in 1..path.points.len() {
                let (s0, s1) = (path.points[i - 1], path.points[i]);
                let (ex, ey) = (s1[0] - s0[0], s1[1] - s0[1]);
                let denom = ex * dy - ey * dx;
                if denom.abs() < 1e-14 * len * ex.hypot(ey).max(1e-300) {
                    continue;
                }
                // s0 + t e = a + u d
                let (rx, ry) = (a[0] - s0[0], a[1] - s0[1]);
                let t = (rx * dy - ry * dx) / denom;
                let u = (rx * ey - ry * ex) / denom;
                if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
                    continue;
                }
                // avoid listing the shared vertex of two chords twice
                if i > 1 && t == 0.0 {
                    continue;
                }
                let c = path.chainage_m[i - 1] + t * (path.chainage_m[i] - path.chainage_m[i - 1]);
                let frac = if path.length_m > 0.0 { c / path.length_m } else { 0.0 };
                let invert = p.start_invert_m + (p.end_invert_m - p.start_invert_m) * frac;
                let px = s0[0] + t * ex;
                let py = s0[1] + t * ey;
                crossings.push(SectionCrossing {
                    pipe: p.id,
                    name: p.name.clone(),
                    size_label: p.shape.size_label(),
                    offset_m: -half_width_m + u * 2.0 * half_width_m,
                    invert_m: invert,
                    crown_m: invert + p.shape.height_m(),
                    outer_bottom_m: invert - wall,
                    outer_top_m: invert + p.shape.height_m() + wall,
                    inside_width_m: p.shape.width_m(),
                    inside_height_m: p.shape.height_m(),
                    ground_level_m: ground.and_then(|g| g.level_m(px, py)),
                });
            }
        }
        crossings.sort_by(|x, y| x.offset_m.partial_cmp(&y.offset_m).unwrap());
        let mut structures = Vec::new();
        for s in self.structures() {
            let (rx, ry) = (s.easting_m - a[0], s.northing_m - a[1]);
            let along = (rx * dx + ry * dy) / len;
            let perp = (rx * dy - ry * dx) / len;
            let radius = s.inner.map(|i| i.bounding_radius_m()).unwrap_or(0.0) + s.wall_thickness_m.unwrap_or(0.0);
            if perp.abs() <= radius && along >= -radius && along <= len + radius {
                structures.push(SectionStructure {
                    id: s.id,
                    name: s.name.clone(),
                    offset_m: -half_width_m + along,
                    distance_to_line_m: perp.abs(),
                    outer_radius_m: radius,
                    rim_level_m: s.rim_level_m,
                    sump_level_m: s.sump_level_m,
                });
            }
        }
        Ok(SectionView { station_m, half_width_m, crossings, structures })
    }
}
