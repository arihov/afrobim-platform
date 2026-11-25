// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The intersection object: inputs, the build, the result.
//!
//! [`build_intersection`] takes [`IntersectionInputs`] (which name the two alignments, never hold
//! them) and the alignments, and returns an [`IntersectionResult`] with new alignments built with
//! the alignment crate's types (edges, kerb returns, kerb lines, the locked secondary), their
//! profiles, findings, the corridor description and the [`DependencyRecord`] of what it was built
//! from. Nothing here has a default engineering value: every offset, radius, length, crossfall and
//! spacing is an input, and a quadrant with no return definition is refused.

use afrobim_alignment::{Alignment, AlignmentElement};
use afrobim_corridor::{Assembly, Side};
use serde::{Deserialize, Serialize};

use crate::corridor::{build_corridor, CorridorInputs, JunctionCorridor};
use crate::crossing::{find_crossings, Crossing};
use crate::edges::{build_edge, RoadEdges};
use crate::error::{Finding, JunctionError, Severity};
use crate::geom::{cross, dir, dist, heading_of, pt_at, slice_alignment, station_offset, sub, Pt};
use crate::kerb::{build_return, Arm, KerbReturn, Quadrant, QuadrantFrame, ReturnSpec};
use crate::vertical::{blend_return_profile, local_minima, lock_secondary_profile, sample_profile, CrossfallInput, EdgeLevel, LockReport, LowPoint, ReturnVertical, SurfaceLevel};

/// Which road keeps its crown through the junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorridorType {
    /// The primary road's crown and profile are kept; the secondary profile is generated from the
    /// primary's surface through the junction ([`lock_secondary_profile`]).
    PrimaryCrownMaintained,
    /// Both roads keep their own profile; the difference between them at the crossing is reported.
    AllCrownsMaintained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadId {
    Primary,
    Secondary,
}

/// How far each road's edge alignments run each side of the crossing, m (entered; clipped to the
/// alignments).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JunctionExtents {
    pub primary_behind_m: f64,
    pub primary_ahead_m: f64,
    pub secondary_behind_m: f64,
    pub secondary_ahead_m: f64,
}

/// A point of an island outline: a station of a road and an offset from it (positive right).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IslandPoint {
    pub station_m: f64,
    pub offset_m: f64,
}

/// A channelising island outline given by stations and offsets from a road alignment, joined by
/// straights in the order given and closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandInput {
    pub name: String,
    pub road: RoadId,
    pub points: Vec<IslandPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandResult {
    pub name: String,
    /// Closed outline (first point repeated at the end).
    pub outline: Vec<Pt>,
    /// The outline as an alignment of straights.
    pub alignment: Alignment,
    pub area_m2: f64,
    pub perimeter_m: f64,
}

/// Inputs of the vertical part. Everything is entered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerticalInputs {
    /// Cross-fall of the primary road (percent, rising away from the centreline positive).
    pub primary_crossfall: CrossfallInput,
    /// Cross-fall of the secondary road outside the primary carriageway.
    pub secondary_crossfall: CrossfallInput,
    /// Distance outside the primary's edge at which the secondary returns to its own profile, m.
    pub tie_in_length_m: f64,
    /// Spacing of extra PVIs of the locked profile across the primary carriageway, m (none: only at the edges and crown).
    #[serde(default)]
    pub lock_sample_spacing_m: Option<f64>,
    /// Spacing of the PVIs of an edge profile, m.
    pub edge_profile_spacing_m: f64,
    /// Spacing of the PVIs of a return profile when it is a sampled cubic, m.
    pub return_profile_spacing_m: f64,
}

/// What an intersection is built from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntersectionInputs {
    pub name: String,
    /// Name of the primary alignment and of the secondary alignment.
    pub primary: String,
    pub secondary: String,
    /// Which crossing of the two (in order of station along the primary), from 0.
    #[serde(default)]
    pub crossing_index: usize,
    /// Straight extension of each alignment end for the crossing search, m (0: none).
    #[serde(default)]
    pub extend_m: f64,
    pub corridor_type: CorridorType,
    pub primary_edges: RoadEdges,
    pub secondary_edges: RoadEdges,
    pub extents: JunctionExtents,
    /// One definition for every quadrant that exists (both arms present).
    pub returns: Vec<ReturnSpec>,
    #[serde(default)]
    pub islands: Vec<IslandInput>,
    #[serde(default)]
    pub vertical: Option<VerticalInputs>,
    #[serde(default)]
    pub corridor: Option<CorridorInputs>,
}

