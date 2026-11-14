# afrobim-coordination

Clash detection between model elements: the Coordination stage of a BIM workflow (what Navisworks
Clash Detective or Solibri do), sized for a road model. Pure geometry. No engineering values are
built in: every tolerance is an input. The crate depends on `afrobim-draw` (`Mesh3`),
`afrobim-kernel` (`Element`, `SitePlacement`, `ElementId`) and, for the GlobalIds in BCF files,
`afrobim-ifc` (`element_global_id`); it does not depend on the app. `afrobim-ifc` does not depend
on this crate, so the dependency graph stays acyclic. Third party: `flate2` (zip), `roxmltree`
(XML reading); both were already in the workspace lock file.

Besides clash detection the crate holds **issues** (with comments, viewpoints, related elements
and a location), **clash grouping** and **BCF 2.1 export and import**; see the sections after
"Not done".

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

- Saved viewpoints are data only: the crate renders nothing, so the app supplies camera and
  snapshot bytes.
- IFC federated models: reading other disciplines' IFC into `Body` values for cross-model tests
  (`afrobim_ifc::import` reads the file; the app builds the `Body` values, see its README).
- Time-based (4D) clashes, rule sets from templates, penetration depth by EPA.
- BCF 3.0 and the BCF REST API (see below).

## Issues

`Coordination::issues` holds `Issue` values and persists with the tests and results (`#[serde(default)]`,
so a document saved before issues existed loads with none). An issue has a stable GUID
(`8-4-4-4-12` hex, the BCF topic GUID), title, description, `issue_type`, `status`, `priority`
(free text: BCF 2.1 leaves these to the project, `values::*` lists common ones), assignee, author,
creation and modification times and the modifier, due date, labels, comments (GUID, author, time,
text, optional viewpoint), related elements, a location (world mm), viewpoints, and the clashes it
covers. Times are Unix seconds (UTC); the caller passes "now", so nothing reads the clock.

- `ElementRef`: `id` (an element of this document), `mark`, `ifc_guid`. A reference to an element
  of a reference model, or one that came from a foreign BCF file, has only the GlobalId.
- `Viewpoint`: GUID, optional `Camera` (position in mm, direction, up, `Projection::Perspective {
  fov_deg }` or `Orthogonal { view_to_world_scale_mm }`), `selected` elements, `Visibility`
  (`All`, `Only(list)`, `Except(list)`) and optional PNG bytes, stored as base64 in the document.
- `Camera::looking_at(target, from_direction, distance_mm, projection)` and
  `Issue::add_location_viewpoint(...)` build the usual "look at the clash" view; the distance is the
  caller's.
- GUIDs of issues from clashes derive from the clash id (UUID version 5 in the AfroBIM
  namespace, `afrobim_ifc::name_uuid`), so raising an issue twice from one clash gives one issue.
  Free-standing issues derive theirs from a counter, the time and the title, made unique.

```rust
use afrobim_coordination::*;

// From a stored clash, with a viewpoint 4 m away looking at it.
let guid = co.raise_issue_from_clash(&clash.id, "ann@example.org", now_unix).unwrap();
let issue = co.issue_mut(&guid).unwrap();
issue.add_location_viewpoint([-1.0, -1.0, 0.6], 4000.0, Projection::Perspective { fov_deg: 50.0 });
issue.add_comment("bob@example.org", now_unix, "Moving the drain 300 mm west.");
issue.set_status(values::STATUS_IN_PROGRESS, "bob@example.org", now_unix);
issue.viewpoints[0].snapshot_png = Some(png_bytes_from_the_3d_view);

// Free-standing.
let note = co.new_issue("Sign face shows the wrong route", "carol", now_unix);
note.elements.push(ElementRef::element(sign_id, "ADS-007"));
note.location = Some([12_345.0, -9_876.0, 1_500.0]);
```

## Grouping clashes

