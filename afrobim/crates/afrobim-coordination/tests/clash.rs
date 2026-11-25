// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Hand-worked geometry. All lengths in millimetres.

use afrobim_coordination::geom::*;
use afrobim_coordination::*;
use afrobim_draw::Mesh3;
use afrobim_kernel::ElementId;

const GREY: [u8; 3] = [128, 128, 128];

fn cube(min: V3, max: V3) -> Mesh3 {
    Mesh3::cuboid("Box", min, max, GREY)
}

fn body(id: u64, cat: &str, meshes: Vec<Mesh3>) -> Body {
    Body::new(ElementId(id), &format!("M{id}"), cat, "0", None, meshes)
}

fn boxed(id: u64, min: V3, max: V3) -> Body {
    body(id, "any", vec![cube(min, max)])
}

fn hard() -> ClashTest {
    ClashTest::hard("T", Selection::all(), Selection::all())
}

fn run(bodies: Vec<Body>, test: &ClashTest) -> Vec<Clash> {
    Scene::new(bodies).run(test).0
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn cuboids_overlapping_flush_by_ten() {
    // Same y and z extents, overlap 10 mm in x: all faces coplanar or crossing at edges.
    let c = run(vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [90.0, 0.0, 0.0], [190.0, 100.0, 100.0])], &hard());
    assert_eq!(c.len(), 1);
    let c = &c[0];
    assert_eq!(c.kind, ClashKind::Hard);
    assert!(near(c.depth_mm.unwrap(), 10.0, 1e-9), "{:?}", c.depth_mm);
    assert_eq!(c.overlap_box.min, [90.0, 0.0, 0.0]);
    assert_eq!(c.overlap_box.max, [100.0, 100.0, 100.0]);
    assert_eq!(c.point, [95.0, 50.0, 50.0]);
    assert!(!c.containment);
    assert_eq!((c.mark_a.as_str(), c.mark_b.as_str()), ("M1", "M2"));
}

#[test]
fn cuboids_overlapping_skew_by_known_box() {
    // Overlap box is x 90..100, y 20..100, z 30..100: depth is its smallest side, 10.
    let c = run(vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [90.0, 20.0, 30.0], [190.0, 120.0, 130.0])], &hard());
    assert_eq!(c.len(), 1);
    assert!(near(c[0].depth_mm.unwrap(), 10.0, 1e-9));
    assert_eq!(c[0].overlap_box.min, [90.0, 20.0, 30.0]);
    assert_eq!(c[0].overlap_box.max, [100.0, 100.0, 100.0]);
}

#[test]
fn touching_faces_do_not_count_by_default() {
    let a = boxed(1, [0.0; 3], [100.0; 3]);
    // Full face contact, partial face contact, edge contact and corner contact.
    let others = [
        ([100.0, 0.0, 0.0], [200.0, 100.0, 100.0]),
        ([100.0, 50.0, 50.0], [200.0, 150.0, 150.0]),
        ([100.0, 100.0, 0.0], [200.0, 200.0, 100.0]),
        ([100.0, 100.0, 100.0], [200.0, 200.0, 200.0]),
    ];
    for (i, (lo, hi)) in others.iter().enumerate() {
        let c = run(vec![a.clone(), boxed(2, *lo, *hi)], &hard());
        assert!(c.is_empty(), "touching case {i} reported {c:?}");
    }
}

#[test]
fn touching_counts_when_asked() {
    let mut t = hard();
    t.touching_counts = true;
    let c = run(vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [100.0, 0.0, 0.0], [200.0, 100.0, 100.0])], &t);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].depth_mm, Some(0.0));
    // A real gap is still no clash.
    let c = run(vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [101.0, 0.0, 0.0], [200.0, 100.0, 100.0])], &t);
    assert!(c.is_empty());
}

#[test]
fn gap_against_clearance_tolerance_either_side() {
    let bodies = || vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [150.0, 0.0, 0.0], [250.0, 100.0, 100.0])];
    assert!(run(bodies(), &hard()).is_empty(), "a gap is not a hard clash");
    let below = ClashTest::clearance("C", Selection::all(), Selection::all(), 40.0);
    assert!(run(bodies(), &below).is_empty());
    let exactly = ClashTest::clearance("C", Selection::all(), Selection::all(), 50.0);
    assert!(run(bodies(), &exactly).is_empty(), "distance must be strictly below the tolerance");
    let above = ClashTest::clearance("C", Selection::all(), Selection::all(), 60.0);
    let c = run(bodies(), &above);
    assert_eq!(c.len(), 1);
    assert!(near(c[0].distance_mm.unwrap(), 50.0, 1e-9));
    assert_eq!(c[0].depth_mm, None);
    assert!(near(c[0].point[0], 125.0, 1e-9));
}

