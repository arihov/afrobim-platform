//! Coordination: clash detection between model elements.
//!
//! Build [`Body`] values (an element's solids in world millimetres), wrap them in a [`Scene`],
//! run [`ClashTest`]s, and merge the clashes into [`ClashResults`] that persist with the
//! document. Pure geometry: no engineering values, every tolerance is an input.
//! See the README for the algorithms and their limits.

mod engine;
pub mod geom;
mod results;
mod rules;
mod scene;

pub use engine::RunStats;
pub use geom::Aabb;
pub use results::{clash_id, Clash, ClashResults, Status};
pub use rules::{ClashKind, ClashTest, Selection};
pub use scene::{Body, Placement, Scene};

/// A document's coordination state: its tests and the results of running them.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Coordination {
    pub tests: Vec<ClashTest>,
    pub results: ClashResults,
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

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Coordination, serde_json::Error> {
        serde_json::from_str(text)
    }
}
