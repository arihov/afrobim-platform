//! The built scenes of the civil elements, kept until the element (or what it depends on) changes.
//! A build runs on a worker thread through `background.rs` and the result is applied if the
//! document did not change meanwhile.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use afrobim_kernel::{Document, Element, ElementId};

use crate::background::{Job, Poll};
use crate::civil::{self, *};
use crate::civil_build::{self, Built, Ctx};

/// What a finished job hands back: element, key it was built for, and the build.
pub type CivilOut = Vec<(ElementId, String, Built)>;

#[derive(Default)]
pub struct CivilState {
    /// Element id to (key, build).
    pub built: HashMap<ElementId, (String, Built)>,
    /// The key of the last discarded or failed spawn, so it is not repeated every frame.
    pub held: Option<u64>,
    /// How many builds have been applied (a test and the status line read it).
    pub applied: u64,
    /// Decoded TINs of imported surfaces: element id to (size of its data, TIN).
    pub imported: HashMap<ElementId, (usize, Arc<afrobim_corridor::Tin>)>,
}

impl CivilState {
    pub fn tin_of(&self, id: ElementId) -> Option<Arc<afrobim_corridor::Tin>> {
        self.built.get(&id).and_then(|(_, b)| b.tin.clone())
    }
}

/// Whether a category's scene reads other elements (alignments, surfaces, profiles).
fn depends(category: &str) -> bool {
    matches!(category, PROFILE | PROFILE_VIEW | CORRIDOR | SAMPLES | PARCEL | PIPEPROFILE | GRADING | INTERSECTION | ROUNDABOUT)
}

/// A hash of what the dependent scenes read: every alignment and every civil element's revision.
pub fn dependency_hash(doc: &Document) -> u64 {
    let mut h = DefaultHasher::new();
    for e in doc.elements() {
        if e.category == afrobim_alignment::CATEGORY {
            e.data.to_string().hash(&mut h);
        } else if e.category == afrobim_corridor::SURFACE_CATEGORY {
            // An imported surface has no revision: a sampled digest of its data tells that it was
            // replaced or edited (cheap enough for every frame, whatever the size of the TIN).
            e.id.0.hash(&mut h);
            e.name.hash(&mut h);
            digest(&e.data).hash(&mut h);
        } else if is_civil(&e.category) && e.category != POINTS && e.category != PARCEL {
            e.id.0.hash(&mut h);
            rev_of(e).hash(&mut h);
        }
    }
    h.finish()
}

/// A digest of a JSON value that looks at the shape and at most 64 evenly spaced entries of every
/// array, so that its cost does not grow with a large TIN.
fn digest(v: &serde_json::Value) -> u64 {
    use serde_json::Value;
    let mut h = DefaultHasher::new();
    match v {
        Value::Null => 0u8.hash(&mut h),
        Value::Bool(b) => b.hash(&mut h),
        Value::Number(n) => n.as_f64().map(f64::to_bits).hash(&mut h),
        Value::String(s) => s.hash(&mut h),
        Value::Array(a) => {
            a.len().hash(&mut h);
            let step = (a.len() / 64).max(1);
            for x in a.iter().step_by(step) {
                digest(x).hash(&mut h);
            }
            if let Some(last) = a.last() {
                digest(last).hash(&mut h);
            }
        }
        Value::Object(m) => {
            for (k, x) in m {
                k.hash(&mut h);
                digest(x).hash(&mut h);
            }
        }
    }
    h.finish()
}

pub fn key_of(e: &Element, dep: u64) -> String {
    if depends(&e.category) {
        format!("{}|{}|{dep}", e.name, rev_of(e))
    } else {
        format!("{}|{}", e.name, rev_of(e))
    }
}

/// The elements a build reads, cloned for the worker.
fn snapshot(doc: &Document) -> Vec<Element> {
    doc.elements().iter().filter(|e| is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY || e.category == afrobim_corridor::SURFACE_CATEGORY).cloned().collect()
}

impl crate::app::AfroBimApp {
    /// The TIN of any surface element: a civil surface (built) or an imported one (decoded once).
    pub(crate) fn tin_for(&mut self, id: ElementId) -> Option<Arc<afrobim_corridor::Tin>> {
        if let Some(t) = self.civil.tin_of(id) {
            return Some(t);
        }
        let e = self.doc.find(id)?;
        if e.category != afrobim_corridor::SURFACE_CATEGORY {
            return None;
        }
        let size = e.data.to_string().len();
        if let Some((n, t)) = self.civil.imported.get(&id) {
            if *n == size {
                return Some(t.clone());
            }
        }
        let t = Arc::new(serde_json::from_value::<afrobim_corridor::SurfaceDefinition>(e.data.clone()).ok()?.tin);
        self.civil.imported.insert(id, (size, t.clone()));
        Some(t)
    }

    /// Once a frame: starts a build for the stale civil elements when none is running.
    pub(crate) fn civil_sync(&mut self) {
        if self.bg.civil.is_some() {
            return;
        }
        let ids: Vec<ElementId> = self.doc.elements().iter().filter(|e| is_civil(&e.category)).map(|e| e.id).collect();
        self.civil.built.retain(|id, _| ids.contains(id));
        if ids.is_empty() {
            return;
        }
        let dep = dependency_hash(&self.doc);
        let stale: Vec<ElementId> = self
            .doc
            .elements()
            .iter()
            .filter(|e| is_civil(&e.category))
            .filter(|e| self.civil.built.get(&e.id).is_none_or(|(k, _)| *k != key_of(e, dep)))
            .map(|e| e.id)
            .collect();
        if stale.is_empty() {
            return;
        }
        let mut h = DefaultHasher::new();
        for id in &stale {
            id.0.hash(&mut h);
            if let Some(e) = self.doc.find(*id) {
                key_of(e, dep).hash(&mut h);
            }
        }
        let held = h.finish();
        if self.civil.held == Some(held) {
            return;
        }
        self.civil.held = Some(held);
        let elements = snapshot(&self.doc);
        let tins: HashMap<u64, Arc<afrobim_corridor::Tin>> = self.civil.built.iter().filter_map(|(id, (_, b))| b.tin.clone().map(|t| (id.0, t))).collect();
        let label = if stale.len() == 1 { "Building a civil object".to_string() } else { format!("Building {} civil objects", stale.len()) };
        let fp = crate::background::fingerprint(&self.doc);
        self.bg.civil = Some(Job::spawn(&label, fp, move || build_all(elements, tins, stale, dep)));
    }

