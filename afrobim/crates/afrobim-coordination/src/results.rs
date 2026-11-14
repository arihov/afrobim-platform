//! Clash records, their review status, merging across runs, and reports.

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::geom::Aabb;
use crate::rules::ClashKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    New,
    Active,
    Reviewed,
    Approved,
    Resolved,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::New => "new",
            Status::Active => "active",
            Status::Reviewed => "reviewed",
            Status::Approved => "approved",
            Status::Resolved => "resolved",
        }
    }
}

/// One clash between two elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clash {
    /// Stable: a hash of the test name and the two element ids, so a re-run keeps identity.
    pub id: String,
    pub test: String,
    /// The lower element id of the pair.
    pub element_a: ElementId,
    pub element_b: ElementId,
    pub mark_a: String,
    pub mark_b: String,
    pub category_a: String,
    pub category_b: String,
    pub kind: ClashKind,
    /// Hard: estimated penetration depth in mm (the smallest side of the overlap box; for one
    /// body wholly inside another, the shortest axis move that frees it). Zero for touching.
    pub depth_mm: Option<f64>,
    /// Clearance: the measured minimum distance in mm (zero if the solids overlap).
    pub distance_mm: Option<f64>,
    /// One closed body wholly inside the other.
    #[serde(default)]
    pub containment: bool,
    /// Representative point, world mm: the centre of the overlap box.
    pub point: [f64; 3],
    /// Hard: bounds of the overlap. Clearance: the box spanned by the two closest points.
    pub overlap_box: Aabb,
    pub status: Status,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub assignee: String,
}

/// Identity of a clash: FNV-1a (64 bit, fixed by definition so it never changes between Rust
/// versions) over the test name, a zero byte and the two ids in ascending order.
pub fn clash_id(test: &str, a: ElementId, b: ElementId) -> String {
    let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for &x in bytes {
            h ^= x as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    feed(test.as_bytes());
    feed(&[0]);
    feed(&lo.to_le_bytes());
    feed(&hi.to_le_bytes());
    format!("CL-{h:016x}")
}

/// The clashes of a document across runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClashResults {
    pub clashes: Vec<Clash>,
}

impl ClashResults {
    /// Folds a new run into the stored results. `tests_run` names the tests that were run:
    /// clashes of other tests are left alone.
    ///
    /// - A clash seen again keeps its status, comment and assignee; `new` and `resolved`
    ///   become `active` (a resolved one has come back). Geometry is refreshed.
    /// - A clash never seen before is `new`.
    /// - A stored clash of a run test that is gone becomes `resolved`, keeping its notes. An
    ///   `approved` one stays approved: approved pairs are skipped by the next run, not gone.
    pub fn merge_run(&mut self, tests_run: &[String], new: Vec<Clash>) {
        let mut incoming: std::collections::HashMap<String, Clash> = new.into_iter().map(|c| (c.id.clone(), c)).collect();
        let mut order: Vec<String> = Vec::new();
        let mut merged: Vec<Clash> = Vec::with_capacity(self.clashes.len());
        for old in self.clashes.drain(..) {
            match incoming.remove(&old.id) {
                Some(mut fresh) => {
                    fresh.status = match old.status {
                        Status::New | Status::Resolved => Status::Active,
                        s => s,
                    };
                    fresh.comment = old.comment;
                    fresh.assignee = old.assignee;
                    merged.push(fresh);
                }
                None => {
                    let mut old = old;
                    if tests_run.contains(&old.test) && old.status != Status::Approved {
                        old.status = Status::Resolved;
                    }
                    merged.push(old);
                }
            }
        }
        // Newly found clashes, in a stable order.
        order.extend(incoming.keys().cloned());
        order.sort();
        for id in order {
            if let Some(c) = incoming.remove(&id) {
                merged.push(c);
            }
        }
        self.clashes = merged;
    }

    /// The approved pairs of a test, ready to feed back into [`ClashTest::approved_pairs`].
    pub fn approved_pairs(&self, test: &str) -> Vec<(ElementId, ElementId)> {
        self.clashes.iter().filter(|c| c.test == test && c.status == Status::Approved).map(|c| (c.element_a, c.element_b)).collect()
    }

    pub fn get(&self, id: &str) -> Option<&Clash> {
        self.clashes.iter().find(|c| c.id == id)
    }

    /// Sets the review fields of a clash. Returns false if the id is unknown.
    pub fn review(&mut self, id: &str, status: Status, comment: &str, assignee: &str) -> bool {
        match self.clashes.iter_mut().find(|c| c.id == id) {
            Some(c) => {
                c.status = status;
                c.comment = comment.to_string();
                c.assignee = assignee.to_string();
                true
            }
            None => false,
        }
    }

    pub fn count(&self, status: Status) -> usize {
        self.clashes.iter().filter(|c| c.status == status).count()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<ClashResults, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// One row per clash; quoted fields escape embedded quotes.
    pub fn to_csv(&self) -> String {
        let mut out = String::from(
            "id,test,type,status,element_a,mark_a,category_a,element_b,mark_b,category_b,depth_mm,distance_mm,containment,x,y,z,box_min_x,box_min_y,box_min_z,box_max_x,box_max_y,box_max_z,assignee,comment\n",
        );
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let num = |v: Option<f64>| v.map(|x| format!("{x:.3}")).unwrap_or_default();
        for c in &self.clashes {
            let f = |x: f64| format!("{x:.3}");
            let row = [
                c.id.clone(),
                q(&c.test),
                c.kind.label().to_string(),
                c.status.label().to_string(),
                c.element_a.0.to_string(),
                q(&c.mark_a),
                q(&c.category_a),
                c.element_b.0.to_string(),
                q(&c.mark_b),
                q(&c.category_b),
                num(c.depth_mm),
                num(c.distance_mm),
                c.containment.to_string(),
                f(c.point[0]),
                f(c.point[1]),
                f(c.point[2]),
                f(c.overlap_box.min[0]),
                f(c.overlap_box.min[1]),
                f(c.overlap_box.min[2]),
                f(c.overlap_box.max[0]),
                f(c.overlap_box.max[1]),
                f(c.overlap_box.max[2]),
                q(&c.assignee),
                q(&c.comment),
            ];
            out.push_str(&row.join(","));
            out.push('\n');
        }
        out
    }
}