/// One full-length edge alignment of a road.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeResult {
    pub road: RoadId,
    pub side: Side,
    pub alignment: Alignment,
    /// Road stations of the first and last point of the edge.
    pub road_station_start_m: f64,
    pub road_station_end_m: f64,
    pub max_taper_chord_deviation_m: f64,
    pub max_joint_gap_m: f64,
    /// Interior low points of the edge level (with a profile).
    pub low_points: Vec<LowPoint>,
}

/// A kerb return, its place on the roads and its vertical definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnResult {
    pub geometry: KerbReturn,
    /// Primary road station of the tangent point on the primary edge, m.
    pub primary_road_station_m: f64,
    /// Secondary road station of the tangent point on the secondary edge, m.
    pub secondary_road_station_m: f64,
    pub vertical: Option<ReturnVertical>,
}

/// A piece of kerb line: an edge between the returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KerbLine {
    pub road: RoadId,
    pub side: Side,
    /// The arm it belongs to: of the secondary road (secondary lines), or of the primary on the side that has a mouth (pieces of the primary edge either side of it). `None`: a whole edge.
    pub arm: Option<Arm>,
    pub alignment: Alignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Alignment,
    Profile,
    Superelevation,
    Assembly,
}

/// One thing the intersection was built from, with a fingerprint of its content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub name: String,
    /// 64-bit FNV-1a of the JSON of the thing, in hex.
    pub fingerprint: String,
}

/// What the intersection was built from, so the application can tell when to rebuild.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyRecord {
    pub intersection: String,
    pub items: Vec<Dependency>,
    /// Fingerprint of the inputs.
    pub inputs_fingerprint: String,
}

pub(crate) fn fnv1a_hex<T: Serialize>(v: &T) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

impl DependencyRecord {
    /// The items whose content differs from what was recorded (or is missing), as `kind name`.
    pub fn stale(&self, alignments: &[Alignment], assemblies: &[Assembly]) -> Vec<String> {
        let mut out = Vec::new();
        for d in &self.items {
            let now = match d.kind {
                DependencyKind::Alignment => alignments.iter().find(|a| a.name == d.name).map(|a| fnv1a_hex(&(&a.start_station_m, &a.elements, &a.station_equations))),
                DependencyKind::Profile => alignments.iter().find(|a| a.name == d.name).and_then(|a| a.profile.as_ref()).map(fnv1a_hex),
                DependencyKind::Superelevation => alignments.iter().find(|a| a.name == d.name).and_then(|a| a.superelevation.as_ref()).map(fnv1a_hex),
                DependencyKind::Assembly => assemblies.iter().find(|a| a.name == d.name).map(fnv1a_hex),
            };
            if now.as_deref() != Some(d.fingerprint.as_str()) {
                out.push(format!("{:?} {}", d.kind, d.name));
            }
        }
        out
    }
}

/// Everything the build produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntersectionResult {
    pub name: String,
    /// Every crossing of the two alignments found; `crossing` is the chosen one.
    pub crossings: Vec<Crossing>,
    pub crossing: Crossing,
    pub edges: Vec<EdgeResult>,
    pub returns: Vec<ReturnResult>,
    pub kerb_lines: Vec<KerbLine>,
    pub islands: Vec<IslandResult>,
    /// The secondary profile locked to the primary (primary crown maintained).
    pub lock: Option<LockReport>,
    /// Every new alignment with its profile, by name: edges, returns, kerb lines, the locked
    /// secondary. The corridor baselines name alignments from this list and the inputs' own.
    pub alignments: Vec<Alignment>,
    pub corridor: Option<JunctionCorridor>,
    pub findings: Vec<Finding>,
    pub dependencies: DependencyRecord,
}

fn find<'a>(alignments: &'a [Alignment], name: &str) -> Result<&'a Alignment, JunctionError> {
    alignments.iter().find(|a| a.name == name).ok_or_else(|| JunctionError::NoAlignment(name.to_string()))
}

fn side_of(c: f64) -> Side {
    if c > 0.0 {
        Side::Left
    } else {
        Side::Right
    }
}

fn margin(e: &RoadEdges) -> f64 {
    let w: f64 = e.left_widening.iter().chain(&e.right_widening).map(|w| w.width_m).sum();
    2.0 * (e.left_offset_m + e.right_offset_m + w) + 1.0
}

