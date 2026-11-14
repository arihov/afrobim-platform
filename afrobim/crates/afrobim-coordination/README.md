# afrobim-coordination

Clash detection between model elements: the Coordination stage of a BIM workflow (what Navisworks
Clash Detective or Solibri do), sized for a road model. Pure geometry. No engineering values are
built in: every tolerance is an input. The crate depends only on `afrobim-draw` (`Mesh3`) and
`afrobim-kernel` (`Element`, `SitePlacement`, `ElementId`), not on the app.

## Use

```rust
use afrobim_coordination::*;

// 1. One Body per element, solids moved to world mm with the transform the 3D view uses.
let placement = Placement::from_site(site, mounting_lift_mm);   // or Placement::new(origin, rot_deg).unrotated()
let body = Body::from_element(&element, &generated.solids, &placement);

// 2. Prepare once, run tests.
let scene = Scene::new(bodies);
let mut co = Coordination { tests: vec![
    ClashTest::hard("Pipes vs structures", Selection::category("drain"), Selection::category("structure")),
    ClashTest::clearance("Services clearance", Selection::all(), Selection::all(), clearance_mm_from_user),
], results: ClashResults::default() };
let stats = co.run(&scene);          // merges into co.results
std::fs::write("clashes.csv", co.results.to_csv())?;
let json = co.to_json()?;            // tests and results persist with the document
```

## Conventions

- Millimetres, Z up. `world = origin + Rz(rotation_deg) * local`, the rule of `view3d.rs` `Frame`.
  Categories the view leaves unrotated are passed with `Placement::unrotated()`.
- Meshes are counter-clockwise seen from outside (as `Mesh3` documents). The hard test relies on it.
- A mesh is *closed* when, after welding vertices within 1 micron, every edge belongs to exactly
  two triangles. Anything else (terrain, a surface) is *open*.
- Selections: categories, layers and ids narrow each other; an empty list means no restriction.
- **Touching does not count** as a hard clash by default (faces, edges or corners in contact,
  with or without partial overlap of the faces). Set `touching_counts` to report contact within
  `epsilon_mm` (default 0.0001 mm) as a hard clash of depth 0.
- Clearance reports a pair when the minimum distance is **strictly below** the tolerance; overlap
  and containment report distance 0. Touching is therefore always a clearance clash.
- Ignore rules: same element id; an element on the other's host chain (host, host of host, ...,
  `ignore_host_chain`, default on); pairs listed in `approved_pairs`.

## Algorithms

1. **Broad phase.** Per body an axis-aligned box. Sweep and prune along the axis with the widest
   spread: boxes sorted by minimum, each tested against those that start before its maximum,
   then on the other two axes. Hard tests inflate by `epsilon_mm`, clearance tests by the
   tolerance, so no pair closer than the tolerance is missed. Per mesh and per triangle the
   same box test culls before any triangle maths. 2,000 boxes: see Timing.
2. **Hard narrow phase (exact on triangles).** For each triangle pair with overlapping boxes:
   - an edge of one triangle that strictly crosses the plane of the other (both ends more than
     epsilon off it) and meets the triangle is a contact point. If the point is strictly inside
     the triangle it is a penetration. If it lies on a triangle edge of a *closed* target, a
     probe point a hair either side along the edge must lie strictly inside the target (ray
     parity, majority of three skew rays), otherwise the bodies only graze;
   - coplanar triangles that face the same way and overlap with positive area count (their
     solids lie on the same side); coplanar triangles facing opposite ways are contact, not
     overlap. This is what separates two flush boxes overlapping by 10 mm from two boxes
     touching face to face;
   - **containment**: if no surfaces meet and one closed mesh's box lies inside the other's, a
     vertex of the inner mesh (not on the outer surface) is ray-cast against the outer mesh;
     inside means one body is wholly inside the other (`containment = true`).
3. **Clearance.** Minimum distance between two triangles: zero if an edge pierces the other
   triangle, otherwise the least of 6 vertex-to-triangle and 9 segment-to-segment distances
   (Ericson's closest-point routines). Exact for triangle meshes. Pairs whose triangle boxes are
   already farther apart than the best distance so far are skipped.
4. **Numbers reported.** *Overlap box*: bounds of the contact points, clipped to the common
   box of the two bodies (for containment, the inner body's box; for clearance, the box
   spanned by the two closest points). *Point*: centre of that box. *Depth (hard)*: the
   smallest side of the overlap box, exact for aligned cuboids and an estimate otherwise; for
   containment, the shortest axis move that carries the inner box out of the outer box.
   *Distance (clearance)*: the measured minimum distance.
5. **Results.** `id = "CL-" + FNV-1a-64(test, 0, lower id, higher id)`, fixed by definition so it
   never changes between releases, and symmetric in the two elements. `merge_run` keeps status,
   comment and assignee of a clash seen again (`new` and `resolved` become `active`, so a
   returning clash is re-opened), marks clashes of the tests just run that are gone as
   `resolved`, and leaves other tests alone. `approved` stays approved: approved pairs are
   skipped by the next run, so they are not "gone". `Coordination::run` feeds them back as
   ignore rules automatically.
6. **Reports.** `ClashResults::to_csv()` (one row per clash, quoted text fields) and
   `to_json()`. Everything is serde: `ClashTest`, `ClashResults`, `Coordination`, `Body`.

## Limits

- **Faceted round solids.** A solid made of `n` flat sides lies inside the true round one, so
  measured distances are larger than the true ones by at most the sagitta `r (1 - cos(pi / n))`
  (zero at a facet vertex). For the 32-sided posts of the sign modules: 0.48 % of the radius,
  0.24 mm at r = 50 mm and 0.48 mm at r = 100 mm. 16 sides: 1.9 %. 64 sides: 0.12 %. Hard
  overlaps of a round solid are likewise missed when they are shallower than the sagitta. If
  millimetre-level clearance to a round element matters, inflate the tolerance by the sagitta
  or tessellate finer.
- **Open meshes** (terrain, surfaces) get intersection and distance tests only. They have no
  inside, so containment is never reported for them, and contact on the very edge of an open
  triangle counts as intersection.
- **Windings**: reversed (inward) winding breaks the touching versus overlapping decision for
  coplanar faces.
- **Identical coincident solids** are not a hard clash unless `touching_counts` is on.
- Hard depth is an estimate, not a minimum translation distance (that needs penetration
  depth algorithms such as EPA on convex parts).
- Precision: coordinates are f64 in world mm. Site coordinates in the tens of millions of mm
  keep sub-micron accuracy, well inside the default epsilon.
- Sweep and prune degrades for a few elements whose box spans the whole model (a long
  pavement layer): each is tested against every body its box meets, which is correct but is
  the bulk of the cost. A grid or BVH per mesh would be the next speed-up for million-triangle
  surfaces; pair loops inside a mesh pair are box-culled but not tree-accelerated.

## Timing

Release build, 2,000 jittered boxes in a 50 by 40 grid, about 2,200 touching or overlapping
pairs: prepare 11 ms, hard test 17 ms, clearance test (150 mm) 10 ms. Run the ignored test
`performance_2000_boxes` with `cargo test --release -p afrobim-coordination -- --ignored --nocapture`.

## Not done (next steps)

- BCF export (topics, comments, viewpoints with camera) and import.
- Saved viewpoints per clash.
- IFC federated models: reading other disciplines' IFC into `Body` values for cross-model tests.
- Grouping clashes by cause (many triangle contacts of one pipe through one wall are already
  one clash per element pair, but grouping by location or by element is not).
- Time-based (4D) clashes, rule sets from templates, penetration depth by EPA.
