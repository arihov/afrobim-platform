// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Quantities: pipe length by size and material, structure count by type, and trench excavation.
//!
//! No rate, bedding thickness, trench width or depth band is built in: each is an input.
//!
//! # Trench excavation method
//!
//! Along each pipe, at both ends and every `station_spacing_m` of plan distance, the ground level
//! is taken from the ground surface. The trench bottom is `outer bottom - bedding thickness`,
//! with the outer bottom the invert minus the wall thickness (zero if not set). The trench depth
//! `d` at a station is the ground level minus the trench bottom (zero if negative: a pipe laid
//! above ground has no trench). The trench has a flat bottom of width `w` and sides of slope `z`
//! (horizontal per vertical; zero for vertical sides), so the cross-section at depth `d` is
//! `A = w d + z d^2` and, between depths `a` and `b` below ground (`b <= d`),
//!
//! `A(a, b) = w (b - a) + z [ 2 d (b - a) - (b^2 - a^2) ]`.
//!
//! The volume is the **average end area** between consecutive stations, `(A1 + A2)/2 x dx` with
//! `dx` the plan distance: the cross-sections are in vertical planes perpendicular to the plan
//! direction, so plan distance is the right spacing for a straight or curved pipe. It is exact
//! for vertical sides and linear ground, and approximate for sloped sides (the area varies as
//! `d^2`). Depth bands are the boundaries given, measured down from the ground; the last band is
//! open-ended.
//!
//! Not included: excavation for structures (pits at manholes), working space at structures,
//! benching, trench support, over-excavation, rock, de-watering, and any swell or compaction
//! factor. The bedding volume is the trench bottom width times the bedding thickness times plan
//! length (the pipe is not surrounded). Pipe displacement is the outside area times the sloping
//! length.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::csv;
use crate::finding::{Finding, Severity};
use crate::ground::Ground;
use crate::model::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeLengthRow {
    pub size: String,
    pub material: String,
    pub count: usize,
    /// Sum of plan lengths between structure centres, m.
    pub plan_length_m: f64,
    /// Sum of lengths along the sloping centreline, m.
    pub sloping_length_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureCountRow {
    pub kind: StructureKind,
    /// Part id, empty when the structures have none.
    pub part: String,
    pub count: usize,
}

/// Width of the trench bottom.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TrenchWidth {
    /// The same width for every pipe, m.
    Fixed { width_m: f64 },
    /// The pipe's outside width plus this clearance on each side, m.
    OutsideWidthPlus { each_side_m: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrenchSpec {
    pub width: TrenchWidth,
    /// Bedding below the outside bottom of the barrel, m (zero for none).
    pub bedding_thickness_m: f64,
    /// Side slope, horizontal per vertical (zero for vertical sides).
    pub side_slope: f64,
    /// Depth band boundaries below ground, m, increasing. `[1.5, 3.0]` gives 0-1.5, 1.5-3.0, over 3.0.
    pub depth_bands_m: Vec<f64>,
    /// Plan distance between stations, m.
    pub station_spacing_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandVolume {
    pub from_m: f64,
    /// `None` for the last band (no upper limit).
    pub to_m: Option<f64>,
    pub volume_m3: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrenchRow {
    pub pipe: PipeId,
    pub name: String,
    pub plan_length_m: f64,
    pub trench_width_m: f64,
    pub min_depth_m: f64,
    pub max_depth_m: f64,
    pub excavation_m3: f64,
    pub bands: Vec<BandVolume>,
    pub bedding_m3: f64,
    pub pipe_displacement_m3: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrenchQuantities {
    pub rows: Vec<TrenchRow>,
    pub band_totals: Vec<BandVolume>,
    pub total_excavation_m3: f64,
    pub total_bedding_m3: f64,
    pub total_pipe_displacement_m3: f64,
    pub method: String,
    pub findings: Vec<Finding>,
}

impl PipeNetwork {
    /// Pipe length by size and material.
    pub fn pipe_length_quantities(&self) -> Vec<PipeLengthRow> {
        let mut map: BTreeMap<(String, String), (usize, f64, f64)> = BTreeMap::new();
        for p in self.pipes() {
            let e = map.entry((p.shape.size_label(), p.material.clone().unwrap_or_default())).or_default();
            e.0 += 1;
            e.1 += self.plan_length_m(p.id).unwrap_or(0.0);
            e.2 += self.sloping_length_m(p.id).unwrap_or(0.0);
        }
        map.into_iter().map(|((size, material), (count, plan, slope))| PipeLengthRow { size, material, count, plan_length_m: plan, sloping_length_m: slope }).collect()
    }

    /// Structure count by kind and part.
    pub fn structure_count_quantities(&self) -> Vec<StructureCountRow> {
        let mut map: BTreeMap<(usize, String), usize> = BTreeMap::new();
        for s in self.structures() {
            let k = StructureKind::ALL.iter().position(|k| *k == s.kind).unwrap_or(0);
            *map.entry((k, s.part.clone().unwrap_or_default())).or_default() += 1;
        }
        map.into_iter().map(|((k, part), count)| StructureCountRow { kind: StructureKind::ALL[k], part, count }).collect()
    }

    pub fn pipe_length_csv(&self) -> String {
        let mut out = csv::row(&["size", "material", "count", "plan_length_m", "sloping_length_m"].map(String::from));
        for r in self.pipe_length_quantities() {
            out.push_str(&csv::row(&[r.size, r.material, r.count.to_string(), r.plan_length_m.to_string(), r.sloping_length_m.to_string()]));
        }
        out
    }

    pub fn structure_count_csv(&self) -> String {
        let mut out = csv::row(&["kind", "part", "count"].map(String::from));
        for r in self.structure_count_quantities() {
            out.push_str(&csv::row(&[r.kind.key().to_string(), r.part, r.count.to_string()]));
        }
        out
    }

    /// Trench excavation, bedding and pipe displacement by the method in the module documentation.
    pub fn trench_quantities(&self, ground: &dyn Ground, spec: &TrenchSpec) -> Result<TrenchQuantities, crate::error::PipesError> {
        use crate::error::PipesError;
        let bad = |m: &str| Err(PipesError::Invalid(m.to_string()));
        if !(spec.station_spacing_m > 0.0) || !spec.station_spacing_m.is_finite() {
            return bad("the station spacing must be greater than zero");
        }
        if !(spec.bedding_thickness_m >= 0.0) || !(spec.side_slope >= 0.0) {
            return bad("the bedding thickness and side slope must not be negative");
        }
        if spec.depth_bands_m.windows(2).any(|w| w[1] <= w[0]) || spec.depth_bands_m.iter().any(|b| !(*b > 0.0)) {
            return bad("depth band boundaries must be greater than zero and increasing");
        }
        match spec.width {
            TrenchWidth::Fixed { width_m } if !(width_m > 0.0) => return bad("the trench width must be greater than zero"),
            TrenchWidth::OutsideWidthPlus { each_side_m } if !(each_side_m >= 0.0) => return bad("the trench clearance must not be negative"),
            _ => {}
        }
        let mut edges = vec![0.0];
        edges.extend(spec.depth_bands_m.iter().copied());
        let band_defs: Vec<(f64, Option<f64>)> = (0..edges.len()).map(|i| (edges[i], edges.get(i + 1).copied())).collect();
        let mut rows = Vec::new();
        let mut findings = Vec::new();
        let mut totals: Vec<f64> = vec![0.0; band_defs.len()];
        let (mut t_exc, mut t_bed, mut t_pipe) = (0.0, 0.0, 0.0);
        for p in self.pipes() {
            let el = vec![ElementRef::Pipe(p.id)];
            let path = self.pipe_path(p.id)?;
            let l = path.length_m;
            let wall = p.wall_thickness_m.unwrap_or(0.0);
            let w = match spec.width {
                TrenchWidth::Fixed { width_m } => width_m,
                TrenchWidth::OutsideWidthPlus { each_side_m } => p.shape.width_m() + 2.0 * wall + 2.0 * each_side_m,
            };
            let mut stations = vec![0.0];
            let mut c = spec.station_spacing_m;
            while c < l {
                stations.push(c);
                c += spec.station_spacing_m;
            }
            stations.push(l);
            // depth at each station
            let mut depths = Vec::new();
            let mut missing = false;
            for c in &stations {
                let pt = path.point_at(*c);
                let t = if l > 0.0 { c / l } else { 0.0 };
                let bottom = p.start_invert_m + (p.end_invert_m - p.start_invert_m) * t - wall - spec.bedding_thickness_m;
                match ground.level_m(pt[0], pt[1]) {
                    Some(g) => depths.push((g - bottom).max(0.0)),
                    None => {
                        missing = true;
                        depths.push(0.0);
                    }
                }
            }
            if missing {
                findings.push(Finding::new("quantity", Severity::Warning, el.clone(), format!("pipe {} runs off the ground surface; the trench there is taken as zero depth", p.name)));
            }
            let area = |d: f64, a: f64, b: Option<f64>| -> f64 {
                let hi = b.map_or(d, |b| b.min(d));
                if hi <= a {
                    return 0.0;
                }
                w * (hi - a) + spec.side_slope * (2.0 * d * (hi - a) - (hi * hi - a * a))
            };
            let mut bands: Vec<f64> = vec![0.0; band_defs.len()];
            let mut total = 0.0;
            for i in 1..stations.len() {
                let dx = stations[i] - stations[i - 1];
                let (d0, d1) = (depths[i - 1], depths[i]);
                total += 0.5 * (area(d0, 0.0, None) + area(d1, 0.0, None)) * dx;
                for (k, (a, b)) in band_defs.iter().enumerate() {
                    bands[k] += 0.5 * (area(d0, *a, *b) + area(d1, *a, *b)) * dx;
                }
            }
            for (k, v) in bands.iter().enumerate() {
                totals[k] += v;
            }
            // bedding: bottom width times thickness, over plan length, limited by the trench depth
            let bedding: f64 = (1..stations.len()).map(|i| 0.5 * (depths[i - 1].min(spec.bedding_thickness_m) + depths[i].min(spec.bedding_thickness_m)) * w * (stations[i] - stations[i - 1])).sum();
            let displacement = outer_area_m2(p) * self.sloping_length_m(p.id)?;
            t_exc += total;
            t_bed += bedding;
            t_pipe += displacement;
            rows.push(TrenchRow {
                pipe: p.id,
                name: p.name.clone(),
                plan_length_m: l,
                trench_width_m: w,
                min_depth_m: depths.iter().copied().fold(f64::INFINITY, f64::min),
                max_depth_m: depths.iter().copied().fold(0.0, f64::max),
                excavation_m3: total,
                bands: band_defs.iter().zip(&bands).map(|((a, b), v)| BandVolume { from_m: *a, to_m: *b, volume_m3: *v }).collect(),
                bedding_m3: bedding,
                pipe_displacement_m3: displacement,
            });
        }
        let band_totals = band_defs.iter().zip(&totals).map(|((a, b), v)| BandVolume { from_m: *a, to_m: *b, volume_m3: *v }).collect();
        Ok(TrenchQuantities {
            rows,
            band_totals,
            total_excavation_m3: t_exc,
            total_bedding_m3: t_bed,
            total_pipe_displacement_m3: t_pipe,
            method: "average end area on plan distance at the station spacing, flat-bottom trench with side slope z: A = w d + z d^2; depth bands measured down from ground; no structure pits, working space, support, swell or compaction".to_string(),
            findings,
        })
    }
}

/// Outside cross-section area of a pipe, m2: inside area plus the perimeter times the wall plus
/// the corner terms (`pi t^2` for a round outline, `4 t^2` for a box).
pub fn outer_area_m2(p: &Pipe) -> f64 {
    let t = p.wall_thickness_m.unwrap_or(0.0);
    let (Some(a), Some(per)) = (p.shape.full_area_m2(), p.shape.full_perimeter_m()) else { return 0.0 };
    let corner = if matches!(p.shape, PipeShape::Box { .. }) { 4.0 * t * t } else { std::f64::consts::PI * t * t };
    a + per * t + corner
}

impl TrenchQuantities {
    pub fn to_csv(&self) -> String {
        let mut out = csv::row(&["pipe", "plan_length_m", "trench_width_m", "min_depth_m", "max_depth_m", "excavation_m3", "bedding_m3", "pipe_displacement_m3"].map(String::from));
        for r in &self.rows {
            out.push_str(&csv::row(&[r.name.clone(), r.plan_length_m.to_string(), r.trench_width_m.to_string(), r.min_depth_m.to_string(), r.max_depth_m.to_string(), r.excavation_m3.to_string(), r.bedding_m3.to_string(), r.pipe_displacement_m3.to_string()]));
        }
        out.push('\n');
        out.push_str(&csv::row(&["band_from_m", "band_to_m", "volume_m3"].map(String::from)));
        for b in &self.band_totals {
            out.push_str(&csv::row(&[b.from_m.to_string(), b.to_m.map(|v| v.to_string()).unwrap_or_default(), b.volume_m3.to_string()]));
        }
        out
    }
}