fn islands(inputs: &IntersectionInputs, primary: &Alignment, secondary: &Alignment) -> Result<Vec<IslandResult>, JunctionError> {
    let mut out = Vec::new();
    for i in &inputs.islands {
        if i.points.len() < 3 {
            return Err(JunctionError::input(format!("island '{}' needs at least three points", i.name)));
        }
        let road = if i.road == RoadId::Primary { primary } else { secondary };
        let mut pts: Vec<Pt> = Vec::new();
        for p in &i.points {
            pts.push(road.point_at_offset(p.station_m, p.offset_m).ok_or_else(|| JunctionError::input(format!("island '{}': station {} is off alignment '{}'", i.name, p.station_m, road.name)))?);
        }
        let first = pts[0];
        pts.push(first);
        let mut area = 0.0;
        let mut perimeter = 0.0;
        let mut elements = Vec::new();
        for w in pts.windows(2) {
            // Coordinates relative to the first point: the shoelace sum is then free of the
            // cancellation that the magnitude of UTM northings would cause.
            let (a, b) = (sub(w[0], first), sub(w[1], first));
            area += cross(a, b) / 2.0;
            let len = dist(w[0], w[1]);
            perimeter += len;
            if len > 0.0 {
                elements.push(AlignmentElement::line(w[0].0, w[0].1, heading_of(sub(w[1], w[0])), len));
            }
        }
        let alignment = Alignment::from_elements(&format!("{} {}", inputs.name, i.name), 0.0, elements).map_err(|e| JunctionError::geometry(e.to_string()))?;
        out.push(IslandResult { name: i.name.clone(), outline: pts, alignment, area_m2: area.abs(), perimeter_m: perimeter });
    }
    Ok(out)
}

