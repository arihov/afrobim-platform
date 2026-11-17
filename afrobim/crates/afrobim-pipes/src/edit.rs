//! Layout and editing: create a run from a sequence of positions, insert, move and delete
//! structures, swap parts, connect and disconnect, set inverts by slope, drop or cover, and apply
//! a rule set.
//!
//! Levels are never invented: a run needs the start invert and slope from the caller, and rules
//! that need a value (minimum cover, a match mode) do nothing when it is not given.

use serde::{Deserialize, Serialize};

use crate::error::PipesError;
use crate::finding::{Finding, Severity};
use crate::ground::Ground;
use crate::model::*;
use crate::parts::Part;
use crate::rules::RuleSet;

/// What a new pipe is made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeSpec {
    pub shape: PipeShape,
    pub material: Option<String>,
    pub wall_thickness_m: Option<f64>,
    pub roughness: Option<Roughness>,
    pub part: Option<String>,
}

impl PipeSpec {
    pub fn new(shape: PipeShape) -> PipeSpec {
        PipeSpec { shape, material: None, wall_thickness_m: None, roughness: None, part: None }
    }

    /// The spec of a pipe part.
    pub fn from_part(part: &Part) -> Result<PipeSpec, PipesError> {
        let mut dummy = Pipe {
            id: PipeId(0),
            name: String::new(),
            start: StructureId(0),
            end: StructureId(0),
            start_invert_m: 0.0,
            end_invert_m: 0.0,
            shape: PipeShape::Circular { diameter_m: 1.0 },
            wall_thickness_m: None,
            material: None,
            roughness: None,
            part: None,
            design_flow_m3_s: None,
            via: None,
            description: None,
        };
        part.apply_to_pipe(&mut dummy)?;
        Ok(PipeSpec { shape: dummy.shape, material: dummy.material, wall_thickness_m: dummy.wall_thickness_m, roughness: dummy.roughness, part: dummy.part })
    }
}

/// One position of a run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RunPoint {
    pub easting_m: f64,
    pub northing_m: f64,
    pub rim_level_m: f64,
}

/// How a run is built from positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSpec {
    pub structure_kind: StructureKind,
    pub structure_prefix: String,
    pub pipe_prefix: String,
    pub pipe: PipeSpec,
    /// Invert of the first pipe at the first structure, m.
    pub start_invert_m: f64,
    /// Fall of every pipe in m/m on plan length (positive falls downstream). Pipes join end to
    /// start at the same invert: no drop across structures until you set one.
    pub slope: f64,
}

/// Which end of a pipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipeEnd {
    Start,
    End,
}

/// What deleting a structure did to its pipes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteOutcome {
    pub removed_pipes: Vec<PipeId>,
    /// The pipe that took the place of an incoming and an outgoing pipe, when they were merged.
    pub merged_into: Option<PipeId>,
}

fn finite(v: f64, what: &str) -> Result<(), PipesError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(PipesError::Invalid(format!("{what} must be a finite number")))
    }
}

impl PipeNetwork {
    /// Add a structure. Its name is `S<id>` when `name` is empty.
    pub fn add_structure(&mut self, name: &str, kind: StructureKind, easting_m: f64, northing_m: f64, rim_level_m: f64) -> Result<StructureId, PipesError> {
        finite(easting_m, "easting")?;
        finite(northing_m, "northing")?;
        finite(rim_level_m, "rim level")?;
        let id = self.alloc_structure();
        let s = Structure {
            id,
            name: if name.is_empty() { id.to_string() } else { name.to_string() },
            kind,
            easting_m,
            northing_m,
            rim_level_m,
            sump_level_m: None,
            inner: None,
            wall_thickness_m: None,
            part: None,
            inflow_m3_s: 0.0,
            loss_coefficient: None,
            description: None,
        };
        self.structures.insert(id, s);
        Ok(id)
    }

    /// Connect two structures with a pipe from `start` to `end` at the given inverts.
    pub fn connect(&mut self, start: StructureId, end: StructureId, spec: &PipeSpec, start_invert_m: f64, end_invert_m: f64, name: Option<&str>) -> Result<PipeId, PipesError> {
        self.structure(start)?;
        self.structure(end)?;
        if start == end {
            return Err(PipesError::Invalid("a pipe cannot start and end at the same structure".to_string()));
        }
        if !spec.shape.is_valid() {
            return Err(PipesError::Invalid("the pipe shape needs positive dimensions".to_string()));
        }
        finite(start_invert_m, "start invert")?;
        finite(end_invert_m, "end invert")?;
        let id = self.alloc_pipe();
        let pipe = Pipe {
            id,
            name: name.map(str::to_string).unwrap_or_else(|| id.to_string()),
            start,
            end,
            start_invert_m,
            end_invert_m,
            shape: spec.shape.clone(),
            wall_thickness_m: spec.wall_thickness_m,
            material: spec.material.clone(),
            roughness: spec.roughness,
            part: spec.part.clone(),
            design_flow_m3_s: None,
            via: None,
            description: None,
        };
        self.pipes.insert(id, pipe);
        Ok(id)
    }

