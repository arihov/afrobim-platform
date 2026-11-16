//! The corridor description of an intersection: a `CorridorModel` for `afrobim_corridor::build_model`.
//!
//! # What is returned
//!
//! * One baseline on the primary centreline with three regions: the approach behind, the through
//!   region (the stretch holding every return, tangent point and corner), and the approach ahead.
//! * One baseline per existing arm of the secondary (on the secondary alignment with its locked
//!   profile): a **mouth** region from the primary's edge to the end of the returns, and an
//!   approach region beyond. (The secondary across the primary carriageway belongs to the
//!   primary's through region; regions of a baseline must be contiguous, hence one baseline per arm.)
//! * One baseline per kerb return, on the return alignment with its blended profile, with one
//!   region over its whole length.
//!
//! The partition: the primary's lanes run to the primary's edges right through the junction; each
//! arm of the secondary runs from the primary's edge outward to the secondary's own edges (the
//! mouth is the stretch to the tangent points, then the approach); the wedge between a return
//! and the two edges, a sliver that has no width at the tangent points, is the return's region,
//! with its width taken to the corner line. On a skewed junction the sections square to the secondary
//! at the primary's edge leave small wedges between the arm and the primary uncovered or covered
//! twice: the regions are laid out by station, not clipped against one another.
//!
//! Assemblies are chosen **by name** from the entered library. The edge geometry reaches an
//! assembly through *target slots*: a part of the named assembly whose target is empty is a slot
//! and is filled here. A slot is `Dim::ToOffset(OffsetTarget::Polyline(vec![]))` (a width to a
//! line), `LinkMode::ToOffset { target: OffsetTarget::Polyline(vec![]) }`,
//! `LinkMode::ToElevation { target: ElevationTarget::Table(vec![]) }` or the same with an empty
//! `Profile`. The slot on a side of the assembly gets:
//!
//! | Region | Offset slot, left and right | Level slot, left and right |
//! |---|---|---|
//! | primary approach and through | the primary's left and right edge alignments | edge levels as a table keyed by primary station |
//! | secondary mouth and approach | the secondary's own left and right edge alignments | edge levels keyed by secondary station |
//! | return (the assembly's right side is used, placed on the paved side) | on the paved side only: the corner line, from the primary tangent point along the primary edge to the corner and along the secondary edge to the secondary tangent point | none |
//!
//! The corridor crate's `ElevationTarget` is a function of the **baseline's** station, so an
//! edge profile (a function of the edge's own station) cannot be given as it is: it is sampled at
//! the baseline stations into a table, at the entered target step.
//!
//! # What the corridor crate cannot express
//!
//! See [`JunctionCorridor::missing`], which lists exactly what is missing.

use afrobim_alignment::Alignment;
use afrobim_corridor::{Assembly, AssemblyRegion, Baseline, CorridorModel, Dim, ElevationTarget, Frequency, LinkMode, OffsetTarget, Side, Strip, SubKind, VolumeOptions};
use serde::{Deserialize, Serialize};

use crate::crossing::{find_crossings, Crossing};
use crate::error::{Finding, JunctionError, Severity};
use crate::geom::{cross, dir, polyline_between, station_offset, Pt};
use crate::intersection::{EdgeResult, IntersectionInputs, ReturnResult, RoadId};
use crate::kerb::Arm;
use crate::vertical::SurfaceLevel;

/// Inputs of the corridor description. Names, frequencies and the target step are entered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorridorInputs {
    /// Name of the model (default: the intersection's name).
    #[serde(default)]
    pub name: Option<String>,
    /// The assemblies to choose from.
    pub library: Vec<Assembly>,
    pub primary_approach_assembly: String,
    pub primary_through_assembly: String,
    pub secondary_approach_assembly: String,
    pub secondary_mouth_assembly: String,
    pub return_assembly: String,
    pub approach_frequency: Frequency,
    pub through_frequency: Frequency,
    pub mouth_frequency: Frequency,
    pub return_frequency: Frequency,
    /// Spacing of the points of a target polyline and of a level table, m.
    pub target_step_m: f64,
    /// How far in from each end of a region its first and last section are placed, m. A section
    /// line that passes exactly through an end vertex of a target polyline may or may not meet it
    /// (rounding decides), and the paved sliver of a return has no width at its tangent points; the
    /// inset is a numerical matter, not a design value, and is entered: a few millimetres do.
    pub end_inset_m: f64,
}

/// A slot of an assembly that was filled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetBinding {
    pub baseline: String,
    pub region: String,
    /// `left offset`, `right level` ...
    pub slot: String,
    /// What it was filled from.
    pub source: String,
    /// Points in the polyline or rows in the table.
    pub points: usize,
}