/// Builds an intersection. See the module documentation.
pub fn build_intersection(inputs: &IntersectionInputs, alignments: &[Alignment]) -> Result<IntersectionResult, JunctionError> {
    inputs.primary_edges.check("primary edges")?;
    inputs.secondary_edges.check("secondary edges")?;
    let ex = inputs.extents;
    for (n, v) in [("primary behind", ex.primary_behind_m), ("primary ahead", ex.primary_ahead_m), ("secondary behind", ex.secondary_behind_m), ("secondary ahead", ex.secondary_ahead_m)] {
        if !(v.is_finite() && v >= 0.0) {
            return Err(JunctionError::input(format!("the {n} extent must be zero or more (got {v})")));
        }
    }
    let primary = find(alignments, &inputs.primary)?;
    let secondary = find(alignments, &inputs.secondary)?;
    let mut findings: Vec<Finding> = Vec::new();

    // 1. The crossing.
    let crossings = find_crossings(primary, secondary, inputs.extend_m);
    let crossing = crossings
        .get(inputs.crossing_index)
        .cloned()
        .ok_or_else(|| JunctionError::NoCrossing { a: primary.name.clone(), b: secondary.name.clone(), found: crossings.len(), index: inputs.crossing_index })?;
    if crossing.on_extension_a || crossing.on_extension_b {
        return Err(JunctionError::geometry(
            "the crossing is on an extension of an alignment: end the alignment on the other road (an alignment that stops short is not extended by the build)",
        ));
    }
    let (sp, ss) = (crossing.station_a_m, crossing.station_b_m);
    let tag = |n: &str| format!("{}: {n}", inputs.name);
    findings.push(Finding::new(
        "junction.skew",
        Severity::Info,
        &inputs.name,
        format!("the roads cross at {:.3} deg ({:.3} deg from a right angle); no limit is applied", crossing.angle_rad.to_degrees(), crossing.skew_rad.to_degrees()),
        &[("angle_rad", crossing.angle_rad), ("skew_rad", crossing.skew_rad), ("angle_deg", crossing.angle_rad.to_degrees()), ("skew_deg", crossing.skew_rad.to_degrees())],
    ));
    if crossings.len() > 1 {
        findings.push(Finding::new(
            "junction.several_crossings",
            Severity::Info,
            &inputs.name,
            format!("the alignments cross {} times; crossing {} is used", crossings.len(), inputs.crossing_index),
            &[("crossings", crossings.len() as f64)],
        ));
    }

    // 2. The window of each road and its edges.
    let (p_lo, p_hi) = ((sp - ex.primary_behind_m).max(primary.start_station_m), (sp + ex.primary_ahead_m).min(primary.end_station_m()));
    let (s_lo, s_hi) = ((ss - ex.secondary_behind_m).max(secondary.start_station_m), (ss + ex.secondary_ahead_m).min(secondary.end_station_m()));
    for (road, lo, hi, at, label) in [(primary, p_lo, p_hi, sp, "primary"), (secondary, s_lo, s_hi, ss, "secondary")] {
        let k = road.curvature_at(at).unwrap_or(0.0);
        let curved_in_window = {
            let starts = road.element_stations();
            road.elements.iter().zip(starts).any(|(e, s0)| e.min_radius().is_some() && s0 < hi && s0 + e.length > lo)
        };
        findings.push(Finding::new(
            "junction.on_curve",
            Severity::Info,
            &tag(label),
            if k != 0.0 {
                format!("the {label} road is on a curve of radius {:.3} m at the crossing", 1.0 / k.abs())
            } else if curved_in_window {
                format!("the {label} road is straight at the crossing but has a curve within the junction extent")
            } else {
                format!("the {label} road is straight through the junction extent")
            },
            &[("curvature_1pm", k)],
        ));
    }
    let mk_edge = |road: &Alignment, side: Side, edges: &RoadEdges, lo: f64, hi: f64, id: RoadId| -> Result<EdgeResult, JunctionError> {
        let nm = format!("{} {} {} edge", inputs.name, if id == RoadId::Primary { "primary" } else { "secondary" }, if side == Side::Left { "left" } else { "right" });
        let b = build_edge(road, side, edges, lo, hi, &nm)?;
        Ok(EdgeResult {
            road: id,
            side,
            alignment: b.alignment,
            road_station_start_m: lo,
            road_station_end_m: hi,
            max_taper_chord_deviation_m: b.max_taper_chord_deviation_m,
            max_joint_gap_m: b.max_joint_gap_m,
            low_points: vec![],
        })
    };
    let mut edges = vec![
        mk_edge(primary, Side::Left, &inputs.primary_edges, p_lo, p_hi, RoadId::Primary)?,
        mk_edge(primary, Side::Right, &inputs.primary_edges, p_lo, p_hi, RoadId::Primary)?,
        mk_edge(secondary, Side::Left, &inputs.secondary_edges, s_lo, s_hi, RoadId::Secondary)?,
        mk_edge(secondary, Side::Right, &inputs.secondary_edges, s_lo, s_hi, RoadId::Secondary)?,
    ];
    for e in &edges {
        if e.max_taper_chord_deviation_m > 1e-6 {
            findings.push(Finding::new(
                "edge.taper_chord",
                Severity::Info,
                &e.alignment.name,
                format!("a widening taper on a curved road is one straight; the true offset curve is up to {:.4} m from it", e.max_taper_chord_deviation_m),
                &[("deviation_m", e.max_taper_chord_deviation_m)],
            ));
        }
        if e.max_joint_gap_m > 1e-4 {
            findings.push(Finding::new("edge.joint_gap", Severity::Info, &e.alignment.name, format!("the offset of a spiral leaves joint gaps of up to {:.6} m", e.max_joint_gap_m), &[("gap_m", e.max_joint_gap_m)]));
        }
    }
    let edge_of = |edges: &[EdgeResult], road: RoadId, side: Side| -> usize { edges.iter().position(|e| e.road == road && e.side == side).expect("four edges") };

    // 3. Returns, quadrant by quadrant.
    let (u, v) = (dir(crossing.heading_a_rad), dir(crossing.heading_b_rad));
    let origin = (crossing.x, crossing.y);
    let present = |arms: crate::crossing::Arms, a: Arm| if a == Arm::Ahead { arms.ahead } else { arms.behind };
    let mut returns: Vec<ReturnResult> = Vec::new();
    for q in Quadrant::ALL {
        let exists = present(crossing.arms_a, q.primary_arm) && present(crossing.arms_b, q.secondary_arm);
        let spec = inputs.returns.iter().find(|r| r.quadrant == q);
        match (exists, spec) {
            (false, Some(_)) => return Err(JunctionError::input(format!("a return is defined for quadrant {} but one of its arms does not exist at this crossing", q.label()))),
            (true, None) => return Err(JunctionError::input(format!("quadrant {} has both arms and needs a return definition (no default radius exists)", q.label()))),
            (false, None) => continue,
            _ => {}
        }
        let spec = spec.expect("checked");
        let (dir_a, dir_b) = ((u.0 * q.primary_arm.sign(), u.1 * q.primary_arm.sign()), (v.0 * q.secondary_arm.sign(), v.1 * q.secondary_arm.sign()));
        let (cb, ca) = (cross(u, dir_b), cross(v, dir_a));
        if cb.abs() < 1e-9 || ca.abs() < 1e-9 {
            return Err(JunctionError::geometry("the two roads are parallel at the crossing"));
        }
        let (e1_side, e2_side) = (side_of(cb), side_of(ca));
        let (i1, i2) = (edge_of(&edges, RoadId::Primary, e1_side), edge_of(&edges, RoadId::Secondary, e2_side));
        let frame = QuadrantFrame { quadrant: q, e1: &edges[i1].alignment, e1_side, e2: &edges[i2].alignment, e2_side, origin, dir_a, dir_b };
        let name = format!("{} return {}", inputs.name, q.label());
        match build_return(&frame, &spec.kind, &name) {
            Ok(r) => {
                let (sp1, _) = station_offset(primary, r.primary_point)?;
                let (ss2, _) = station_offset(secondary, r.secondary_point)?;
                returns.push(ReturnResult { geometry: r, primary_road_station_m: sp1, secondary_road_station_m: ss2, vertical: None });
            }
            Err(JunctionError::Input(m)) => return Err(JunctionError::Input(m)),
            Err(e) => findings.push(Finding::new("return.does_not_fit", Severity::Error, &format!("{} {}", inputs.name, q.label()), e.to_string(), &[])),
        }
    }
    for r in &returns {
        let g = &r.geometry;
        let widened = |road: &RoadEdges, side: Side, s: f64| road.offset_at(side, s) - road.base(side) > 1e-9;
        findings.push(Finding::new(
            "return.geometry",
            Severity::Info,
            &format!("{} {}", inputs.name, g.quadrant.label()),
            format!(
                "return {}: length {:.3} m, turns {:.3} deg, tangent lengths {:.3} m (primary edge) and {:.3} m (secondary edge)",
                g.quadrant.label(),
                g.length_m,
                g.turn_rad.to_degrees(),
                g.primary_tangent_length_m,
                g.secondary_tangent_length_m
            ),
            &[("length_m", g.length_m), ("turn_rad", g.turn_rad), ("primary_tangent_m", g.primary_tangent_length_m), ("secondary_tangent_m", g.secondary_tangent_length_m)],
        ));
        if g.edge_deviation_m > 1e-4 {
            findings.push(Finding::new(
                "return.edge_deviation",
                Severity::Warning,
                &format!("{} {}", inputs.name, g.quadrant.label()),
                format!("the compound return takes its edges as straight; its tangent points are {:.4} m off the real edges", g.edge_deviation_m),
                &[("deviation_m", g.edge_deviation_m)],
            ));
        }
        if widened(&inputs.primary_edges, g.primary_side, r.primary_road_station_m) || widened(&inputs.secondary_edges, g.secondary_side, r.secondary_road_station_m) {
            findings.push(Finding::new(
                "return.on_widening",
                Severity::Info,
                &format!("{} {}", inputs.name, g.quadrant.label()),
                "a tangent point of this return lies where the edge is widened".to_string(),
                &[],
            ));
        }
    }
    // Overlaps between returns.
    let polys: Vec<Vec<Pt>> = returns.iter().map(|r| crate::geom::polyline_between(&r.geometry.alignment, 0.0, r.geometry.length_m, (r.geometry.length_m / 64.0).max(1e-3))).collect();
    for i in 0..returns.len() {
        for j in i + 1..returns.len() {
            if polylines_intersect(&polys[i], &polys[j]) {
                findings.push(Finding::new(
                    "return.overlap",
                    Severity::Warning,
                    &inputs.name,
                    format!("returns {} and {} overlap", returns[i].geometry.quadrant.label(), returns[j].geometry.quadrant.label()),
                    &[],
                ));
            }
        }
    }

    // 4. Islands.
    let island_results = islands(inputs, primary, secondary)?;

    // 5. Vertical.
    let mut all: Vec<Alignment> = Vec::new();
    let mut lock: Option<LockReport> = None;
    let mut working_secondary: Option<Alignment> = None;
    if let Some(vin) = &inputs.vertical {
        primary.profile.as_ref().ok_or_else(|| JunctionError::NoProfile(primary.name.clone()))?;
        let sec_prof = secondary.profile.as_ref().ok_or_else(|| JunctionError::NoProfile(secondary.name.clone()))?;
        let g_p = primary.profile.as_ref().and_then(|p| p.grade_at(sp).ok());
        let g_s = sec_prof.grade_at(ss).ok();
        findings.push(Finding::new(
            "junction.on_grade",
            Severity::Info,
            &inputs.name,
            format!(
                "profile grade at the crossing: primary {} m/m, secondary {} m/m (no limit applied)",
                g_p.map_or("n/a".into(), |g| format!("{g:.5}")),
                g_s.map_or("n/a".into(), |g| format!("{g:.5}"))
            ),
            &[("primary_grade", g_p.unwrap_or(f64::NAN)), ("secondary_grade", g_s.unwrap_or(f64::NAN))],
        ));
        let mut sec_work = secondary.clone();
        match inputs.corridor_type {
            CorridorType::PrimaryCrownMaintained => {
                let pe_l = &edges[edge_of(&edges, RoadId::Primary, Side::Left)].alignment;
                let pe_r = &edges[edge_of(&edges, RoadId::Primary, Side::Right)].alignment;
                let pw = slice_alignment(primary, p_lo - margin(&inputs.primary_edges), p_hi + margin(&inputs.primary_edges), "primary window")?;
                let rep = lock_secondary_profile(
                    &pw,
                    [pe_l, pe_r],
                    secondary,
                    &crossing,
                    &vin.primary_crossfall,
                    vin.tie_in_length_m,
                    vin.lock_sample_spacing_m,
                    &format!("{} locked to {}", secondary.name, primary.name),
                )?;
                findings.push(Finding::new(
                    "profile.lock",
                    Severity::Info,
                    &inputs.name,
                    format!(
                        "the secondary profile is locked to the primary surface between stations {:.3} and {:.3}; the crown level {:.4} m differs from the secondary's own level {:.4} m by {:.4} m",
                        rep.region_start_station_m, rep.region_end_station_m, rep.crown_level_m, rep.original_level_at_crossing_m, rep.level_shift_m
                    ),
                    &[("crown_level_m", rep.crown_level_m), ("level_shift_m", rep.level_shift_m)],
                ));
                for (n, g) in [("start", rep.tie_in_grade_start), ("end", rep.tie_in_grade_end)] {
                    if let Some(g) = g {
                        findings.push(Finding::new("profile.tie_in_grade", Severity::Info, &inputs.name, format!("the tie-in at the {n} of the lock has a grade of {g:.5} m/m"), &[("grade", g)]));
                    }
                }
                sec_work.name = rep.profile.name.clone();
                sec_work.profile = Some(rep.profile.clone());
                lock = Some(rep);
            }
            CorridorType::AllCrownsMaintained => {
                let zp = primary.profile.as_ref().and_then(|p| p.elevation_at(sp).ok());
                let zs = sec_prof.elevation_at(ss).ok();
                if let (Some(zp), Some(zs)) = (zp, zs) {
                    findings.push(Finding::new(
                        "profile.crowns",
                        Severity::Info,
                        &inputs.name,
                        format!("all crowns maintained: the primary profile is {zp:.4} m and the secondary {zs:.4} m at the crossing, a difference of {:.4} m", zs - zp),
                        &[("primary_level_m", zp), ("secondary_level_m", zs), ("difference_m", zs - zp)],
                    ));
                }
            }
        }
        let pw = slice_alignment(primary, p_lo - margin(&inputs.primary_edges), p_hi + margin(&inputs.primary_edges), "primary window")?;
        let sw = slice_alignment(&sec_work, s_lo - margin(&inputs.secondary_edges), s_hi + margin(&inputs.secondary_edges), "secondary window")?;
        let surf_p = SurfaceLevel { road: &pw, crossfall: &vin.primary_crossfall };
        let surf_s = SurfaceLevel { road: &sw, crossfall: &vin.secondary_crossfall };
        // Edge profiles.
        for e in edges.iter_mut() {
            let surface = if e.road == RoadId::Primary { surf_p } else { surf_s };
            let lv = EdgeLevel { edge: &e.alignment, surface };
            let (a, b) = (e.alignment.start_station_m, e.alignment.end_station_m());
            let prof = sample_profile(&format!("{} profile", e.alignment.name), a, b, vin.edge_profile_spacing_m, &|s| lv.level(s))?;
            let level = |s: f64| lv.level(s).unwrap_or(f64::NAN);
            let grade = |s: f64| {
                let h = 0.01;
                (level((s + h).min(b)) - level((s - h).max(a))) / ((s + h).min(b) - (s - h).max(a))
            };
            e.low_points = local_minima(&level, &grade, a, b)
                .into_iter()
                .map(|(s, z)| {
                    let p = pt_at(&e.alignment, s).unwrap_or((f64::NAN, f64::NAN));
                    LowPoint { station_m: s, level_m: z, x: p.0, y: p.1 }
                })
                .collect();
            e.alignment.profile = Some(prof);
        }
        // Return profiles.
        for r in returns.iter_mut() {
            let g = &r.geometry;
            let (i1, i2) = (edge_index_for(&edges, g, RoadId::Primary), edge_index_for(&edges, g, RoadId::Secondary));
            let lv1 = EdgeLevel { edge: &edges[i1].alignment, surface: surf_p };
            let lv2 = EdgeLevel { edge: &edges[i2].alignment, surface: surf_s };
            let (z0, ge1) = (lv1.level(g.primary_edge_station_m)?, lv1.grade(g.primary_edge_station_m)?);
            let (z1, ge2) = (lv2.level(g.secondary_edge_station_m)?, lv2.grade(g.secondary_edge_station_m)?);
            let al = &g.alignment;
            let rv = blend_return_profile(&format!("{} profile", al.name), g.length_m, z0, g.start_direction_sign * ge1, z1, g.end_direction_sign * ge2, vin.return_profile_spacing_m, &|s| {
                al.point_at(s).map_or((f64::NAN, f64::NAN), |p| (p.x, p.y))
            })?;
            findings.push(Finding::new(
                "return.drainage",
                Severity::Info,
                &format!("{} {}", inputs.name, g.quadrant.label()),
                if let Some(lp) = rv.low_points.first() {
                    format!("return {} has a low point at return station {:.3} m, level {:.4} m, at ({:.3}, {:.3})", g.quadrant.label(), lp.station_m, lp.level_m, lp.x, lp.y)
                } else {
                    format!("return {} has no interior low point; its lower end is at return station {:.3} m, level {:.4} m", g.quadrant.label(), rv.lowest_end.station_m, rv.lowest_end.level_m)
                },
                &[("low_level_m", rv.low_points.first().map_or(rv.lowest_end.level_m, |l| l.level_m)), ("max_abs_grade", rv.max_abs_grade)],
            ));
            r.geometry.alignment.profile = Some(rv.profile.clone());
            r.vertical = Some(rv);
        }
        working_secondary = Some(sec_work);
    }

    // 6. Kerb lines.
    let kerb_lines = kerb_lines(&edges, &returns, vin_levels(inputs, primary, secondary, &working_secondary)?)?;

    // 7. Alignment list.
    for e in &edges {
        all.push(e.alignment.clone());
    }
    for r in &returns {
        all.push(r.geometry.alignment.clone());
    }
    for k in &kerb_lines {
        all.push(k.alignment.clone());
    }
    if let (Some(ws), Some(_)) = (&working_secondary, &lock) {
        all.push(ws.clone());
    }

    // 8. Corridor.
    let corridor = match &inputs.corridor {
        Some(ci) => {
            let sec_for_corridor = working_secondary.as_ref().unwrap_or(secondary);
            Some(build_corridor(inputs, ci, primary, sec_for_corridor, &crossing, &edges, &returns, (p_lo, p_hi), (s_lo, s_hi), &mut findings)?)
        }
        None => None,
    };

    // 9. Dependencies.
    let mut items = Vec::new();
    for a in [primary, secondary] {
        items.push(Dependency { kind: DependencyKind::Alignment, name: a.name.clone(), fingerprint: fnv1a_hex(&(&a.start_station_m, &a.elements, &a.station_equations)) });
        if let Some(p) = &a.profile {
            items.push(Dependency { kind: DependencyKind::Profile, name: a.name.clone(), fingerprint: fnv1a_hex(p) });
        }
        if let Some(s) = &a.superelevation {
            items.push(Dependency { kind: DependencyKind::Superelevation, name: a.name.clone(), fingerprint: fnv1a_hex(s) });
        }
    }
    if let Some(ci) = &inputs.corridor {
        for a in &ci.library {
            items.push(Dependency { kind: DependencyKind::Assembly, name: a.name.clone(), fingerprint: fnv1a_hex(a) });
        }
    }
    let dependencies = DependencyRecord { intersection: inputs.name.clone(), items, inputs_fingerprint: fnv1a_hex(inputs) };
    Ok(IntersectionResult { name: inputs.name.clone(), crossings, crossing, edges, returns, kerb_lines, islands: island_results, lock, alignments: all, corridor, findings, dependencies })
}

