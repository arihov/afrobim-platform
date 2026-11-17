//! Outputs: plain geometry, slope pattern ticks and CSV tables.
//!
//! Geometry is returned as arrays in source coordinates: lines as `Vec<[f64; 3]>`, triangles as
//! `[[f64; 3]; 3]`. CSV uses a comma separator, a dot decimal point, the shortest text that reads
//! back to the same `f64`, and a header row. Text fields containing a comma, quote or newline are
//! quoted with doubled quotes.

use serde::{Deserialize, Serialize};

use crate::error::{GradingError, Result};
use crate::featureline::GradeRow;
use crate::geom::*;
use crate::grading::{DaylightLine, GradingResult, RayStatus};
use crate::group::{BalanceResult, GroupBuild};
use crate::rays::RayKind;
use crate::site::CrossingReport;
use afrobim_terrain::{StagePoint, VolumeResult};

/// One tick of a slope pattern.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlopeTick {
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// Long ticks run the whole way from the top line to the toe line, short ticks part of the way.
    pub long: bool,
}

/// Plain geometry of a built group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupGeometry {
    pub triangles: Vec<[[f64; 3]; 3]>,
    pub projection_lines: Vec<[[f64; 3]; 2]>,
    pub daylight_lines: Vec<DaylightLine>,
}

impl GroupGeometry {
    pub fn of(build: &GroupBuild) -> GroupGeometry {
        let t = build.tin.triangles().iter().map(|t| t.map(|i| build.tin.points()[i as usize])).collect();
        GroupGeometry { triangles: t, projection_lines: build.gradings.iter().flat_map(|g| g.projection_lines.iter().cloned()).collect(), daylight_lines: build.gradings.iter().flat_map(|g| g.daylight_lines.iter().cloned()).collect() }
    }
}

/// Slope pattern ticks for drawing an embankment or cutting: between the footprint (top line) and
/// the daylight (toe line), a tick every `spacing / 2` along the footprint, alternately long and
/// short; a short tick reaches `short_fraction` (0 to 1) of the way to the daylight. Every ray of
/// an exterior-corner fan also gets a long tick. Ticks are interpolated linearly between the two
/// rays that enclose their position, so they follow the daylight line as drawn.
pub fn slope_ticks(result: &GradingResult, spacing: f64, short_fraction: f64) -> Result<Vec<SlopeTick>> {
    if !(spacing > 0.0 && spacing.is_finite()) {
        return Err(GradingError::Invalid("tick spacing must be positive".into()));
    }
    if !(short_fraction > 0.0 && short_fraction < 1.0) {
        return Err(GradingError::Invalid("short_fraction must be between 0 and 1".into()));
    }
    let rays = &result.rays;
    let n = rays.len();
    let mut ticks = vec![];
    if n < 2 {
        return Ok(ticks);
    }
    let mut cum = vec![0.0; n];
    for k in 1..n {
        let (a, b) = (rays[k - 1].foot, rays[k].foot);
        cum[k] = cum[k - 1] + len([b[0] - a[0], b[1] - a[1]]);
    }
    let push = |foot: [f64; 3], day: [f64; 3], long: bool, ticks: &mut Vec<SlopeTick>| {
        let f = if long { 1.0 } else { short_fraction };
        ticks.push(SlopeTick { from: [foot[0], foot[1]], to: [foot[0] + f * (day[0] - foot[0]), foot[1] + f * (day[1] - foot[1])], long });
    };
    for r in rays.iter().filter(|r| r.kind == RayKind::Fan) {
        if let Some(d) = r.daylight {
            push(r.foot, d, true, &mut ticks);
        }
    }
    let total = cum[n - 1];
    let step = spacing / 2.0;
    let mut m = 0usize;
    loop {
        let c = m as f64 * step;
        if c > total + 1e-9 {
            break;
        }
        if let Some(k) = (0..n - 1).find(|&k| cum[k + 1] > cum[k] && c >= cum[k] - 1e-12 && c <= cum[k + 1] + 1e-12) {
            if let (Some(da), Some(db)) = (rays[k].daylight, rays[k + 1].daylight) {
                if rays[k].status != RayStatus::NoDaylight && rays[k + 1].status != RayStatus::NoDaylight {
                    let u = (c - cum[k]) / (cum[k + 1] - cum[k]);
                    let lerp = |a: [f64; 3], b: [f64; 3]| [a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1]), a[2] + u * (b[2] - a[2])];
                    push(lerp(rays[k].foot, rays[k + 1].foot), lerp(da, db), m % 2 == 0, &mut ticks);
                }
            }
        }
        m += 1;
    }
    Ok(ticks)
}

fn esc(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV of a grade and elevation table.
pub fn grade_table_csv(rows: &[GradeRow]) -> String {
    let mut s = String::from("from,to,station_from,station_to,z_from,z_to,plan_length,dz,grade_percent,slope_h_per_v,length_3d,on_arc\n");
    for r in rows {
        s += &format!("{},{},{},{},{},{},{},{},{},{},{},{}\n", r.from, r.to, r.station_from, r.station_to, r.z_from, r.z_to, r.plan_length, r.dz, r.grade_percent, r.slope_h_per_v.map(|v| v.to_string()).unwrap_or_default(), r.length_3d, r.on_arc);
    }
    s
}

/// CSV of the crossings a site resolved.
pub fn crossings_csv(rows: &[CrossingReport]) -> String {
    let mut s = String::from("line_a,line_b,x,y,station_a,station_b,z_a,z_b,z_used,winner,rule,changed\n");
    for r in rows {
        s += &format!("{},{},{},{},{},{},{},{},{},{},{:?},{}\n", esc(&r.line_a), esc(&r.line_b), r.x, r.y, r.station_a, r.station_b, r.z_a, r.z_b, r.z_used, esc(&r.winner), r.rule, r.changed);
    }
    s
}

/// CSV of the rays of a grading.
pub fn rays_csv(result: &GradingResult) -> String {
    let mut s = String::from("node,region,station,kind,foot_x,foot_y,foot_z,distance,day_x,day_y,day_z,status\n");
    for r in &result.rays {
        let (dx, dy, dz) = r.daylight.map(|d| (d[0].to_string(), d[1].to_string(), d[2].to_string())).unwrap_or_default();
        s += &format!("{},{},{},{:?},{},{},{},{},{},{},{},{:?}\n", r.node, r.region, r.station, r.kind, r.foot[0], r.foot[1], r.foot[2], r.distance.map(|v| v.to_string()).unwrap_or_default(), dx, dy, dz, r.status);
    }
    s
}

/// CSV of a volume result.
pub fn volume_csv(v: &VolumeResult) -> String {
    format!("cut,fill,net,overlap_area,cut_area,fill_area,first_only_area,second_only_area\n{},{},{},{},{},{},{},{}\n", v.cut, v.fill, v.net, v.overlap_area, v.cut_area, v.fill_area, v.first_only_area, v.second_only_area)
}

/// CSV of a stage-storage table.
pub fn stage_storage_csv(rows: &[StagePoint]) -> String {
    let mut s = String::from("elevation,volume,water_area,incremental_volume\n");
    for r in rows {
        s += &format!("{},{},{},{}\n", r.elevation, r.volume, r.water_area, r.incremental_volume);
    }
    s
}

/// CSV of the evaluations of an auto-balance.
pub fn balance_csv(b: &BalanceResult) -> String {
    let mut s = String::from("evaluation,shift,cut,fill,net\n");
    for (i, h) in b.history.iter().enumerate() {
        s += &format!("{},{},{},{},{}\n", i, h.shift, h.cut, h.fill, h.net);
    }
    s
}