/// The model and what goes with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JunctionCorridor {
    pub model: CorridorModel,
    /// The alignments the baselines name, with profiles: pass them to `build_model` (the primary
    /// as given, the locked secondary, and the returns).
    pub alignments: Vec<Alignment>,
    pub bindings: Vec<TargetBinding>,
    pub notes: Vec<String>,
    /// What the corridor crate cannot express for this junction. The geometry is returned anyway.
    pub missing: Vec<String>,
}

#[derive(Default, Clone)]
struct Slots {
    left_offset: Option<(Vec<Pt>, String)>,
    right_offset: Option<(Vec<Pt>, String)>,
    left_level: Option<(Vec<(f64, f64)>, String)>,
    right_level: Option<(Vec<(f64, f64)>, String)>,
}

struct Binder<'a> {
    baseline: &'a str,
    region: &'a str,
    out: &'a mut Vec<TargetBinding>,
}

impl Binder<'_> {
    fn offset(&mut self, side: &str, slots: &Option<(Vec<Pt>, String)>, t: &mut OffsetTarget) -> bool {
        if let (OffsetTarget::Polyline(v), Some((pts, src))) = (&*t, slots) {
            if v.is_empty() {
                self.out.push(TargetBinding { baseline: self.baseline.into(), region: self.region.into(), slot: format!("{side} offset"), source: src.clone(), points: pts.len() });
                *t = OffsetTarget::Polyline(pts.clone());
                return true;
            }
        }
        false
    }
    fn level(&mut self, side: &str, slots: &Option<(Vec<(f64, f64)>, String)>, t: &mut ElevationTarget) -> bool {
        if let Some((rows, src)) = slots {
            let empty = match &*t {
                ElevationTarget::Table(v) => v.is_empty(),
                ElevationTarget::Profile(p) => p.pvis.is_empty(),
                ElevationTarget::Fixed(_) => false,
            };
            if empty {
                self.out.push(TargetBinding { baseline: self.baseline.into(), region: self.region.into(), slot: format!("{side} level"), source: src.clone(), points: rows.len() });
                *t = ElevationTarget::Table(rows.clone());
                return true;
            }
        }
        false
    }
    fn dim(&mut self, side: &str, slots: &Option<(Vec<Pt>, String)>, d: &mut Dim) -> bool {
        match d {
            Dim::ToOffset(t) => self.offset(side, slots, t),
            Dim::Fixed(_) => false,
        }
    }
    fn strip(&mut self, side: &str, slots: &Option<(Vec<Pt>, String)>, s: &mut Strip) -> bool {
        self.dim(side, slots, &mut s.width)
    }
}

fn bind_assembly(a: &Assembly, slots: &Slots, binder: &mut Binder) -> Assembly {
    let mut a = a.clone();
    for (list, side, off, lev) in [(&mut a.left, "left", &slots.left_offset, &slots.left_level), (&mut a.right, "right", &slots.right_offset, &slots.right_level)] {
        for sub in list.iter_mut() {
            match &mut sub.kind {
                SubKind::Lane(s) | SubKind::Shoulder(s) | SubKind::Footway(s) | SubKind::Verge(s) => {
                    binder.strip(side, off, s);
                }
                SubKind::KerbAndChannel { channel, .. } => {
                    binder.strip(side, off, channel);
                }
                SubKind::Link(l) => match &mut l.mode {
                    LinkMode::WidthSlope { width, .. } => {
                        binder.dim(side, off, width);
                    }
                    LinkMode::ToOffset { target, .. } => {
                        binder.offset(side, off, target);
                    }
                    LinkMode::ToElevation { target, .. } => {
                        binder.level(side, lev, target);
                    }
                },
                _ => {}
            }
        }
    }
    a
}

fn lib<'a>(ci: &'a CorridorInputs, name: &str) -> Result<&'a Assembly, JunctionError> {
    ci.library.iter().find(|a| a.name == name).ok_or_else(|| JunctionError::Corridor(format!("there is no assembly named '{name}' in the library")))
}

fn level_table(surface: &SurfaceLevel, edges: &crate::edges::RoadEdges, side: Side, s0: f64, s1: f64, step: f64) -> Result<Vec<(f64, f64)>, JunctionError> {
    let n = (((s1 - s0) / step).ceil() as usize).clamp(1, 100_000);
    (0..=n)
        .map(|i| {
            let s = if i == n { s1 } else { s0 + (s1 - s0) * i as f64 / n as f64 };
            surface.at_station_offset(s, side.sign() * edges.offset_at(side, s)).map(|z| (s, z))
        })
        .collect()
}