fn edge_index_for(edges: &[EdgeResult], g: &KerbReturn, road: RoadId) -> usize {
    let side = if road == RoadId::Primary { g.primary_side } else { g.secondary_side };
    edges.iter().position(|e| e.road == road && e.side == side).expect("four edges")
}

/// Plain pointer to the surfaces used for kerb line profiles.
struct KerbLevels<'a> {
    primary: Option<(Alignment, &'a CrossfallInput)>,
    secondary: Option<(Alignment, &'a CrossfallInput)>,
    spacing: f64,
}

fn vin_levels<'a>(inputs: &'a IntersectionInputs, primary: &Alignment, secondary: &Alignment, working_secondary: &Option<Alignment>) -> Result<KerbLevels<'a>, JunctionError> {
    match &inputs.vertical {
        None => Ok(KerbLevels { primary: None, secondary: None, spacing: 0.0 }),
        Some(v) => {
            let pw = slice_alignment(primary, primary.start_station_m, primary.end_station_m(), "primary kerb surface")?;
            let sw_src = working_secondary.as_ref().unwrap_or(secondary);
            let sw = slice_alignment(sw_src, sw_src.start_station_m, sw_src.end_station_m(), "secondary kerb surface")?;
            Ok(KerbLevels { primary: Some((pw, &v.primary_crossfall)), secondary: Some((sw, &v.secondary_crossfall)), spacing: v.edge_profile_spacing_m })
        }
    }
}