#[test]
fn clearance_diagonal_corner_distance() {
    // Corner to corner: 50 mm on each axis, distance 50 * sqrt(3).
    let bodies = vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [150.0; 3], [250.0; 3])];
    let t = ClashTest::clearance("C", Selection::all(), Selection::all(), 90.0);
    let c = run(bodies, &t);
    assert_eq!(c.len(), 1);
    assert!(near(c[0].distance_mm.unwrap(), 50.0 * 3f64.sqrt(), 1e-9));
}

#[test]
fn clearance_edge_to_edge_distance() {
    // Edges of the two boxes are skew: closest points on edges, 50 mm apart in x and y.
    let bodies = vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [150.0, 150.0, 20.0], [250.0, 250.0, 60.0])];
    let t = ClashTest::clearance("C", Selection::all(), Selection::all(), 100.0);
    let c = run(bodies, &t);
    assert_eq!(c.len(), 1);
    assert!(near(c[0].distance_mm.unwrap(), 50.0 * 2f64.sqrt(), 1e-9));
}

#[test]
fn clearance_reports_overlap_as_zero_distance() {
    let t = ClashTest::clearance("C", Selection::all(), Selection::all(), 10.0);
    let c = run(vec![boxed(1, [0.0; 3], [100.0; 3]), boxed(2, [50.0; 3], [150.0; 3])], &t);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].distance_mm, Some(0.0));
}

#[test]
fn cylinder_through_cuboid() {
    // Slab 0..200 x 0..200 x 0..100, a 100 mm diameter 64-sided post through its middle,
    // sticking out 100 mm below and 200 mm above.
    let slab = boxed(1, [0.0; 3], [200.0, 200.0, 100.0]);
    let post = body(2, "any", vec![Mesh3::cylinder("Post", 100.0, 100.0, -100.0, 300.0, 50.0, 64, GREY)]);
    let c = run(vec![slab.clone(), post], &hard());
    assert_eq!(c.len(), 1);
    let c = &c[0];
    assert!(near(c.overlap_box.min[2], 0.0, 1e-6) && near(c.overlap_box.max[2], 100.0, 1e-6));
    assert!(near(c.overlap_box.extent()[0], 100.0, 0.5) && near(c.overlap_box.extent()[1], 100.0, 0.5));
    assert!(near(c.depth_mm.unwrap(), 100.0, 0.5));
    assert!(near(c.point[0], 100.0, 0.5) && near(c.point[2], 50.0, 1e-6));
    // The same post beside the slab, clear of it.
    let beside = body(3, "any", vec![Mesh3::cylinder("Post", 400.0, 100.0, -100.0, 300.0, 50.0, 64, GREY)]);
    assert!(run(vec![slab, beside], &hard()).is_empty());
}

#[test]
fn faceted_cylinder_distance_error_is_bounded_by_sagitta() {
    // Radius 50, 32 sides, turned half a facet so a facet midpoint faces the box. The box
    // face is at x = 100: the true round-post gap is 50, the faceted gap is 100 - r cos(pi/n).
    let n = 32u32;
    let r = 50.0;
    let post = Mesh3::cylinder("Post", 0.0, 0.0, 0.0, 300.0, r, n, GREY);
    let placed = Body::from_element(
        &afrobim_kernel::Element {
            id: ElementId(2),
            category: "any".into(),
            name: "M2".into(),
            origin: afrobim_draw::Pt2::new(0.0, 0.0),
            width: 0.0,
            height: 0.0,
            layer: "0".into(),
            data: serde_json::Value::Null,
            properties: Default::default(),
            site: None,
            host: None,
        },
        &[post],
        &Placement::new([0.0; 3], 180.0 / n as f64),
    );
    let wall = boxed(1, [100.0, -500.0, 0.0], [200.0, 500.0, 300.0]);
    let t = ClashTest::clearance("C", Selection::all(), Selection::all(), 80.0);
    let c = run(vec![wall, placed], &t);
    assert_eq!(c.len(), 1);
    let d = c[0].distance_mm.unwrap();
    let sagitta = r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    assert!(near(d, 100.0 - r * (std::f64::consts::PI / n as f64).cos(), 1e-9), "{d}");
    assert!(d >= 50.0 && d <= 50.0 + sagitta + 1e-9, "faceted {d} vs true 50, sagitta {sagitta}");
}

