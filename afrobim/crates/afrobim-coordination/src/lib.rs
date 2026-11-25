// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Coordination: clash detection between model elements.
//!
//! Build [`Body`] values (an element's solids in world millimetres), wrap them in a [`Scene`],
//! run [`ClashTest`]s, and merge the clashes into [`ClashResults`] that persist with the
//! document. Pure geometry: no engineering values, every tolerance is an input.
//! See the README for the algorithms and their limits.

mod b64;
pub mod bcf;
mod engine;
pub mod geom;
mod group;
mod issue;
mod results;
mod rules;
mod scene;

pub use engine::RunStats;
pub use bcf::{export_bcf, import_bcf, BcfError, BcfExport, BcfImport, BcfOptions, ImportOptions, ImportReport};
pub use geom::Aabb;
pub use group::{group_clashes, ClashGroup, GroupOptions};
pub use issue::{values, Camera, ClashLink, ElementRef, Issue, IssueComment, Projection, Viewpoint, Visibility};
pub use results::{clash_id, Clash, ClashResults, Status};
pub use rules::{ClashKind, ClashTest, Selection};
pub use scene::{Body, Placement, Scene};

/// A document's coordination state: its tests and the results of running them.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Coordination {
    pub tests: Vec<ClashTest>,
    pub results: ClashResults,
    /// Issues (free-standing or raised from clashes). Absent in documents saved before issues
    /// existed, which load with none.
    #[serde(default)]
    pub issues: Vec<Issue>,
}

impl Coordination {
    /// Runs every test on the scene and merges the outcome into the stored results.
    pub fn run(&mut self, scene: &Scene) -> Vec<RunStats> {
        // Approved clashes are skipped by the engine, so they are not "gone": merge keeps them.
        let tests: Vec<ClashTest> = self
            .tests
            .iter()
            .map(|t| {
                let mut t = t.clone();
                for pair in self.results.approved_pairs(&t.name) {
                    if !t.is_approved(pair.0, pair.1) {
                        t.approved_pairs.push(pair);
                    }
                }
                t
            })
            .collect();
        let (clashes, stats) = scene.run_all(&tests);
        let names: Vec<String> = self.tests.iter().map(|t| t.name.clone()).collect();
        self.results.merge_run(&names, clashes);
        stats
    }

    /// Raises an issue from a stored clash and adds it, unless one with the same GUID exists
    /// (the GUID derives from the clash id). Returns the issue's GUID, or `None` for an unknown
    /// clash id.
    pub fn raise_issue_from_clash(&mut self, clash_id: &str, author: &str, now: i64) -> Option<String> {
        let issue = Issue::from_clash(self.results.get(clash_id)?, author, now);
        let guid = issue.guid.clone();
        if !self.issues.iter().any(|i| i.guid == guid) {
            self.issues.push(issue);
        }
        Some(guid)
    }

    /// Adds a free-standing issue. The GUID derives from the issue count, `now` and the title,
    /// and is made unique if it still collides.
    pub fn new_issue(&mut self, title: &str, author: &str, now: i64) -> &mut Issue {
        let mut n = self.issues.len();
        let guid = loop {
            let g = afrobim_ifc::name_uuid(&format!("issue/free/{n}/{now}/{title}"));
            if !self.issues.iter().any(|i| i.guid == g) {
                break g;
            }
            n += 1;
        };
        self.issues.push(Issue::with_guid(&guid, title, author, now));
        self.issues.last_mut().unwrap()
    }

    pub fn issue(&self, guid: &str) -> Option<&Issue> {
        self.issues.iter().find(|i| i.guid == guid)
    }

    pub fn issue_mut(&mut self, guid: &str) -> Option<&mut Issue> {
        self.issues.iter_mut().find(|i| i.guid == guid)
    }

    /// Adds imported issues, replacing the ones with the same GUID (a re-imported file updates
    /// instead of duplicating). Returns how many were added and how many replaced.
    pub fn merge_issues(&mut self, imported: Vec<Issue>) -> (usize, usize) {
        let (mut added, mut replaced) = (0, 0);
        for i in imported {
            match self.issues.iter_mut().find(|x| x.guid == i.guid) {
                Some(slot) => {
                    *slot = i;
                    replaced += 1;
                }
                None => {
                    self.issues.push(i);
                    added += 1;
                }
            }
        }
        (added, replaced)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Coordination, serde_json::Error> {
        serde_json::from_str(text)
    }
}
