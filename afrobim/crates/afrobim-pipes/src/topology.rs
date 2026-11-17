//! Network topology: ordering, loops, branches, and flow accumulation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::finding::{Finding, Severity};
use crate::model::{ElementRef, PipeId, PipeNetwork, StructureId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topology {
    /// Structures from upstream to downstream (a structure after every structure that feeds it).
    /// Structures on or downstream of a loop are missing.
    pub order: Vec<StructureId>,
    /// Pipes with both ends missing from `order`: on a loop or downstream of one.
    pub loop_pipes: Vec<PipeId>,
    /// Structures with more than one pipe leaving: a branch, which a gravity tree does not have.
    pub diverging: Vec<StructureId>,
    /// Structures with pipes entering and none leaving (outlets of the network).
    pub outlets: Vec<StructureId>,
    /// Structures with no pipe at all.
    pub isolated: Vec<StructureId>,
}

impl Topology {
    /// A converging tree (or forest): no loops and no branches.
    pub fn is_tree(&self) -> bool {
        self.loop_pipes.is_empty() && self.diverging.is_empty()
    }
}

/// Flow in each pipe, m3/s, and the findings from getting there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowResult {
    pub flows: BTreeMap<PipeId, f64>,
    pub findings: Vec<Finding>,
}

impl PipeNetwork {
    pub fn topology(&self) -> Topology {
        let mut indeg: BTreeMap<StructureId, usize> = self.structures().map(|s| (s.id, 0)).collect();
        for p in self.pipes() {
            if let Some(d) = indeg.get_mut(&p.end) {
                *d += 1;
            }
        }
        let mut queue: Vec<StructureId> = indeg.iter().filter(|(_, d)| **d == 0).map(|(id, _)| *id).collect();
        let mut order = Vec::new();
        let mut qi = 0;
        while qi < queue.len() {
            let s = queue[qi];
            qi += 1;
            order.push(s);
            for p in self.pipes().filter(|p| p.start == s) {
                if let Some(d) = indeg.get_mut(&p.end) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(p.end);
                    }
                }
            }
        }
        let ordered: BTreeSet<StructureId> = order.iter().copied().collect();
        let loop_pipes = self.pipes().filter(|p| !ordered.contains(&p.start) && !ordered.contains(&p.end)).map(|p| p.id).collect();
        let mut out_count: BTreeMap<StructureId, usize> = BTreeMap::new();
        let mut in_count: BTreeMap<StructureId, usize> = BTreeMap::new();
        for p in self.pipes() {
            *out_count.entry(p.start).or_default() += 1;
            *in_count.entry(p.end).or_default() += 1;
        }
        let diverging = out_count.iter().filter(|(_, c)| **c > 1).map(|(s, _)| *s).collect();
        let outlets = self.structures().filter(|s| !out_count.contains_key(&s.id) && in_count.contains_key(&s.id)).map(|s| s.id).collect();
        let isolated = self.structures().filter(|s| !out_count.contains_key(&s.id) && !in_count.contains_key(&s.id)).map(|s| s.id).collect();
        Topology { order, loop_pipes, diverging, outlets, isolated }
    }

    pub fn is_tree(&self) -> bool {
        self.topology().is_tree()
    }

    /// Findings about the shape of the network: loops (an error: flows cannot be accumulated),
    /// branches (a warning) and structures with no pipe (information).
    pub fn topology_findings(&self) -> Vec<Finding> {
        let t = self.topology();
        let mut out = Vec::new();
        if !t.loop_pipes.is_empty() {
            let names: Vec<String> = t.loop_pipes.iter().filter_map(|id| self.pipe(*id).ok().map(|p| p.name.clone())).collect();
            out.push(Finding::new(
                "topology",
                Severity::Error,
                t.loop_pipes.iter().map(|p| ElementRef::Pipe(*p)).collect(),
                format!("the network has a loop through or upstream of pipes {}; a gravity network must be a tree, so flows are not accumulated for these pipes", names.join(", ")),
            ));
        }
        for s in &t.diverging {
            let name = self.structure(*s).map(|s| s.name.clone()).unwrap_or_default();
            let mut el = vec![ElementRef::Structure(*s)];
            el.extend(self.outgoing(*s).into_iter().map(ElementRef::Pipe));
            out.push(Finding::new(
                "topology",
                Severity::Warning,
                el,
                format!("structure {name} has {} pipes leaving it; the flow is not split between them and each is given the whole accumulated flow", self.outgoing(*s).len()),
            ));
        }
        for s in &t.isolated {
            let name = self.structure(*s).map(|s| s.name.clone()).unwrap_or_default();
            out.push(Finding::new("topology", Severity::Info, vec![ElementRef::Structure(*s)], format!("structure {name} has no pipe connected")));
        }
        out
    }

    /// Flow in every pipe: a pipe's `design_flow_m3_s` if set, otherwise the structure inflow at
    /// its start plus the flow of every pipe entering that structure. This is continuity at a
    /// node: flow leaving equals the sum entering plus the local inflow (any hydraulics text).
    /// A pipe with its own design flow passes that value downstream.
    pub fn accumulate_flows(&self) -> FlowResult {
        let topo = self.topology();
        let mut findings = self.topology_findings();
        let mut flows: BTreeMap<PipeId, f64> = BTreeMap::new();
        for s in &topo.order {
            let Ok(st) = self.structure(*s) else { continue };
            let entering: f64 = self.incoming(*s).iter().filter_map(|p| flows.get(p)).sum();
            let acc = st.inflow_m3_s + entering;
            for pid in self.outgoing(*s) {
                let Ok(p) = self.pipe(pid) else { continue };
                flows.insert(pid, p.design_flow_m3_s.unwrap_or(acc));
            }
        }
        for p in self.pipes() {
            if !flows.contains_key(&p.id) {
                findings.push(Finding::new("topology", Severity::Error, vec![ElementRef::Pipe(p.id)], format!("pipe {} has no accumulated flow because it lies on or downstream of a loop", p.name)));
            }
        }
        FlowResult { flows, findings }
    }
}
