//! Clash tests: what is checked against what, and how.

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::scene::Body;

/// A set of bodies picked by category, layer or explicit ids. The criteria narrow each other
/// (a body must satisfy every non-empty list); an empty list places no restriction, so a
/// default selection takes every body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub ids: Vec<ElementId>,
}

impl Selection {
    pub fn all() -> Selection {
        Selection::default()
    }
    pub fn category(name: &str) -> Selection {
        Selection { categories: vec![name.to_string()], ..Selection::default() }
    }
    pub fn layer(name: &str) -> Selection {
        Selection { layers: vec![name.to_string()], ..Selection::default() }
    }
    pub fn ids(ids: &[ElementId]) -> Selection {
        Selection { ids: ids.to_vec(), ..Selection::default() }
    }
    pub fn matches(&self, b: &Body) -> bool {
        (self.categories.is_empty() || self.categories.contains(&b.category))
            && (self.layers.is_empty() || self.layers.contains(&b.layer))
            && (self.ids.is_empty() || self.ids.contains(&b.id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClashKind {
    /// The solids overlap.
    Hard,
    /// The solids come closer than the tolerance (touching and overlapping included).
    Clearance,
}

impl ClashKind {
    pub fn label(self) -> &'static str {
        match self {
            ClashKind::Hard => "hard",
            ClashKind::Clearance => "clearance",
        }
    }
}

fn yes() -> bool {
    true
}
fn default_eps() -> f64 {
    1e-4
}

/// One clash test: selection A against selection B.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClashTest {
    /// Unique among a document's tests; part of every clash id.
    pub name: String,
    pub a: Selection,
    pub b: Selection,
    pub kind: ClashKind,
    /// Clearance only: the pair is reported when the distance is strictly below this (mm).
    /// An input, never a built-in value.
    #[serde(default)]
    pub tolerance_mm: f64,
    /// Hard only: whether faces that merely touch count as a clash. Default no.
    #[serde(default)]
    pub touching_counts: bool,
    /// Skip a pair when one element is on the other's host chain (post, foundation and sign
    /// touch by design). Default yes.
    #[serde(default = "yes")]
    pub ignore_host_chain: bool,
    /// Pairs already approved by the team: never reported. Order does not matter.
    #[serde(default)]
    pub approved_pairs: Vec<(ElementId, ElementId)>,
    /// Geometric tolerance for "on the surface" decisions (mm).
    #[serde(default = "default_eps")]
    pub epsilon_mm: f64,
}

impl ClashTest {
    pub fn hard(name: &str, a: Selection, b: Selection) -> ClashTest {
        ClashTest {
            name: name.to_string(),
            a,
            b,
            kind: ClashKind::Hard,
            tolerance_mm: 0.0,
            touching_counts: false,
            ignore_host_chain: true,
            approved_pairs: Vec::new(),
            epsilon_mm: default_eps(),
        }
    }
    pub fn clearance(name: &str, a: Selection, b: Selection, tolerance_mm: f64) -> ClashTest {
        ClashTest { kind: ClashKind::Clearance, tolerance_mm, ..ClashTest::hard(name, a, b) }
    }
    pub fn is_approved(&self, x: ElementId, y: ElementId) -> bool {
        self.approved_pairs.iter().any(|&(p, q)| (p == x && q == y) || (p == y && q == x))
    }
}