/// Builds the corridor description. See the module documentation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_corridor(
    inputs: &IntersectionInputs,
    ci: &CorridorInputs,
    primary: &Alignment,
    secondary: &Alignment,
    crossing: &Crossing,
    edges: &[EdgeResult],
    returns: &[ReturnResult],
    (p_lo, p_hi): (f64, f64),
    (s_lo, s_hi): (f64, f64),
    findings: &mut Vec<Finding>,
) -> Result<JunctionCorridor, JunctionError> {
    let vin = inputs.vertical.as_ref().ok_or_else(|| JunctionError::Corridor("the corridor description needs the vertical inputs (profiles for the baselines)".into()))?;
    if !(ci.end_inset_m.is_finite() && ci.end_inset_m > 0.0) {
        return Err(JunctionError::Corridor(format!("the end inset must be greater than zero (got {})", ci.end_inset_m)));
    }
    if !(ci.target_step_m.is_finite() && ci.target_step_m > 0.0) {
        return Err(JunctionError::Corridor(format!("the target step must be greater than zero (got {})", ci.target_step_m)));
    }
    if returns.is_empty() {
        return Err(JunctionError::Corridor("there are no returns to build regions around".into()));
    }
    let step = ci.target_step_m;
    let inset = ci.end_inset_m;
    let a_pa = lib(ci, &ci.primary_approach_assembly)?;
    let a_pt = lib(ci, &ci.primary_through_assembly)?;
    let a_sa = lib(ci, &ci.secondary_approach_assembly)?;
    let a_sm = lib(ci, &ci.secondary_mouth_assembly)?;
    let a_rt = lib(ci, &ci.return_assembly)?;
    let mut bindings = Vec::new();
    let mut baselines = Vec::new();
    let mut alignments: Vec<Alignment> = vec![primary.clone(), secondary.clone()];
    let name = &inputs.name;
    let edge = |road: RoadId, side: Side| edges.iter().find(|e| e.road == road && e.side == side).expect("four edges");
    let region = |n: &str, a: f64, b: f64, asm: Assembly, f: &Frequency| AssemblyRegion { name: n.to_string(), start_station_m: a, end_station_m: b, assembly: asm, frequency: f.clone() };

    // Primary.
    let (mut j0, mut j1) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in returns {
        for p in [r.geometry.corner, r.geometry.primary_point, r.geometry.secondary_point] {
            let (s, _) = station_offset(primary, p)?;
            j0 = j0.min(s);
            j1 = j1.max(s);
        }
    }
    let (j0, j1) = (j0.max(p_lo), j1.min(p_hi));
    let pw = crate::geom::slice_alignment(primary, p_lo, p_hi, "primary window")?;
    let surf_p = SurfaceLevel { road: &pw, crossfall: &vin.primary_crossfall };
    let slots_p = |lo: f64, hi: f64| -> Result<Slots, JunctionError> {
        let (el, er) = (edge(RoadId::Primary, Side::Left), edge(RoadId::Primary, Side::Right));
        Ok(Slots {
            left_offset: Some((polyline_between(&el.alignment, el.alignment.start_station_m, el.alignment.end_station_m(), step), el.alignment.name.clone())),
            right_offset: Some((polyline_between(&er.alignment, er.alignment.start_station_m, er.alignment.end_station_m(), step), er.alignment.name.clone())),
            left_level: Some((level_table(&surf_p, &inputs.primary_edges, Side::Left, lo, hi, step)?, format!("{} levels", el.alignment.name))),
            right_level: Some((level_table(&surf_p, &inputs.primary_edges, Side::Right, lo, hi, step)?, format!("{} levels", er.alignment.name))),
        })
    };
    let bname = format!("{name} primary");
    let slots = slots_p(p_lo, p_hi)?;
    let mut regions = Vec::new();
    let add = |regions: &mut Vec<AssemblyRegion>, bindings: &mut Vec<TargetBinding>, n: &str, a: f64, b: f64, asm: &Assembly, f: &Frequency, slots: &Slots| {
        if b - a > 1e-3 {
            let mut binder = Binder { baseline: &bname, region: n, out: bindings };
            let bound = bind_assembly(asm, slots, &mut binder);
            regions.push(region(n, a, b, bound, f));
        }
    };
    add(&mut regions, &mut bindings, "approach behind", p_lo + inset, j0, a_pa, &ci.approach_frequency, &slots);
    add(&mut regions, &mut bindings, "through", j0, j1, a_pt, &ci.through_frequency, &slots);
    add(&mut regions, &mut bindings, "approach ahead", j1, p_hi - inset, a_pa, &ci.approach_frequency, &slots);
    baselines.push(Baseline { name: bname, alignment: primary.name.clone(), regions, crossfall_table: vec![] });

    // Secondary arms.
    let sw = crate::geom::slice_alignment(secondary, s_lo, s_hi, "secondary window")?;
    let surf_s = SurfaceLevel { road: &sw, crossfall: &vin.secondary_crossfall };
    let (u, v) = (dir(crossing.heading_a_rad), dir(crossing.heading_b_rad));
    for arm in [Arm::Behind, Arm::Ahead] {
        let exists = if arm == Arm::Ahead { crossing.arms_b.ahead } else { crossing.arms_b.behind };
        let rs: Vec<&ReturnResult> = returns.iter().filter(|r| r.geometry.quadrant.secondary_arm == arm).collect();
        if !exists || rs.is_empty() {
            continue;
        }
        let dir_b = (v.0 * arm.sign(), v.1 * arm.sign());
        let primary_side = if cross(u, dir_b) > 0.0 { Side::Left } else { Side::Right };
        let pe = &edge(RoadId::Primary, primary_side).alignment;
        let s_edge = find_crossings(secondary, pe, 0.0)
            .into_iter()
            .filter(|c| (c.station_a_m - crossing.station_b_m) * arm.sign() > 0.0)
            .min_by(|a, b| (a.station_a_m - crossing.station_b_m).abs().total_cmp(&(b.station_a_m - crossing.station_b_m).abs()))
            .map(|c| c.station_a_m)
            .ok_or_else(|| JunctionError::Corridor("the secondary does not reach the primary's edge on the side of its arm".into()))?;
        let mut j = if arm == Arm::Ahead { f64::NEG_INFINITY } else { f64::INFINITY };
        for r in &rs {
            for p in [r.geometry.corner, r.geometry.primary_point, r.geometry.secondary_point] {
                let (s, _) = station_offset(secondary, p)?;
                j = if arm == Arm::Ahead { j.max(s) } else { j.min(s) };
            }
        }
        let (lo, hi) = if arm == Arm::Ahead { (s_edge + inset, s_hi - inset) } else { (s_lo + inset, s_edge - inset) };
        let j = j.clamp(lo, hi);
        // The secondary's own edge on a side, the whole alignment: the arm's lanes run to it from the
        // primary's edge outward. The paved wedge between a return and the two edges is the return's.
        let outline = |side: Side| -> Option<(Vec<Pt>, String)> {
            let e = edge(RoadId::Secondary, side);
            Some((polyline_between(&e.alignment, e.alignment.start_station_m, e.alignment.end_station_m(), step), e.alignment.name.clone()))
        };
        let levels = |side: Side| -> Result<Option<(Vec<(f64, f64)>, String)>, JunctionError> {
            Ok(Some((level_table(&surf_s, &inputs.secondary_edges, side, lo, hi, step)?, format!("{} {:?} edge levels", secondary.name, side))))
        };
        let slots = Slots { left_offset: outline(Side::Left), right_offset: outline(Side::Right), left_level: levels(Side::Left)?, right_level: levels(Side::Right)? };
        let bname = format!("{name} secondary {}", if arm == Arm::Ahead { "ahead" } else { "behind" });
        let mut regions = Vec::new();
        let mut add2 = |n: &str, a: f64, b: f64, asm: &Assembly, f: &Frequency| {
            if b - a > 1e-3 {
                let mut binder = Binder { baseline: &bname, region: n, out: &mut bindings };
                let bound = bind_assembly(asm, &slots, &mut binder);
                regions.push(region(n, a, b, bound, f));
            }
        };
        if arm == Arm::Ahead {
            add2("mouth", lo, j, a_sm, &ci.mouth_frequency);
            add2("approach", j, hi, a_sa, &ci.approach_frequency);
        } else {
            add2("approach", lo, j, a_sa, &ci.approach_frequency);
            add2("mouth", j, hi, a_sm, &ci.mouth_frequency);
        }
        baselines.push(Baseline { name: bname, alignment: secondary.name.clone(), regions, crossfall_table: vec![] });
    }

    // Returns.
    for r in returns {
        let g = &r.geometry;
        let e1 = &edge(RoadId::Primary, g.primary_side).alignment;
        let e2 = &edge(RoadId::Secondary, g.secondary_side).alignment;
        let mut corner = polyline_between(e1, g.primary_edge_station_m.min(g.corner_primary_edge_station_m), g.primary_edge_station_m.max(g.corner_primary_edge_station_m), step);
        if g.primary_edge_station_m > g.corner_primary_edge_station_m {
            corner.reverse();
        }
        let mut second = polyline_between(e2, g.secondary_edge_station_m.min(g.corner_secondary_edge_station_m), g.secondary_edge_station_m.max(g.corner_secondary_edge_station_m), step);
        if g.secondary_edge_station_m < g.corner_secondary_edge_station_m {
            second.reverse();
        }
        corner.extend(second);
        let src = format!("corner line of {}", g.alignment.name);
        let slots = if g.pavement_side == Side::Left {
            Slots { left_offset: Some((corner, src)), ..Default::default() }
        } else {
            Slots { right_offset: Some((corner, src)), ..Default::default() }
        };
        let bname = format!("{name} return {}", g.quadrant.label());
        // The return assembly is written for its paved side as its *right* side; it goes on the side
        // the pavement is on, and the other side is left empty.
        let mut placed = Assembly::new(&a_rt.name);
        placed.baseline_offset_m = a_rt.baseline_offset_m;
        placed.baseline_elevation_m = a_rt.baseline_elevation_m;
        if g.pavement_side == Side::Left {
            placed.left = a_rt.right.clone();
        } else {
            placed.right = a_rt.right.clone();
        }
        let mut binder = Binder { baseline: &bname, region: "return", out: &mut bindings };
        let asm = bind_assembly(&placed, &slots, &mut binder);
        baselines.push(Baseline {
            name: bname,
            alignment: g.alignment.name.clone(),
            regions: vec![region("return", inset, g.length_m - inset, asm, &ci.return_frequency)],
            crossfall_table: vec![],
        });
        alignments.push(g.alignment.clone());
    }
    for b in &baselines {
        b.check().map_err(JunctionError::Corridor)?;
    }
    let mut notes = Vec::new();
    for (asm, what) in [(a_pa, "primary approach"), (a_pt, "primary through"), (a_sa, "secondary approach"), (a_sm, "secondary mouth"), (a_rt, "return")] {
        if !assembly_has_slot(asm) {
            notes.push(format!("assembly '{}' ({what}) has no empty target slot, so no edge geometry reaches it: its widths and levels are its own", asm.name));
        }
    }
    notes.dedup();
    if bindings.is_empty() {
        findings.push(Finding::new("corridor.no_targets", Severity::Warning, name, "no assembly has an empty target slot: the edges, returns and levels are returned but not attached to any region".into(), &[]));
    }
    let missing = vec![
        "No link takes an offset target and an elevation target together (Civil 3D's link to offset and elevation targets). The paved quadrant between a return and the two edges gets its width from the corner line but its level from the assembly's own slope; the levels of the edges are given as tables keyed by baseline station, and the cross slope they imply has to be entered in the assembly.".to_string(),
        "ElevationTarget reads the baseline's station only, so an edge profile (a function of the edge's station) is converted into a level table at the baseline stations; a kerb return's own levels are its baseline profile.".to_string(),
        "A baseline's regions must be contiguous and an offset target is met by a section line square to the baseline: the primary through region covers the secondary across the primary carriageway, and the secondary has one baseline per arm.".to_string(),
        "Islands (channelising or splitter) are returned as outlines only: the corridor crate has no region bounded by an outline.".to_string(),
    ];
    Ok(JunctionCorridor {
        model: CorridorModel { name: ci.name.clone().unwrap_or_else(|| inputs.name.clone()), baselines, volumes: VolumeOptions::default() },
        alignments,
        bindings,
        notes,
        missing,
    })
}

fn assembly_has_slot(a: &Assembly) -> bool {
    let slot_dim = |d: &Dim| matches!(d, Dim::ToOffset(OffsetTarget::Polyline(v)) if v.is_empty());
    a.left.iter().chain(&a.right).any(|s| match &s.kind {
        SubKind::Lane(x) | SubKind::Shoulder(x) | SubKind::Footway(x) | SubKind::Verge(x) => slot_dim(&x.width),
        SubKind::KerbAndChannel { channel, .. } => slot_dim(&channel.width),
        SubKind::Link(l) => match &l.mode {
            LinkMode::WidthSlope { width, .. } => slot_dim(width),
            LinkMode::ToOffset { target: OffsetTarget::Polyline(v), .. } => v.is_empty(),
            LinkMode::ToElevation { target: ElevationTarget::Table(v), .. } => v.is_empty(),
            LinkMode::ToElevation { target: ElevationTarget::Profile(p), .. } => p.pvis.is_empty(),
            _ => false,
        },
        _ => false,
    })
}
