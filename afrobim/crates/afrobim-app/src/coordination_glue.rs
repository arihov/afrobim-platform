//! Clash detection in the app: the scene the tests run on, and where tests and results are kept.
//!
//! The scene holds one `Body` per element that has solids, moved to world millimetres with exactly
//! the transform the 3D view draws by (`view3d::frame_in`; the crate's `Placement` is the same
//! rule), corridor and ground surface meshes included as the view draws them. Tests and results
//! are one document element of category `coordination`, holding the crate's JSON, so they are
//! saved, opened and undone with the document.

use std::collections::BTreeMap;

use afrobim_coordination::{Body, Coordination};
use afrobim_draw::{Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, ModuleRegistry, Pt};

use crate::view3d::{frame_in, WorldCtx};

pub const CATEGORY: &str = "coordination";
pub const LAYER: &str = "Coordination";
const NAME: &str = "Clash tests";

/// Elements that hold documents, not model geometry: they are never offered to a clash test.
pub fn is_record(category: &str) -> bool {
    [CATEGORY, afrobim_schedule::SCHEDULE_CATEGORY, afrobim_cost::ESTIMATE_CATEGORY, afrobim_progress::PROGRESS_CATEGORY].contains(&category)
}

/// The module of the coordination element: it draws a small marker on the sheet and reports the
/// counts as derived values.
pub struct CoordinationModule;

impl Module for CoordinationModule {
    fn category(&self) -> &str {
        CATEGORY
    }

    fn display_name(&self) -> &str {
        "Clash tests"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let co = Coordination::from_json(&element.data.to_string()).map_err(|e| format!("not valid clash data: {e}"))?;
        let mut derived = BTreeMap::new();
        derived.insert("Tests".to_string(), co.tests.len().to_string());
        derived.insert("Clashes".to_string(), co.results.clashes.len().to_string());
        for s in [afrobim_coordination::Status::New, afrobim_coordination::Status::Active, afrobim_coordination::Status::Reviewed, afrobim_coordination::Status::Approved, afrobim_coordination::Status::Resolved] {
            derived.insert(format!("Clashes {}", s.label()), co.results.count(s).to_string());
        }
        // A plain box on the sheet: this element is a record, not geometry.
        let (w, h) = (2000.0, 500.0);
        let corners = [Pt2::new(0.0, 0.0), Pt2::new(w, 0.0), Pt2::new(w, h), Pt2::new(0.0, h)];
        let primitives = (0..4).map(|i| Primitive::Line { from: corners[i], to: corners[(i + 1) % 4], width: 10.0, colour: None }).collect();
        Ok(Generated { drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(w, h) } }, derived, ..Generated::default() })
    }
}

pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(CoordinationModule));
}

/// The coordination element of the document and its data; an empty `Coordination` when there is none.
pub fn load(doc: &Document) -> (Option<ElementId>, Coordination) {
    match doc.elements().iter().find(|e| e.category == CATEGORY) {
        Some(e) => (Some(e.id), serde_json::from_value(e.data.clone()).unwrap_or_default()),
        None => (None, Coordination::default()),
    }
}

/// Stores the tests and results in the document, adding the element the first time.
pub fn store(doc: &mut Document, co: &Coordination) -> Result<ElementId, String> {
    let value = serde_json::to_value(co).map_err(|e| e.to_string())?;
    if let Some(id) = doc.elements().iter().find(|e| e.category == CATEGORY).map(|e| e.id) {
        if let Some(e) = doc.find_mut(id) {
            e.data = value;
        }
        return Ok(id);
    }
    doc.ensure_layer(LAYER);
    let at = doc.next_free_origin(2000.0, 500.0);
    let name = doc.unique_name(NAME);
    Ok(doc.add(CATEGORY, &name, Pt::new(at.x, at.y), 2000.0, 500.0, LAYER, value))
}

/// One body per element that has solids, in world millimetres as the 3D view draws them.
pub fn bodies(ctx: &WorldCtx, site_mode: bool) -> Vec<Body> {
    ctx.doc
        .elements()
        .iter()
        .filter(|e| !is_record(&e.category))
        .filter_map(|e| {
            let meshes = (ctx.solids_of)(e.id).filter(|m| !m.is_empty())?;
            let frame = frame_in(ctx, e, site_mode)?;
            Some(Body::from_element(e, meshes, &frame.placement()))
        })
        .collect()
}

