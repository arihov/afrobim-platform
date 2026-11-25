// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Structure and pipe tables, with CSV export.

use serde::{Deserialize, Serialize};

use crate::csv;
use crate::model::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureRow {
    pub id: StructureId,
    pub name: String,
    pub kind: StructureKind,
    pub easting_m: f64,
    pub northing_m: f64,
    pub rim_level_m: f64,
    pub sump_level_m: Option<f64>,
    /// Inner size as text (`1.200 m dia`, `1.500 x 1.000 m`), empty when not set.
    pub inner_size: String,
    pub wall_thickness_m: Option<f64>,
    pub part: Option<String>,
    pub pipes_in: Vec<String>,
    pub pipes_out: Vec<String>,
    /// Lowest invert of the connected pipes at this structure, m.
    pub lowest_invert_m: Option<f64>,
    /// Rim level minus the lowest invert, m.
    pub depth_to_lowest_invert_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeRow {
    pub id: PipeId,
    pub name: String,
    pub start: String,
    pub end: String,
    pub size: String,
    pub width_m: f64,
    pub height_m: f64,
    pub material: Option<String>,
    pub wall_thickness_m: Option<f64>,
    pub part: Option<String>,
    pub roughness: String,
    /// Between structure centres, along the plan path.
    pub plan_length_m: f64,
    /// Along the sloping centreline.
    pub sloping_length_m: f64,
    pub start_invert_m: f64,
    pub end_invert_m: f64,
    pub start_crown_m: f64,
    pub end_crown_m: f64,
    /// Fall over plan length, m/m (negative when the pipe rises).
    pub slope: f64,
    /// Design flow: the pipe's own, or accumulated down the network; empty on a loop.
    pub flow_m3_s: Option<f64>,
}

fn roughness_text(r: Option<Roughness>) -> String {
    match r {
        Some(Roughness::Manning { n }) => format!("Manning n {n}"),
        Some(Roughness::ColebrookWhite { ks_m }) => format!("Colebrook-White ks {ks_m} m"),
        None => String::new(),
    }
}

fn inner_text(i: Option<StructureInner>) -> String {
    match i {
        None => String::new(),
        Some(StructureInner::Circular { diameter_m }) => format!("{diameter_m:.3} m dia"),
        Some(StructureInner::Rectangular { length_m, width_m }) => format!("{length_m:.3} x {width_m:.3} m"),
    }
}

impl PipeNetwork {
    pub fn structure_table(&self) -> Vec<StructureRow> {
        self.structures()
            .map(|s| {
                let ins = self.incoming(s.id);
                let outs = self.outgoing(s.id);
                let name_of = |ids: &[PipeId]| ids.iter().filter_map(|i| self.pipe(*i).ok().map(|p| p.name.clone())).collect::<Vec<_>>();
                let lowest = ins
                    .iter()
                    .filter_map(|i| self.pipe(*i).ok().map(|p| p.end_invert_m))
                    .chain(outs.iter().filter_map(|o| self.pipe(*o).ok().map(|p| p.start_invert_m)))
                    .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
                StructureRow {
                    id: s.id,
                    name: s.name.clone(),
                    kind: s.kind,
                    easting_m: s.easting_m,
                    northing_m: s.northing_m,
                    rim_level_m: s.rim_level_m,
                    sump_level_m: s.sump_level_m,
                    inner_size: inner_text(s.inner),
                    wall_thickness_m: s.wall_thickness_m,
                    part: s.part.clone(),
                    pipes_in: name_of(&ins),
                    pipes_out: name_of(&outs),
                    lowest_invert_m: lowest,
                    depth_to_lowest_invert_m: lowest.map(|l| s.rim_level_m - l),
                }
            })
            .collect()
    }

    pub fn pipe_table(&self) -> Vec<PipeRow> {
        let flows = self.accumulate_flows().flows;
        self.pipes()
            .map(|p| {
                let name = |id: StructureId| self.structure(id).map(|s| s.name.clone()).unwrap_or_default();
                let h = p.shape.height_m();
                PipeRow {
                    id: p.id,
                    name: p.name.clone(),
                    start: name(p.start),
                    end: name(p.end),
                    size: p.shape.size_label(),
                    width_m: p.shape.width_m(),
                    height_m: h,
                    material: p.material.clone(),
                    wall_thickness_m: p.wall_thickness_m,
                    part: p.part.clone(),
                    roughness: roughness_text(p.roughness),
                    plan_length_m: self.plan_length_m(p.id).unwrap_or(f64::NAN),
                    sloping_length_m: self.sloping_length_m(p.id).unwrap_or(f64::NAN),
                    start_invert_m: p.start_invert_m,
                    end_invert_m: p.end_invert_m,
                    start_crown_m: p.start_invert_m + h,
                    end_crown_m: p.end_invert_m + h,
                    slope: self.slope(p.id).unwrap_or(f64::NAN),
                    flow_m3_s: flows.get(&p.id).copied(),
                }
            })
            .collect()
    }

    /// The structure table as CSV. Coordinates are written with every digit (shortest text that
    /// reads back to the same number). Pipe names are joined with `;`.
    pub fn structure_table_csv(&self) -> String {
        let mut out = csv::row(&[
            "id", "name", "kind", "easting_m", "northing_m", "rim_level_m", "sump_level_m", "inner_size", "wall_thickness_m", "part", "pipes_in", "pipes_out", "lowest_invert_m", "depth_to_lowest_invert_m",
        ]
        .map(String::from));
        let f = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
        for r in self.structure_table() {
            out.push_str(&csv::row(&[
                r.id.to_string(),
                r.name,
                r.kind.key().to_string(),
                r.easting_m.to_string(),
                r.northing_m.to_string(),
                r.rim_level_m.to_string(),
                f(r.sump_level_m),
                r.inner_size,
                f(r.wall_thickness_m),
                r.part.unwrap_or_default(),
                r.pipes_in.join(";"),
                r.pipes_out.join(";"),
                f(r.lowest_invert_m),
                f(r.depth_to_lowest_invert_m),
            ]));
        }
        out
    }

    /// The pipe table as CSV.
    pub fn pipe_table_csv(&self) -> String {
        let mut out = csv::row(&[
            "id", "name", "start", "end", "size", "width_m", "height_m", "material", "wall_thickness_m", "part", "roughness", "plan_length_m", "sloping_length_m", "start_invert_m", "end_invert_m", "start_crown_m",
            "end_crown_m", "slope", "flow_m3_s",
        ]
        .map(String::from));
        let f = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
        for r in self.pipe_table() {
            out.push_str(&csv::row(&[
                r.id.to_string(),
                r.name,
                r.start,
                r.end,
                r.size,
                r.width_m.to_string(),
                r.height_m.to_string(),
                r.material.unwrap_or_default(),
                f(r.wall_thickness_m),
                r.part.unwrap_or_default(),
                r.roughness,
                r.plan_length_m.to_string(),
                r.sloping_length_m.to_string(),
                r.start_invert_m.to_string(),
                r.end_invert_m.to_string(),
                r.start_crown_m.to_string(),
                r.end_crown_m.to_string(),
                r.slope.to_string(),
                f(r.flow_m3_s),
            ]));
        }
        out
    }
}