fn kerb_lines(edges: &[EdgeResult], returns: &[ReturnResult], lv: KerbLevels) -> Result<Vec<KerbLine>, JunctionError> {
    let mut out = Vec::new();
    let mut push = |road: RoadId, side: Side, arm: Option<Arm>, edge: &EdgeResult, s0: f64, s1: f64, label: &str| -> Result<(), JunctionError> {
        if s1 - s0 < 1e-6 {
            return Ok(());
        }
        let name = format!("{} {label}", edge.alignment.name);
        let mut al = slice_alignment(&edge.alignment, s0, s1, &name)?;
        al.profile = None;
        if let (Some((p, cf)), Some((s, cs))) = (&lv.primary, &lv.secondary) {
            let surface = if road == RoadId::Primary { SurfaceLevel { road: p, crossfall: cf } } else { SurfaceLevel { road: s, crossfall: cs } };
            let e = EdgeLevel { edge: &al, surface };
            let prof = sample_profile(&format!("{name} profile"), al.start_station_m, al.end_station_m(), lv.spacing, &|st| e.level(st))?;
            al.profile = Some(prof);
        }
        out.push(KerbLine { road, side, arm, alignment: al });
        Ok(())
    };
    for e in edges.iter().filter(|e| e.road == RoadId::Primary) {
        let rs: Vec<&ReturnResult> = returns.iter().filter(|r| r.geometry.primary_side == e.side).collect();
        let (a, b) = (e.alignment.start_station_m, e.alignment.end_station_m());
        if rs.is_empty() {
            push(RoadId::Primary, e.side, None, e, a, b, "kerb")?;
            continue;
        }
        let behind = rs.iter().find(|r| r.geometry.quadrant.primary_arm == Arm::Behind);
        let ahead = rs.iter().find(|r| r.geometry.quadrant.primary_arm == Arm::Ahead);
        if let Some(r) = behind {
            push(RoadId::Primary, e.side, Some(Arm::Behind), e, a, r.geometry.primary_edge_station_m, "kerb behind")?;
        }
        if let Some(r) = ahead {
            push(RoadId::Primary, e.side, Some(Arm::Ahead), e, r.geometry.primary_edge_station_m, b, "kerb ahead")?;
        }
    }
    for r in returns {
        let g = &r.geometry;
        let e = edges.iter().find(|e| e.road == RoadId::Secondary && e.side == g.secondary_side).expect("four edges");
        let (a, b) = (e.alignment.start_station_m, e.alignment.end_station_m());
        match g.quadrant.secondary_arm {
            Arm::Ahead => push(RoadId::Secondary, e.side, Some(Arm::Ahead), e, g.secondary_edge_station_m, b, &format!("kerb ahead {}", g.quadrant.label()))?,
            Arm::Behind => push(RoadId::Secondary, e.side, Some(Arm::Behind), e, a, g.secondary_edge_station_m, &format!("kerb behind {}", g.quadrant.label()))?,
        }
    }
    Ok(out)
}

/// Whether two polylines cross each other (proper crossings of segments; touching at an end is not a crossing).
pub fn polylines_intersect(a: &[Pt], b: &[Pt]) -> bool {
    for x in a.windows(2) {
        for y in b.windows(2) {
            let (r, q) = (sub(x[1], x[0]), sub(y[1], y[0]));
            let den = cross(r, q);
            if den.abs() < 1e-15 {
                continue;
            }
            let w = sub(y[0], x[0]);
            let (t, u) = (cross(w, q) / den, cross(w, r) / den);
            if t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9 {
                return true;
            }
        }
    }
    false
}