    /// Remove a pipe. Its structures stay.
    pub fn disconnect(&mut self, pipe: PipeId) -> Result<Pipe, PipesError> {
        self.pipes.remove(&pipe).ok_or(PipesError::NoPipe(pipe))
    }

    /// Create structures and pipes from a sequence of positions: a structure at each position and
    /// a pipe between each consecutive pair, falling at `spec.slope` from `spec.start_invert_m`.
    pub fn add_run(&mut self, positions: &[RunPoint], spec: &RunSpec) -> Result<(Vec<StructureId>, Vec<PipeId>), PipesError> {
        if positions.len() < 2 {
            return Err(PipesError::Invalid("a run needs at least two positions".to_string()));
        }
        finite(spec.start_invert_m, "start invert")?;
        finite(spec.slope, "slope")?;
        let mut sids = Vec::new();
        for p in positions {
            let id = self.add_structure("", spec.structure_kind, p.easting_m, p.northing_m, p.rim_level_m)?;
            if !spec.structure_prefix.is_empty() {
                let id_num = id.0;
                self.structure_mut(id)?.name = format!("{}{}", spec.structure_prefix, id_num);
            }
            sids.push(id);
        }
        let mut pids = Vec::new();
        let mut invert = spec.start_invert_m;
        for w in sids.windows(2) {
            let (a, b) = (self.position(w[0])?, self.position(w[1])?);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            let next = invert - spec.slope * l;
            let pid = self.connect(w[0], w[1], &spec.pipe, invert, next, None)?;
            if !spec.pipe_prefix.is_empty() {
                self.pipe_mut(pid)?.name = format!("{}{}", spec.pipe_prefix, pid.0);
            }
            pids.push(pid);
            invert = next;
        }
        Ok((sids, pids))
    }