#[test]
fn cube_wholly_inside_another() {
    let outer = boxed(1, [0.0; 3], [1000.0; 3]);
    let inner = boxed(2, [400.0; 3], [600.0; 3]);
    let c = run(vec![outer.clone(), inner.clone()], &hard());
    assert_eq!(c.len(), 1, "nested solid must not be missed");
    assert!(c[0].containment);
    assert_eq!(c[0].overlap_box.min, [400.0; 3]);
    assert_eq!(c[0].overlap_box.max, [600.0; 3]);
    // Shortest axis move that frees the inner cube: 600 (out through the min side).
    assert!(near(c[0].depth_mm.unwrap(), 600.0, 1e-9));
    // Clearance sees it as zero distance too.
    let t = ClashTest::clearance("C", Selection::all(), Selection::all(), 5.0);
    let c = run(vec![outer, inner], &t);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].distance_mm, Some(0.0));
}

#[test]
fn open_mesh_terrain_intersection_only() {
    // A flat open surface (two triangles) at z = 0.
    let ground = Mesh3 {
        name: "Terrain".into(),
        vertices: vec![[-1000.0, -1000.0, 0.0], [1000.0, -1000.0, 0.0], [1000.0, 1000.0, 0.0], [-1000.0, 1000.0, 0.0]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        colour: GREY,
    };
    let terrain = body(1, "terrain", vec![ground]);
    let scene = Scene::new(vec![terrain.clone()]);
    assert!(!scene.is_closed(0));
    let through = boxed(2, [0.0, 0.0, -50.0], [100.0, 100.0, 50.0]);
    let above = boxed(3, [0.0, 0.0, 10.0], [100.0, 100.0, 50.0]);
    assert_eq!(run(vec![terrain.clone(), through], &hard()).len(), 1);
    assert!(run(vec![terrain.clone(), above.clone()], &hard()).is_empty());
    // Distance to an open surface works: 10 mm.
    let t = ClashTest::clearance("C", Selection::all(), Selection::all(), 20.0);
    let c = run(vec![terrain, above], &t);
    assert!(near(c[0].distance_mm.unwrap(), 10.0, 1e-9));
}

#[test]
fn host_chain_pairs_are_ignored() {
    let mut sign = boxed(1, [0.0; 3], [100.0; 3]);
    sign.category = "sign".into();
    let mut post = boxed(2, [20.0; 3], [80.0; 3]);
    post.host = Some(ElementId(1));
    let mut foundation = boxed(3, [10.0; 3], [90.0; 3]);
    foundation.host = Some(ElementId(2));
    let stranger = boxed(4, [85.0; 3], [200.0; 3]);
    let scene = Scene::new(vec![sign, post, foundation, stranger]);
    let (c, _) = scene.run(&hard());
    // Only the stranger meets anything outside the chain: it overlaps the sign and foundation.
    let pairs: Vec<(u64, u64)> = c.iter().map(|x| (x.element_a.0, x.element_b.0)).collect();
    assert_eq!(pairs, vec![(1, 4), (3, 4)]);
    let mut all = hard();
    all.ignore_host_chain = false;
    let (c, _) = scene.run(&all);
    assert_eq!(c.len(), 5, "sign/post, sign/foundation, post/foundation, sign/stranger, foundation/stranger");
}

#[test]
fn same_element_and_approved_pairs_are_ignored() {
    // Two solids of one element overlap each other by design.
    let twin = body(1, "any", vec![cube([0.0; 3], [100.0; 3]), cube([50.0; 3], [150.0; 3])]);
    assert!(run(vec![twin], &hard()).is_empty());
    let a = boxed(1, [0.0; 3], [100.0; 3]);
    let b = boxed(2, [50.0; 3], [150.0; 3]);
    assert_eq!(run(vec![a.clone(), b.clone()], &hard()).len(), 1);
    let mut t = hard();
    t.approved_pairs.push((ElementId(2), ElementId(1)));
    assert!(run(vec![a, b], &t).is_empty());
}

#[test]
fn selections_by_category_layer_and_id() {
    let mut a = boxed(1, [0.0; 3], [100.0; 3]);
    a.category = "pipe".into();
    let mut b = boxed(2, [50.0; 3], [150.0; 3]);
    b.category = "road".into();
    b.layer = "L1".into();
    let mut c = boxed(3, [60.0; 3], [160.0; 3]);
    c.category = "road".into();
    let scene = Scene::new(vec![a, b, c]);
    let pairs = |t: &ClashTest| -> Vec<(u64, u64)> { scene.run(t).0.iter().map(|x| (x.element_a.0, x.element_b.0)).collect() };
    assert_eq!(pairs(&ClashTest::hard("T", Selection::category("pipe"), Selection::category("road"))), vec![(1, 2), (1, 3)]);
    assert_eq!(pairs(&ClashTest::hard("T", Selection::category("pipe"), Selection::layer("L1"))), vec![(1, 2)]);
    assert_eq!(pairs(&ClashTest::hard("T", Selection::ids(&[ElementId(1)]), Selection::ids(&[ElementId(3)]))), vec![(1, 3)]);
    // Road against road only finds the road pair.
    assert_eq!(pairs(&ClashTest::hard("T", Selection::category("road"), Selection::category("road"))), vec![(2, 3)]);
}

#[test]
fn placement_rotates_about_z_like_the_view() {
    // A 100 x 10 bar along +X at the origin, turned 90 degrees and moved to (1000, 0).
    let bar = Mesh3::cuboid("Bar", [0.0, 0.0, 0.0], [100.0, 10.0, 10.0], GREY);
    let p = Placement::new([1000.0, 0.0, 5.0], 90.0);
    let e = afrobim_kernel::Element {
        id: ElementId(1),
        category: "x".into(),
        name: "B".into(),
        origin: afrobim_draw::Pt2::new(0.0, 0.0),
        width: 0.0,
        height: 0.0,
        layer: "0".into(),
        data: serde_json::Value::Null,
        properties: Default::default(),
        site: None,
        host: Some(ElementId(9)),
    };
    let b = Body::from_element(&e, &[bar], &p);
    let bb = Aabb::of_points(b.meshes[0].vertices.iter());
    assert!(near(bb.min[0], 990.0, 1e-9) && near(bb.max[0], 1000.0, 1e-9));
    assert!(near(bb.min[1], 0.0, 1e-9) && near(bb.max[1], 100.0, 1e-9));
    assert!(near(bb.min[2], 5.0, 1e-9));
    assert_eq!(b.host, Some(ElementId(9)));
}

#[test]
fn results_merge_across_runs() {
    let a = boxed(1, [0.0; 3], [100.0; 3]);
    let b = boxed(2, [50.0; 3], [150.0; 3]);
    let apart = boxed(2, [500.0; 3], [600.0; 3]);
    let mut co = Coordination { tests: vec![ClashTest::hard("T", Selection::all(), Selection::all()), ClashTest::hard("Other", Selection::all(), Selection::all())], results: ClashResults::default(), ..Coordination::default() };

    co.run(&Scene::new(vec![a.clone(), b.clone()]));
    assert_eq!(co.results.clashes.len(), 2);
    let id = co.results.clashes.iter().find(|c| c.test == "T").unwrap().id.clone();
    assert!(co.results.clashes.iter().all(|c| c.status == Status::New));
    assert!(co.results.review(&id, Status::Reviewed, "check with the drainage lead", "A. Moyo"));

    // Same model again: identity and notes kept.
    co.run(&Scene::new(vec![a.clone(), b.clone()]));
    let c = co.results.get(&id).unwrap();
    assert_eq!((c.status, c.comment.as_str(), c.assignee.as_str()), (Status::Reviewed, "check with the drainage lead", "A. Moyo"));
    assert_eq!(co.results.clashes.len(), 2);

    // Element moved away: both resolve, notes kept.
    co.run(&Scene::new(vec![a.clone(), apart.clone()]));
    assert!(co.results.clashes.iter().all(|c| c.status == Status::Resolved));
    assert_eq!(co.results.get(&id).unwrap().comment, "check with the drainage lead");

    // Back again: re-opened as active, not new.
    co.run(&Scene::new(vec![a.clone(), b.clone()]));
    assert!(co.results.clashes.iter().all(|c| c.status == Status::Active));
    assert_eq!(co.results.get(&id).unwrap().assignee, "A. Moyo");

    // A run of one test leaves the other test's results alone.
    let mut only = co.clone();
    only.tests.truncate(1);
    only.run(&Scene::new(vec![a.clone(), apart]));
    assert_eq!(only.results.get(&id).unwrap().status, Status::Resolved);
    let other = only.results.clashes.iter().find(|c| c.test == "Other").unwrap();
    assert_eq!(other.status, Status::Active);

    // Approved stays approved and is skipped by the engine.
    co.results.review(&id, Status::Approved, "accepted", "");
    co.run(&Scene::new(vec![a, b]));
    assert_eq!(co.results.get(&id).unwrap().status, Status::Approved);
}

#[test]
fn ids_are_stable_and_symmetric() {
    let x = clash_id("T", ElementId(1), ElementId(2));
    assert_eq!(x, clash_id("T", ElementId(2), ElementId(1)));
    assert_ne!(x, clash_id("U", ElementId(1), ElementId(2)));
    assert_ne!(x, clash_id("T", ElementId(1), ElementId(3)));
    // Pinned value: FNV-1a is fixed, so the id may never change between releases.
    assert_eq!(x, "CL-268fcfc40c57ba5a");
    assert!(x.starts_with("CL-") && x.len() == 19);
}

#[test]
fn reports_and_persistence_round_trip() {
    let a = boxed(1, [0.0; 3], [100.0; 3]);
    let b = body(2, "pipe, \"main\"", vec![cube([50.0; 3], [150.0; 3])]);
    let mut co = Coordination { tests: vec![hard()], results: ClashResults::default(), ..Coordination::default() };
    co.run(&Scene::new(vec![a, b]));
    let id = co.results.clashes[0].id.clone();
    co.results.review(&id, Status::Active, "say \"hello\", then fix", "me");
    let csv = co.results.to_csv();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("id,test,type,status"));
    assert!(lines[1].contains("\"pipe, \"\"main\"\"\""), "{}", lines[1]);
    assert!(lines[1].contains("\"say \"\"hello\"\", then fix\""));
    let back = Coordination::from_json(&co.to_json().unwrap()).unwrap();
    assert_eq!(back, co);
    let r = ClashResults::from_json(&co.results.to_json().unwrap()).unwrap();
    assert_eq!(r, co.results);
}