/// The categories and layers of the elements a test can select from, sorted.
pub fn selectable(doc: &Document) -> (Vec<String>, Vec<String>) {
    let mut cats: Vec<String> = doc.elements().iter().filter(|e| !is_record(&e.category) && e.category != crate::alignment_glue::CATEGORY).map(|e| e.category.clone()).collect();
    cats.sort();
    cats.dedup();
    let mut layers: Vec<String> = doc.elements().iter().filter(|e| !is_record(&e.category) && e.category != crate::alignment_glue::CATEGORY).map(|e| e.layer.clone()).collect();
    layers.sort();
    layers.dedup();
    (cats, layers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_coordination::{ClashKind, ClashTest, Scene, Selection};
    use afrobim_draw::Mesh3;
    use afrobim_kernel::SitePlacement;
    use std::collections::HashMap;

    fn site(x: f64, y: f64, z: f64, rot: f64) -> SitePlacement {
        SitePlacement { x, y, z, rotation_deg: rot, alignment: None, station_m: None, offset_m: None }
    }

    fn cube(min: [f64; 3], max: [f64; 3]) -> Mesh3 {
        Mesh3::cuboid("Test", min, max, [128, 128, 128])
    }

    /// Two drains with a 1 m cube each, in their own frames at the site placements given.
    fn two_drains(second_x: f64, second_rot: f64) -> (Document, HashMap<ElementId, Vec<Mesh3>>) {
        let mut doc = Document::default();
        let mut solids = HashMap::new();
        for (i, (x, rot)) in [(0.0, 0.0), (second_x, second_rot)].into_iter().enumerate() {
            let id = doc.add(afrobim_drainage::DRAIN_CATEGORY, &format!("D{i}"), Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
            doc.find_mut(id).unwrap().site = Some(site(x, 0.0, 0.0, rot));
            solids.insert(id, vec![cube([0.0, 0.0, 0.0], [1000.0, 1000.0, 1000.0])]);
        }
        (doc, solids)
    }

    fn bodies_of(doc: &Document, solids: &HashMap<ElementId, Vec<Mesh3>>) -> Vec<Body> {
        let get = |id: ElementId| solids.get(&id).map(|v| v.as_slice());
        let none = |_: ElementId| None;
        let ctx = WorldCtx { doc, solids_of: &get, corridor_origin: &none };
        bodies(&ctx, crate::view3d::site_mode_of(doc))
    }

    #[test]
    fn a_known_overlap_is_found_and_a_gap_is_not() {
        // The second cube starts 600 mm along X, so the cubes overlap by 400 mm.
        let (doc, solids) = two_drains(600.0, 0.0);
        let scene = Scene::new(bodies_of(&doc, &solids));
        let (clashes, _) = scene.run(&ClashTest::hard("t", Selection::all(), Selection::all()));
        assert_eq!(clashes.len(), 1);
        let depth = clashes[0].depth_mm.unwrap();
        assert!((depth - 400.0).abs() < 1e-6, "depth {depth}");
        // 1500 mm along X leaves a 500 mm gap: no hard clash, one clearance clash at 600 mm.
        let (doc, solids) = two_drains(1500.0, 0.0);
        let scene = Scene::new(bodies_of(&doc, &solids));
        assert!(scene.run(&ClashTest::hard("t", Selection::all(), Selection::all())).0.is_empty());
        let (c, _) = scene.run(&ClashTest::clearance("c", Selection::all(), Selection::all(), 600.0));
        assert_eq!(c.len(), 1);
        assert!((c[0].distance_mm.unwrap() - 500.0).abs() < 1e-6);
        assert!(scene.run(&ClashTest::clearance("c", Selection::all(), Selection::all(), 400.0)).0.is_empty());
    }

    #[test]
    fn bodies_use_the_transform_of_the_3d_view() {
        // Rotated 90 degrees about Z at x = 1500: the cube's local +X runs along world +Y, so its
        // footprint is x in [500, 1500] (local y runs to world -X), y in [0, 1000].
        let (doc, solids) = two_drains(1500.0, 90.0);
        let bodies = bodies_of(&doc, &solids);
        let second = bodies.iter().find(|b| b.mark == "D1").unwrap();
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in second.meshes.iter().flat_map(|m| m.vertices.iter()) {
            for i in 0..3 {
                lo[i] = lo[i].min(v[i]);
                hi[i] = hi[i].max(v[i]);
            }
        }
        for (got, want) in [(lo[0], 500.0), (hi[0], 1500.0), (lo[1], 0.0), (hi[1], 1000.0)] {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
        // The same point through the view's own frame.
        let e = doc.elements().iter().find(|e| e.name == "D1").unwrap();
        let get = |id: ElementId| solids.get(&id).map(|v| v.as_slice());
        let none = |_: ElementId| None;
        let ctx = WorldCtx { doc: &doc, solids_of: &get, corridor_origin: &none };
        let p = frame_in(&ctx, e, true).unwrap().placement().apply([1000.0, 0.0, 0.0]);
        assert!((p[0] - 1500.0).abs() < 1e-6 && (p[1] - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn a_sign_on_its_own_post_is_not_a_clash() {
        // A sign element, a post hosted by it and the post's foundation, stood on the site. The
        // sign has a plate solid here (a real sign face is flat and has none) that overlaps the
        // post's top, and the post stands inside its foundation: both are by design.
        let mut doc = Document::default();
        let sign = doc.add("sign", "S1", Pt::new(0.0, 0.0), 1000.0, 600.0, "Signs", serde_json::Value::Null);
        let post = doc.add(afrobim_supports::POST_CATEGORY, "S1-P1", Pt::new(0.0, 0.0), 100.0, 100.0, "Supports", serde_json::Value::Null);
        let foundation = doc.add(afrobim_supports::FOUNDATION_CATEGORY, "S1-F1", Pt::new(0.0, 0.0), 500.0, 500.0, "Supports", serde_json::Value::Null);
        doc.find_mut(post).unwrap().host = Some(sign);
        doc.find_mut(foundation).unwrap().host = Some(post);
        for id in [sign, post, foundation] {
            doc.find_mut(id).unwrap().site = Some(site(0.0, 0.0, 0.0, 0.0));
        }
        let mut solids: HashMap<ElementId, Vec<Mesh3>> = HashMap::new();
        solids.insert(sign, vec![cube([-400.0, 0.0, 1800.0], [600.0, 20.0, 2400.0])]);
        solids.insert(post, vec![cube([0.0, 0.0, 0.0], [100.0, 100.0, 2000.0])]);
        solids.insert(foundation, vec![cube([-200.0, -200.0, -600.0], [300.0, 300.0, 100.0])]);
        let scene = Scene::new(bodies_of(&doc, &solids));
        let (found, _) = scene.run(&ClashTest::hard("host chain ignored", Selection::all(), Selection::all()));
        assert!(found.is_empty(), "{found:?}");
        // Switched off, the same pairs do clash, so the rule is what removes them.
        let mut open = ClashTest::hard("host chain counted", Selection::all(), Selection::all());
        open.ignore_host_chain = false;
        assert_eq!(scene.run(&open).0.len(), 2);
        // A second sign's post through the first sign's foundation is a real clash.
        let other = doc.add(afrobim_supports::POST_CATEGORY, "S2-P1", Pt::new(0.0, 0.0), 100.0, 100.0, "Supports", serde_json::Value::Null);
        doc.find_mut(other).unwrap().site = Some(site(100.0, 100.0, 0.0, 0.0));
        solids.insert(other, vec![cube([0.0, 0.0, 0.0], [100.0, 100.0, 2000.0])]);
        let scene = Scene::new(bodies_of(&doc, &solids));
        let (found, _) = scene.run(&ClashTest::hard("t", Selection::all(), Selection::all()));
        assert!(!found.is_empty());
        assert!(found.iter().all(|c| c.mark_a == "S2-P1" || c.mark_b == "S2-P1"), "{found:?}");
    }

    #[test]
    fn tests_and_results_survive_save_and_open() {
        let (mut doc, solids) = two_drains(600.0, 0.0);
        let (id, empty) = load(&doc);
        assert!(id.is_none() && empty.tests.is_empty());
        let mut co = Coordination::default();
        co.tests.push(ClashTest::hard("Drains", Selection::category(afrobim_drainage::DRAIN_CATEGORY), Selection::all()));
        let mut clearance = ClashTest::clearance("Clearance", Selection::all(), Selection::all(), 250.0);
        clearance.ignore_host_chain = false;
        co.tests.push(clearance);
        let scene = Scene::new(bodies_of(&doc, &solids));
        co.run(&scene);
        assert!(!co.results.clashes.is_empty());
        let first = co.results.clashes[0].id.clone();
        assert!(co.results.review(&first, afrobim_coordination::Status::Reviewed, "checked on site", "A. Surveyor"));
        store(&mut doc, &co).unwrap();
        store(&mut doc, &co).unwrap(); // a second store updates, it does not add another element
        assert_eq!(doc.elements().iter().filter(|e| e.category == CATEGORY).count(), 1);
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        let (_, back) = load(&reopened);
        assert_eq!(back, co);
        assert_eq!(back.results.get(&first).unwrap().comment, "checked on site");
        assert_eq!(back.tests[1].kind, ClashKind::Clearance);
        // Records are not geometry: they are never bodies.
        assert!(bodies_of(&reopened, &solids).iter().all(|b| b.category != CATEGORY));
    }
}