    /// Split a pipe with a new structure at a position. The structure is put at the position
    /// given; the inverts of the two new pipes are interpolated linearly with the distance along
    /// the old chord (the foot of the position on the start-to-end line, kept within the pipe).
    /// A curved pipe loses its curve: both halves are straight. The first half keeps the old
    /// pipe id, the second is new. Returns the new structure and the new pipe.
    pub fn split_pipe(&mut self, pipe: PipeId, name: &str, kind: StructureKind, easting_m: f64, northing_m: f64, rim_level_m: f64) -> Result<(StructureId, PipeId), PipesError> {
        let old = self.pipe(pipe)?.clone();
        let a = self.position(old.start)?;
        let b = self.position(old.end)?;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((easting_m - a[0]) * dx + (northing_m - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let mid_invert = old.start_invert_m + (old.end_invert_m - old.start_invert_m) * t;
        let s = self.add_structure(name, kind, easting_m, northing_m, rim_level_m)?;
        let second = self.alloc_pipe();
        let mut p2 = old.clone();
        p2.id = second;
        p2.name = format!("{}-b", old.name);
        p2.start = s;
        p2.start_invert_m = mid_invert;
        p2.via = None;
        self.pipes.insert(second, p2);
        let p1 = self.pipe_mut(pipe)?;
        p1.end = s;
        p1.end_invert_m = mid_invert;
        p1.via = None;
        p1.name = format!("{}-a", old.name);
        Ok((s, second))
    }

    /// Move a structure. The inverts of its pipes are left as they are, so their slopes change.
    pub fn move_structure(&mut self, id: StructureId, easting_m: f64, northing_m: f64) -> Result<(), PipesError> {
        finite(easting_m, "easting")?;
        finite(northing_m, "northing")?;
        let s = self.structure_mut(id)?;
        s.easting_m = easting_m;
        s.northing_m = northing_m;
        Ok(())
    }

    /// Delete a structure and every pipe connected to it.
    pub fn delete_structure(&mut self, id: StructureId) -> Result<DeleteOutcome, PipesError> {
        self.structure(id)?;
        let removed: Vec<PipeId> = self.pipes().filter(|p| p.start == id || p.end == id).map(|p| p.id).collect();
        for p in &removed {
            self.pipes.remove(p);
        }
        self.structures.remove(&id);
        Ok(DeleteOutcome { removed_pipes: removed, merged_into: None })
    }

    /// Delete a structure. If exactly one pipe enters and one leaves, they are joined into one
    /// pipe (the incoming pipe's id and properties, from its start invert to the outgoing pipe's
    /// end invert, curve dropped); otherwise as [`PipeNetwork::delete_structure`].
    pub fn delete_structure_merge(&mut self, id: StructureId) -> Result<DeleteOutcome, PipesError> {
        self.structure(id)?;
        let (ins, outs) = (self.incoming(id), self.outgoing(id));
        if ins.len() == 1 && outs.len() == 1 && self.pipe(ins[0])?.start != self.pipe(outs[0])?.end {
            let out = self.pipes.remove(&outs[0]).ok_or(PipesError::NoPipe(outs[0]))?;
            let p = self.pipe_mut(ins[0])?;
            p.end = out.end;
            p.end_invert_m = out.end_invert_m;
            p.via = None;
            self.structures.remove(&id);
            return Ok(DeleteOutcome { removed_pipes: vec![out.id], merged_into: Some(ins[0]) });
        }
        self.delete_structure(id)
    }

    /// Swap the part of a pipe: shape, material, wall, roughness (when the part has one) and part
    /// reference change. **Inverts do not move**, so the crown and cover change with the size;
    /// re-apply the rules or set inverts again.
    pub fn swap_pipe_part(&mut self, pipe: PipeId, part: &Part) -> Result<(), PipesError> {
        let p = self.pipe_mut(pipe)?;
        part.apply_to_pipe(p)
    }

    /// Swap the part of a structure: kind, inner dimensions, wall and part reference.
    pub fn swap_structure_part(&mut self, structure: StructureId, part: &Part) -> Result<(), PipesError> {
        let s = self.structure_mut(structure)?;
        part.apply_to_structure(s)
    }

    /// Set both inverts of a pipe from one end and a slope: the other end is
    /// `anchor_invert -/+ slope * plan length` (slope positive falls from start to end).
    pub fn set_inverts_by_slope(&mut self, pipe: PipeId, anchor: PipeEnd, anchor_invert_m: f64, slope: f64) -> Result<(), PipesError> {
        finite(anchor_invert_m, "invert")?;
        finite(slope, "slope")?;
        let l = self.plan_length_m(pipe)?;
        let p = self.pipe_mut(pipe)?;
        match anchor {
            PipeEnd::Start => {
                p.start_invert_m = anchor_invert_m;
                p.end_invert_m = anchor_invert_m - slope * l;
            }
            PipeEnd::End => {
                p.end_invert_m = anchor_invert_m;
                p.start_invert_m = anchor_invert_m + slope * l;
            }
        }
        Ok(())
    }

    /// Set the inverts of the pipes leaving a structure so that their matched level (invert,
    /// crown or centreline) is `drop_m` below the lowest matched level of the pipes entering.
    pub fn set_drop_across_structure(&mut self, structure: StructureId, drop_m: f64, mode: crate::rules::MatchMode) -> Result<(), PipesError> {
        finite(drop_m, "drop")?;
        self.structure(structure)?;
        let ins = self.incoming(structure);
        let outs = self.outgoing(structure);
        if ins.is_empty() || outs.is_empty() {
            return Err(PipesError::Invalid("a drop needs a pipe entering and a pipe leaving the structure".to_string()));
        }
        let mut level = f64::INFINITY;
        for i in &ins {
            let p = self.pipe(*i)?;
            level = level.min(mode.pipe_end_level_m(p, false));
        }
        for o in outs {
            let p = self.pipe_mut(o)?;
            p.start_invert_m = mode.invert_for_level_m(&p.shape, level - drop_m);
        }
        Ok(())
    }

    /// Set both inverts of a pipe so that the cover at each end is `cover_m`: ground level minus
    /// cover minus wall thickness minus inside height. Errors when the ground is not defined at an end.
    pub fn set_inverts_by_cover(&mut self, pipe: PipeId, ground: &dyn Ground, cover_m: f64) -> Result<(), PipesError> {
        finite(cover_m, "cover")?;
        let p = self.pipe(pipe)?.clone();
        let (a, b) = (self.position(p.start)?, self.position(p.end)?);
        let ga = ground.level_m(a[0], a[1]).ok_or_else(|| PipesError::Invalid(format!("the ground is not defined at the start of pipe {}", p.name)))?;
        let gb = ground.level_m(b[0], b[1]).ok_or_else(|| PipesError::Invalid(format!("the ground is not defined at the end of pipe {}", p.name)))?;
        let down = p.wall_thickness_m.unwrap_or(0.0) + p.shape.height_m();
        let pm = self.pipe_mut(pipe)?;
        pm.start_invert_m = ga - cover_m - down;
        pm.end_invert_m = gb - cover_m - down;
        Ok(())
    }

    /// Apply a rule set, working from the outlets upstream.
    ///
    /// The end invert of each outlet pipe is the anchor and is not changed. Then, for each pipe
    /// in turn, downstream first:
    ///
    /// 1. **End.** If `match_mode` is set and the end structure has a pipe leaving it, the pipe's
    ///    end invert is set so its matched level is `structure_drop_m` (zero if not set) above
    ///    the leaving pipe's matched level. (With several pipes entering, each is matched to the
    ///    leaving pipe.)
    /// 2. **Start.** If `min_cover_m` is set and the ground is defined at the start, the start
    ///    invert is the shallowest the cover allows. Otherwise it is kept.
    /// 3. **Slope.** The fall over the plan length is brought inside `min_slope` and `max_slope`
    ///    by moving the **start** invert (upstream end), never the end invert.
    ///
    /// Rules that cannot be met together (for example a minimum slope that pushes the start above
    /// the cover limit), and the length rule, which moves nothing, are reported by the returned
    /// findings (cover, slope and length checks run after the changes; cover is checked at the ends only).
    /// A network with a loop is refused. Branches are processed but the check says they are not a tree.
    pub fn apply_rules(&mut self, rules: &RuleSet, ground: Option<&dyn Ground>) -> Result<Vec<Finding>, PipesError> {
        if rules.structure_drop_m.is_some() && rules.match_mode.is_none() {
            return Err(PipesError::Invalid("a drop across structures needs a match mode (invert, crown or centreline)".to_string()));
        }
        for (v, what) in [(rules.min_slope, "minimum slope"), (rules.max_slope, "maximum slope"), (rules.min_cover_m, "minimum cover"), (rules.max_cover_m, "maximum cover")] {
            if let Some(v) = v {
                finite(v, what)?;
            }
        }
        if let (Some(a), Some(b)) = (rules.min_slope, rules.max_slope) {
            if a > b {
                return Err(PipesError::Invalid("the minimum slope is larger than the maximum slope".to_string()));
            }
        }
        let topo = self.topology();
        if !topo.loop_pipes.is_empty() {
            return Err(PipesError::Invalid("the network has a loop; rules are not applied".to_string()));
        }
        let index: std::collections::BTreeMap<StructureId, usize> = topo.order.iter().enumerate().map(|(i, s)| (*s, i)).collect();
        let mut pipes: Vec<PipeId> = self.pipes().map(|p| p.id).collect();
        pipes.sort_by_key(|p| std::cmp::Reverse(self.pipes.get(p).and_then(|p| index.get(&p.start)).copied().unwrap_or(0)));
        for pid in pipes {
            let p = self.pipe(pid)?.clone();
            let l = self.plan_length_m(pid)?;
            // 1. end invert by matching to the pipe leaving the end structure
            if let Some(mode) = rules.match_mode {
                if let Some(q) = self.outgoing(p.end).first().copied() {
                    let q = self.pipe(q)?;
                    let level = mode.pipe_end_level_m(q, true) + rules.structure_drop_m.unwrap_or(0.0);
                    let inv = mode.invert_for_level_m(&p.shape, level);
                    self.pipe_mut(pid)?.end_invert_m = inv;
                }
            }
            let end_invert = self.pipe(pid)?.end_invert_m;
            // 2. start invert by minimum cover
            let mut start = p.start_invert_m;
            if let (Some(min_cover), Some(g)) = (rules.min_cover_m, ground) {
                let a = self.position(p.start)?;
                if let Some(gl) = g.level_m(a[0], a[1]) {
                    start = gl - min_cover - p.wall_thickness_m.unwrap_or(0.0) - p.shape.height_m();
                }
            }
            // 3. slope limits by moving the start
            if l > 0.0 {
                let slope = (start - end_invert) / l;
                if let Some(max) = rules.max_slope {
                    if slope > max {
                        start = end_invert + max * l;
                    }
                }
                let slope = (start - end_invert) / l;
                if let Some(min) = rules.min_slope {
                    if slope < min {
                        start = end_invert + min * l;
                    }
                }
            }
            self.pipe_mut(pid)?.start_invert_m = start;
        }
        let mut findings = Vec::new();
        if let Some(g) = ground {
            findings.extend(self.check_cover(g, rules, None));
        }
        findings.extend(self.check_slope(rules));
        findings.extend(self.check_length(rules));
        if rules.min_cover_m.is_some() && ground.is_none() {
            findings.push(Finding::new("cover", Severity::Info, vec![], "no ground surface was given, so cover rules were not applied".to_string()));
        }
        Ok(findings)
    }
}