#[test]
fn closed_meshes_are_recognised() {
    let s = Scene::new(vec![
        body(1, "any", vec![cube([0.0; 3], [1.0; 3])]),
        body(2, "any", vec![Mesh3::cylinder("C", 0.0, 0.0, 0.0, 10.0, 5.0, 12, GREY)]),
        body(3, "any", vec![Mesh3 { name: "Open".into(), vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 2]], colour: GREY }]),
    ]);
    assert!(s.is_closed(0) && s.is_closed(1) && !s.is_closed(2));
}

#[test]
fn triangle_distance_primitives() {
    let t1: Tri = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]];
    let t2: Tri = [[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]];
    assert!(near(tri_closest(&t1, &t2).0, 5.0, 1e-12));
    // Piercing triangles are at distance zero.
    let t3: Tri = [[2.0, 2.0, -3.0], [2.0, 2.0, 3.0], [4.0, 2.0, 3.0]];
    assert_eq!(tri_closest(&t1, &t3).0, 0.0);
    // Skew segments: (0,0,0)-(10,0,0) and (5,3,4)-(5,3,9) are 5 apart (3 in y, 4 in z).
    let (p, q) = closest_seg_seg([0.0; 3], [10.0, 0.0, 0.0], [5.0, 3.0, 4.0], [5.0, 3.0, 9.0]);
    assert!(near(dist(p, q), 5.0, 1e-12));
}