`group_clashes(&clashes, &GroupOptions)` puts clashes in one group when they share an element
(`share_element`) or when their points are at most `distance_mm` apart. Groups are the connected
components (a chain A-B, B-C, C-D is one group even if A and D are far apart). **No distance is
built in**: `distance_mm = None` switches the rule off. `ignore_shared` lists elements that must
not link clashes by being shared (a terrain surface or a pavement layer touches everything).
Each `ClashGroup` has a stable id (hash of its sorted clash ids), the member ids, the distinct
elements, the centroid and the bounds of the points. `Issue::from_group(&group, &clashes, author, now)`
makes one issue for the group (all elements related, the centroid as location, one `ClashLink` per
clash). The distance search sorts by x and compares only while the x gap is within the distance.

```rust
let groups = group_clashes(&co.results.clashes, &GroupOptions::by_element_or_distance(distance_mm_from_user));
for g in &groups {
    co.issues.push(Issue::from_group(g, &co.results.clashes, "ann", now_unix));
}
```

## BCF 2.1

**Source.** The buildingSMART/BCF-XML repository (https://github.com/buildingSMART/BCF-XML), tag
`v2.1` (commit `a0fbab777706b5ac282105f724979e1f8e271e51`): `Schemas/markup.xsd`,
`Schemas/visinfo.xsd`, `Schemas/version.xsd`, `Schemas/project.xsd`, `Documentation/README.md`
(file layout, units, zip rules) and `Extension Schemas/extensions.xsd` (a sample, used only for the
status words). Element and attribute names in the code are those of the schemas, not written from
memory. The repository's `release_3_0` branch (BCF 3.0) was fetched but not implemented: 3.0 has a
different markup layout, and this crate refuses a 3.0 file with an error that says so.

```rust
use afrobim_coordination::*;

let options = BcfOptions {
    project_name: "A104 signs".into(),            // the IfcOptions::project_name of the IFC file
    ifc_file_name: Some("a104-signs.ifc".into()), // written to the topic header
    project_id: Some("A104".into()),              // optional project.bcfp
    ..BcfOptions::default()
};
let out = export_bcf(&co.issues, &options)?;
std::fs::write("issues.bcf", &out.bytes)?;        // .bcf or .bcfzip, a zip file
for note in &out.notes { eprintln!("{note}"); }   // what was left out or changed

let bytes = std::fs::read("from-a-colleague.bcf")?;
let got = import_bcf(&bytes, &ImportOptions { project_name: "A104 signs".into(), known_ids: doc_element_ids })?;
co.merge_issues(got.issues);                      // same GUID replaces, new ones are added
println!("{} topics, {} comments, {} viewpoints", got.report.topics, got.report.comments, got.report.viewpoints);
for s in &got.report.skipped { eprintln!("not imported: {s}"); }
for w in &got.report.warnings { eprintln!("warning: {w}"); }
```

**What is written.** `bcf.version` (2.1), optional `project.bcfp`, and for each issue a folder named
by its GUID with `markup.bcf` (Header/File when an IFC file name is given; Topic with Guid,
TopicType, TopicStatus, Title, Priority, Index, Labels, CreationDate, CreationAuthor,
ModifiedDate, ModifiedAuthor, DueDate, AssignedTo, Description; Comment with Guid, Date, Author,
Comment and the viewpoint back reference; Viewpoints with Viewpoint, Snapshot, Index),
`viewpoint.bcfv` (and `viewpoint-1.bcfv`, ...) with Components (Selection, Visibility with
DefaultVisibility and Exceptions), OrthogonalCamera or PerspectiveCamera, and `snapshot.png` (and
`snapshot-1.png`, ...). Element order follows the schemas. The zip has directory entries and
forward slashes, as the 2.1 documentation asks, and is byte-reproducible for the same input.

- **Units.** BCF coordinates are metres and the scale is "visible size in meters" (the 2.1
  documentation and `visinfo.xsd`); the crate divides millimetres by 1000. Directions are written
  normalised. The perspective field of view must be 45 to 60 degrees in `visinfo.xsd`: other values
  are clamped and a note says so.
- **Elements by IFC GlobalId.** A component's `IfcGuid` is `afrobim_ifc::element_global_id(project_name, id)`,
  the function the IFC exporter uses, so a BCF file opened beside the exported IFC selects the same
  objects. A reference that is a reference-model product (only a GlobalId) is written as is.
  `AuthoringToolId` carries the element id and `OriginatingSystem` is `AfroBIM`. A reference with
  neither an id nor a valid 22 character GlobalId is left out, with a note. The test
  `bcf_components_select_the_objects_of_the_exported_ifc` exports an IFC and a BCF and checks the
  GlobalIds against the products the importer reads back from the IFC.
- **What BCF 2.1 has no place for** goes in an extra file per topic, `afrobim.json`: the
  location, the marks of related elements, the clash links, and the exact camera and element lists
  per viewpoint. This is an AfroBIM extension, not part of BCF-XML. No other BCF tool has been
  tried with it; a tool that ignores files it does not know will ignore it. Without it (a
  foreign file) the location is empty and the related elements are the ones the viewpoints select.
  On import the file is used only when it parses and names itself; its camera values replace the
  XML's only when they still agree with the XML (so an edit by another tool wins).
- **The field of view you set is kept** in `afrobim.json`, so a round trip returns 90 even though
  the XML says 60.

**Import tolerance.** Anything outside what the model can hold is counted in `report.skipped`:
`ReferenceLink`, `Stage`, `BimSnippet`, `DocumentReference`, `RelatedTopic`, `Lines`,
`ClippingPlanes`, `Bitmap`, `Coloring`, `ViewSetupHints`, comment modification fields, BCF 2.0
`Status`/`VerbalStatus`, unknown elements and attributes, and files that are not part of the format.
Damaged topics (not well-formed XML, a DTD, not UTF-8) are dropped with a warning and the others
are kept. Folder names that differ from the Topic Guid, bad dates and missing required elements are
warnings. BCF 2.0 and 1.x files are read with the 2.1 rules (1.x with a warning); the
`Viewpoints` element is optional (a 1.0 style `viewpoint.bcfv` is found by name). DTDs are refused
(`roxmltree` default), which also rules out entity expansion attacks. XML in UTF-16 is not read.
Topics are ordered by `Index` where present. The zip reader supports stored and deflated entries,
refuses ZIP64 and encryption, refuses entries over 512 MB unpacked, and checks every CRC-32.

**What was verified.**

- Export, then import gives equal `Issue` values (`==`) for issues with every field, three
  viewpoints (perspective with snapshot and `Except` visibility, orthogonal with `Only`
  visibility, selection only), comments with a viewpoint link, a foreign element reference and
  special characters; a second export of the imported issues is byte-identical.
- **Schema validation: passed.** All 10 XML files of an export of those issues plus an empty
  export (`bcf.version`, `project.bcfp`, three `markup.bcf`, three `viewpoint*.bcfv`, ...)
  validate against `markup.xsd`, `visinfo.xsd`, `version.xsd` and `project.xsd` of the v2.1 tag
  with `xmllint --schema` (libxml 2.9.14). A negative control (a markup that breaks the schema)
  must fail, so a pass means something. The test needs `xmllint` and the schemas
  (`BCF_XSD_DIR`, default `/home/user/buildingsmart/v21/Schemas`); it prints `SKIPPED` and passes
  without them. The schemas are not in this repository (the BCF-XML repository is licensed
  CC BY-ND 4.0).
- The archive is read by Python's `zipfile` (`testzip`) and `unzip -t`.
- **Foreign files.** The 19 `.bcf` files under `Test Cases/v2.1` of the BCF-XML repository
  (markup cases, viewpoint cases, extension schema, internal and external snippets, PDF
  reference, related topics) all import: 21 topics, with what could not be held reported as
  skipped and no warnings. `BCF_TESTCASES_DIR` points the test at them; it is skipped when absent.
  Hand-written foreign files cover unknown elements, a 2.0 comment, damaged topics, 1.0 and 3.0.
- **Not verified:** no file was opened in a commercial BIM or BCF tool, in any viewer, or posted
  to a BCF server. That another tool shows the viewpoints, the components or the camera as intended
  is not known.

**Limits.** BCF 2.1 only (3.0 is refused); no BCF REST API; clipping planes, 3D lines, bitmaps and
coloring are not modelled; at most 65534 topic files and entries under 4 GB (no ZIP64); the
GlobalIds the IFC exporter derives from other names than `element/<id>` (for example the
`IfcElementAssembly` that groups a sign with its supports, named `element/<id>/assembly`) cannot
be derived from an `ElementRef::element`; for such objects pass `ElementRef::foreign` with the
GlobalId taken from the IFC file.