    /// The derived figures and notes of a civil element for the Properties panel.
    pub(crate) fn civil_derived(&self, id: ElementId) -> Option<(std::collections::BTreeMap<String, String>, Vec<afrobim_kernel::Note>)> {
        let e = self.doc.find(id)?;
        if !is_civil(&e.category) {
            return None;
        }
        let Some((_, b)) = self.civil.built.get(&id) else {
            return Some((std::iter::once(("Build".to_string(), "waiting".to_string())).collect(), vec![]));
        };
        let derived = b.scene.derived.iter().cloned().collect();
        let note = |sev, code: &str, m: &String| afrobim_kernel::Note { severity: sev, code: code.to_string(), message: m.clone() };
        let mut notes: Vec<afrobim_kernel::Note> = b.scene.notes.iter().map(|m| note(afrobim_kernel::Severity::Warning, "CIVIL", m)).collect();
        notes.extend(b.report.iter().filter(|m| !b.scene.notes.contains(m)).map(|m| note(afrobim_kernel::Severity::Info, "BUILD", m)));
        Some((derived, notes))
    }

    /// Sizes an element to what it draws, once that is known (a profile view, a corridor, sections).
    fn fit_bounds(&mut self, id: ElementId, b: &Built) {
        let Some(cat) = self.doc.find(id).map(|e| e.category.clone()) else { return };
        let bounds = match cat.as_str() {
            PROFILE_VIEW | SAMPLES | CORRIDOR | PIPEPROFILE => civil_build::scene_bounds(&b.scene),
            SURFACE => b.tin.as_ref().map(|t| {
                let x = t.bounds();
                civil::Bounds { min_e: x.min_x, min_n: x.min_y, max_e: x.max_x, max_n: x.max_y }
            }),
            _ => None,
        };
        if let Some(bounds) = bounds {
            // The 3D mesh is in the frame of the element's origin: a moved origin means a rebuild.
            if civil::set_bounds(&mut self.doc, id, bounds) && (cat == SURFACE || cat == CORRIDOR) {
                civil::touch(&mut self.doc, id);
            }
        }
    }

    pub(crate) fn civil_tick(&mut self) {
        if let Some(polled) = crate::background::take(&mut self.bg.civil, &self.doc) {
            match polled {
                Poll::Done(out) => {
                    for (id, key, b) in out {
                        self.fit_bounds(id, &b);
                        self.civil.built.insert(id, (key, b));
                        self.civil.applied += 1;
                    }
                    self.civil.held = None;
                    self.fitted_3d = false;
                }
                // Discarded (an edit meanwhile, or Cancel): the next frame starts again unless it
                // was cancelled, in which case an edit is needed first (`held` keeps it quiet).
                Poll::Cancelled => self.status = "Civil build cancelled: the result was discarded.".into(),
                _ => self.civil.held = None,
            }
        }
    }

    /// Waits for every civil build to finish (the tests and the command line use this).
    #[allow(dead_code)]
    pub(crate) fn civil_build_now(&mut self) {
        self.civil.held = None;
        self.bg.civil = None;
        let dep = dependency_hash(&self.doc);
        let stale: Vec<ElementId> = self.doc.elements().iter().filter(|e| is_civil(&e.category)).filter(|e| self.civil.built.get(&e.id).is_none_or(|(k, _)| *k != key_of(e, dep))).map(|e| e.id).collect();
        if stale.is_empty() {
            return;
        }
        let tins: HashMap<u64, Arc<afrobim_corridor::Tin>> = self.civil.built.iter().filter_map(|(id, (_, b))| b.tin.clone().map(|t| (id.0, t))).collect();
        for (id, key, b) in build_all(snapshot(&self.doc), tins, stale, dep) {
            self.fit_bounds(id, &b);
            self.civil.built.insert(id, (key, b));
            self.civil.applied += 1;
        }
    }
}

/// Builds the stale elements, surfaces first so that what reads them finds the new TIN.
pub fn build_all(elements: Vec<Element>, tins: HashMap<u64, Arc<afrobim_corridor::Tin>>, stale: Vec<ElementId>, dep: u64) -> CivilOut {
    let mut ctx = Ctx { elements, tins };
    let mut order: Vec<ElementId> = stale.clone();
    order.sort_by_key(|id| match ctx.element(id.0).map(|e| e.category.as_str()) {
        Some(SURFACE) => 0,
        Some(PROFILE) | Some(ASSEMBLY) => 1,
        Some(CORRIDOR) => 2,
        _ => 3,
    });
    let mut out = Vec::new();
    for id in order {
        let Some(e) = ctx.element(id.0).cloned() else { continue };
        let b = civil_build::build(&e, &ctx);
        if let Some(t) = &b.tin {
            ctx.tins.insert(id.0, t.clone());
        }
        for (i, (_, t)) in b.extra.iter().enumerate() {
            let _ = i;
            let _ = t;
        }
        out.push((id, key_of(&e, dep), b));
    }
    let _ = civil::LAYER;
    out
}