/// 2,000 boxes in a grid, many overlapping their neighbours; prints the time.
/// Run with `cargo test --release -p afrobim-coordination -- --ignored --nocapture`.
#[test]
#[ignore]
fn performance_2000_boxes() {
    let mut seed = 12345u64;
    let mut rnd = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as f64 / (1u64 << 31) as f64
    };
    let mut bodies = Vec::new();
    for i in 0..2000u64 {
        let (gx, gy) = ((i % 50) as f64, (i / 50) as f64);
        let (x, y) = (gx * 1000.0 + rnd() * 200.0, gy * 1000.0 + rnd() * 200.0);
        let (sx, sy, sz) = (700.0 + rnd() * 500.0, 700.0 + rnd() * 500.0, 500.0 + rnd() * 500.0);
        bodies.push(boxed(i + 1, [x, y, 0.0], [x + sx, y + sy, sz]));
    }
    let t0 = std::time::Instant::now();
    let scene = Scene::new(bodies);
    let t1 = std::time::Instant::now();
    let (hardc, hs) = scene.run(&hard());
    let t2 = std::time::Instant::now();
    let cl = ClashTest::clearance("C", Selection::all(), Selection::all(), 150.0);
    let (clearc, cs) = scene.run(&cl);
    let t3 = std::time::Instant::now();
    println!("prepare {:?}; hard {:?} -> {} clashes ({:?}); clearance {:?} -> {} clashes ({:?})", t1 - t0, t2 - t1, hardc.len(), hs, t3 - t2, clearc.len(), cs);
    assert!(hardc.len() > 100);
    assert!(clearc.len() >= hardc.len());
}
