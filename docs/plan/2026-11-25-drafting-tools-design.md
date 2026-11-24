# AfroBIM: core 2D drafting set, design (2026-11-25)

Status: design for review, no code. Scope follows section 7.4 of `2026-11-25-afrobim-launch-and-product-roadmap.md` ("add a core drafting set ... first-class element family that exports to DWG and DXF, stays out of IFC quantities unless tagged, and can be converted into model objects").

## 1. What the application has today (read from the code)

| Area | What exists | Where |
|---|---|---|
| Elements | `Element { id, category, name, origin, width, height, layer, data: serde_json::Value, properties, site, host }`. `origin`, `width`, `height` are millimetres in the site plan and are the only geometry the kernel understands. `data` belongs to the module that owns the category. | `afrobim-kernel/src/document.rs` |
| Modules | `trait Module { category, display_name, generate(&Element) -> Generated { drawing, notes, derived, solids } }`, held in a `ModuleRegistry`. | `afrobim-kernel/src/module.rs` |
| Interaction | `CanvasController` is a state machine with a fixed tool list (Idle, box, drag, MOVE, COPY, ERASE, zoom, place). It holds no GUI types. | `afrobim-kernel/src/controller.rs` |
| Move, copy | `Document::translate` changes only `origin`. So anything stored relative to `origin` moves with no extra code. | `document.rs:316` |
| Selection | `Document::pick` and `in_box` test **bounding boxes**. A diagonal line's box is large, so a precise hit test is needed for drafting. | `document.rs:269-285` |
| Typed points | `parse_point`: `x,y`, `@dx,dy`, `@d<a`; also `ortho`, `snap`, grid. | `afrobim-kernel/src/drafting.rs` |
| Command table | `commands.rs`: one entry per command with `name`, `aliases`, `help`, `action`, shared by menus, ribbon, palette and command line. `SCALE` is today an alias of `APPEARANCE` and must move. | `afrobim-app/src/commands.rs:370` |
| Annotation | Text, MText, dimensions, leaders, measure commands, layers with colour and lineweight, text and dimension styles. Stored **local to the element origin**, with `read_absolute`, `replace_absolute`, `normalize`. Interaction is a `Session` (`start`, `point`, `enter`, `text`, `preview`, `prompt` returning an `Out`). | `afrobim-annotate`, `afrobim-app/src/anno_glue.rs` |
| Object snap | `SnapMode`: Endpoint, Midpoint, Centre, Quadrant, Intersection, Node, Grid; `Osnap` settings; `candidates_with` reads the lines and arcs of any element's `Drawing`; underlays also give snaps. Perpendicular, Tangent, Nearest, Insertion and Extension are missing. | `anno_glue.rs:816-1107`, `anno_ui.rs:252-300` |
| Export | `export_dxf` writes an **R12** DXF through `afrobim-dxf` (`POLYLINE/VERTEX`, `ARC`, `SOLID`, `TEXT`). DWG goes through `dxf_to_dwg` (acadrust). R12 has no LWPOLYLINE, ELLIPSE, SPLINE, HATCH or linetype table. | `afrobim-dxf/src/lib.rs`, `afrobim-dwg` |
| Read | The underlay reader already expands LINE, POLYLINE, SPLINE and HATCH into shapes, with its own snap collector. | `afrobim-dxf/src/reader/` |
| TRANSFORM | `transform_doc::how(category)` names how each category carries coordinates. An unknown category is `Unsupported`, and a whole-document transform is **refused** while one is present. | `afrobim-app/src/transform_doc.rs:97-114` |
| Filters a new family must respect | `imagery::is_backdrop` (underlay, image, GIS overlay); `app::ifc_exportable` (`app.rs:3373`); the sign-DXF loop that skips annotation and backdrops (`app.rs:948`); `anno_glue::pick_model` (line 180); `gis_io.rs:120`; `canvas.rs:124-171`; `app.rs:1303`, `1667`. | as listed |

There is no LINE, PLINE, ARC, CIRCLE, RECTANGLE, ELLIPSE, SPLINE, HATCH, OFFSET, TRIM, EXTEND, FILLET, CHAMFER, ROTATE, SCALE, MIRROR, ARRAY, STRETCH, JOIN, EXPLODE, BLOCK, INSERT, MATCHPROP or PROPERTIES command today. MOVE, COPY, ERASE, UNDO and the zooms exist.

## 2. Principles

1. **Follow the annotation pattern.** It already solved the same problem (a free-form element family with a session-driven editor, grips, DXF output and layers). A new crate copies its shape, not its code.
2. **One data model, many outputs.** Every drafting object reduces to a small geometry list (`DraftGeometry`: strokes, fills) that feeds the canvas, DXF/DWG, PDF and sheets.
3. **Store points, not angles.** A circle is a centre and a point on it; an arc is three points; an ellipse is a centre and two axis end points. Then ROTATE, SCALE, MIRROR, ARRAY and the TRANSFORM command are all the same operation, "map every point", and are exact for a turn, a scale and a reflection.
4. **Drafting never silently becomes the model.** It stays out of IFC, quantities, Green BIM and clash tests unless the user converts it.
5. **AutoCAD habits first.** Command names, aliases, prompts, option letters, F-keys and the right-click Enter all follow AutoCAD, so a Civil 3D user needs no training.

## 3. Element category and data model

### 3.1 Categories

| Category | Holds | Transform class |
|---|---|---|
| `draft` | One drafting entity per element, so each can be picked, moved, layered and erased by the existing kernel code | new `How::Draft` (section 7) |
| `draft-block` | A block definition: name, base point, list of entities in block-local coordinates | `NotGeographic` (local coordinates) |

One category `draft` (not one per entity kind) keeps every filter to a single test, `afrobim_drafting::is_drafting(&Element)`, in the same way as `afrobim_annotate::is_annotation`.

### 3.2 Entity types

```
DraftEntity (serde enum, in element.data, coordinates local to element.origin, mm)
  Line     { a, b }
  Polyline { verts: [ {p, bulge} ], closed, width_start, width_end }   // RECTANGLE, POLYGON, DONUT make these
  Arc      { start, mid, end }                                         // three points; direction from the order
  Circle   { centre, through }                                         // radius = distance
  Ellipse  { centre, major_end, minor_end, from_t, to_t }              // full or elliptical arc
  Spline   { fit: [p], closed, start_tangent?, end_tangent? }          // degree 3; control points derived
  Hatch    { loops: [ Loop ], pattern, angle_deg, scale, solid, assoc: [ElementId] }
  Insert   { block, at, x_ref, y_ratio }                               // two points carry turn and scale
  Point    { p }                                                       // later
common style on every entity (ByLayer when absent)
  style { colour?, linetype?, lineweight_mm?, linetype_scale? }
```

Notes:

* `Polyline` with bulges covers arcs inside a polyline exactly as AutoCAD and DXF do. A bulge keeps its value under a turn, a scale and a move, and flips sign under a reflection.
* `Spline` stores **fit points**, which survive any point-wise map. Control points and knots (clamped, degree 3, chord-length parameterisation) are computed when drawing or exporting.
* `Hatch` stores its loops as points (so a non-associative hatch is transformed like everything else) and, when associative, the ids of its boundary elements; it is rebuilt when they change. A pattern is a name plus angle and scale. The built-in patterns are our own definitions (SOLID, lines at 45 degrees, cross-hatch, dots, earth, concrete, gravel, brick, insulation, water, grass) with ISO-style names. AutoCAD's `acad.pat` is Autodesk's and is **not** copied. A user `.pat` file can be imported.
* `Insert` keeps two points: the insertion point and the point where the block's +X axis of unit length lands, plus the Y ratio. A pointwise map of those two points keeps the turn and scale correct.
* Geometry is local to `origin`, as annotation is, so MOVE, COPY, ERASE, UNDO, copy and paste, drag and the layer commands work with no kernel change. `normalize(e)` after every edit re-bases `origin` on the lower-left of the geometry and sets `width`, `height`, as `afrobim_annotate::normalize` does. Absolute accessors `read_absolute` and `replace_absolute` give editing commands model coordinates.
* Units: millimetres, as everything in `Element`. Site coordinates are large (UTM about 450 000 m, 9 960 000 m); local storage keeps precision.

### 3.3 Where it lives

A **new crate `afrobim-drafting`**, not an extension of `afrobim-annotate`:

* annotation is about text and dimensions and already carries style, units and measure code;
* drafting needs geometry kernels (intersection, offset, trim, fillet, tessellation, hatch fill) that are large and testable on their own;
* the crate depends only on `afrobim-kernel` (and `serde`); it does **not** depend on the app, egui or the DXF crate.

Modules in the crate: `entity` (types, read, write, normalize, map_points), `geom` (distances, intersections, tangent and perpendicular points), `tess` (arc, ellipse, spline to polyline), `hatch` (pattern lines clipped to loops, island detection), `edit` (offset, trim, extend, fillet, chamfer, join, explode, stretch, array, mirror), `block`, `session` (the command state machines), `props` (read and write the properties shown in the panel), `convert` (section 8), `export` (a neutral list of DXF entities, section 6).

`afrobim-annotate` is reused for the layer table, text, dimension and leader, and for `AnnGeometry`/`Seg`, which the canvas already paints (`anno_ui::paint_geometry`). `DraftGeometry` converts to `AnnGeometry` so the canvas paints drafting with no new painter; the hatch uses a new `FillKind::Solid` (a small addition to annotate) and pattern lines as strokes.

## 4. Interaction model

### 4.1 Session

Each command is a `Session` in `afrobim-drafting::session`, shaped like `anno_glue::Session`:

```
Session::start(command, &doc, layer, last_state) -> Result<Session,String>
session.prompt(&doc) -> String                  // text for the command line, e.g. "Specify next point or [Close/Undo]:"
session.point(&mut doc, Pt, pick_radius) -> Out // a click or an object-snapped point
session.pick(&mut doc, ElementId, Pt) -> Out    // a pick of an object (OFFSET, TRIM, FILLET ...)
session.text(&mut doc, &str) -> Out             // typed option letter, number, distance or point
session.enter(&mut doc) -> Out                  // Enter, space or right click
session.preview(&doc, cursor) -> Option<DraftGeometry>   // the rubber band, drawn by the canvas
```

`Out` carries the same fields as the annotation `Out` (echo lines, created id, finished, changed). The app owns one `DraftState` like `AnnoState` (`session`, `osnap`, `snap` marker, `grip_drag`) and routes pointer, keyboard and command line input to it ahead of the controller. Undo takes one step per finished command (`Out.changed`), as annotation does.

### 4.2 Point input, in the order the app applies it

1. Typed point or distance: `x,y`, `@dx,dy`, `@d<a`, `d` while the pointer gives the direction (direct distance entry). `parse_point` already covers the first three.
2. Object snap (running, F3, and one-shot by typing `END`, `MID`, `CEN`, `QUA`, `INT`, `PER`, `TAN`, `NEA`, `NOD`, `INS`, `EXT`), through the existing `candidates_with`. **Add** Perpendicular, Tangent, Nearest, Insertion and Extension to `SnapMode`; the first three need the previous point, which the session supplies.
3. Ortho (F8) and polar tracking (F10, increments 15, 30, 45, 90 degrees) relative to the last point (`ortho` exists; polar is new).
4. Grid snap (F9) and grid display (F7), as now.
5. Object snap tracking (F11) is Increment 3.

### 4.3 Dynamic input (F12)

A small floating box at the cursor, drawn by the canvas, showing the fields the prompt needs: first point shows X and Y; later points show Length and Angle (and Radius, Distance, Height as the command needs). Tab moves between fields, a typed field locks, Enter accepts, Escape cancels. It does nothing the command line cannot; it is a view of the same session, so the two always agree. Increment 2 for LINE, PLINE, CIRCLE, RECTANGLE; Increment 3 for the rest. The command line works from Increment 1.

### 4.4 Selection

* Precise hit test: add a hook to the kernel, `Document::pick_by(&dyn Fn(&Element, Pt, f64) -> Option<f64>)` returning a distance, with the existing bounding-box test as the first filter. Drafting supplies exact distance to a line, arc, polyline, ellipse, spline or hatch edge. The same applies to window and crossing: crossing selects an entity whose **geometry** touches the box, not its bounding box. Without this, picking a diagonal line selects everything in its large box.
* Add the AutoCAD selection options that edit commands need: Window (W), Crossing (C), Fence (F), Last (L), Previous (P), All (ALL), Remove (R), Add (A). QSELECT (by layer, kind, colour) is Increment 3.
* A spatial index (uniform grid of cell lists, built lazily and cleared on change) for pick, snap and TRIM, because a drawing with 20 000 entities makes linear scans too slow. Increment 2.
* Grips: every entity gets vertex grips, midpoint grips and a centre grip (Line: two ends and a midpoint; Polyline: vertices and segment midpoints, with "stretch", "add vertex", "convert to arc"; Circle: centre and four quadrants; Arc: three points; Hatch: none, edit through HATCHEDIT). Grip editing reuses the pattern of `anno_glue::move_grip`.

### 4.5 Command list

Names and aliases follow AutoCAD. Aliases are the standard acad.pgp letters. "Pick" means a selection of objects; "base" is a base point.

**Draw**

| Command (alias) | Interaction, in order | Options (letter) | Increment |
|---|---|---|---|
| `LINE` (`L`) | first point; next point, repeat; Enter ends | Close (C), Undo (U); typed length with direction (direct distance) | 1 |
| `PLINE` (`PL`) | start; next point | Arc (A), Close (C), Halfwidth (H), Length (L), Undo (U), Width (W); in arc mode: Angle, CEnter, Close, Direction, Line, Radius, Second pt | 1 (Arc, Close, Undo, Width, Line); 2 (rest) |
| `ARC` (`A`) | three points start, second, end | Centre (C), End (E), Angle (A), Length (L), Radius (R), Direction (D); the eleven AutoCAD methods | 1 (3 point; Start-Centre-End; Start-End-Radius); 2 (rest) |
| `CIRCLE` (`C`) | centre; radius or diameter | 2P, 3P, TTR (tangent, tangent, radius), Diameter (D) | 1 (centre-radius, centre-diameter, 2P, 3P); 2 (TTR) |
| `RECTANGLE` (`REC`, `RECTANG`) | first corner; other corner | Chamfer (C), Fillet (F), Width (W), Area (A), Dimensions (D), Rotation (R) | 1 (corner, Dimensions, Rotation); 2 (Fillet, Chamfer, Width, Area) |
| `POLYGON` (`POL`) | number of sides; centre; Inscribed or Circumscribed; radius | Edge (E) | 2 |
| `ELLIPSE` (`EL`) | axis end 1; axis end 2; other axis distance | Arc (A), Centre (C), Rotation (R) | 2 |
| `SPLINE` (`SPL`) | fit points; Enter; start and end tangents | Close (C), Fit tolerance (F), Object (O), Method (M) | 3 |
| `HATCH` (`H`, `BHATCH`) | pick internal point(s) or Select objects; the hatch tab shows pattern, scale, angle | Select objects (S), seTtings (T); pattern from a drop-down; Associative on or off; Islands (normal, outer, ignore) | 2 (pick point, Select, SOLID and 6 patterns, associative); 3 (islands settings, `.pat` import, HATCHEDIT) |
| `DONUT` (`DO`) | inside diameter, outside diameter, centre | | 3 |
| `POINT` (`PO`), `XLINE` (`XL`), `RAY` | point(s) | | 3 |

**Modify**

| Command (alias) | Interaction | Options | Increment |
|---|---|---|---|
| `MOVE` (`M`), `COPY` (`CO`, `CP`), `ERASE` (`E`) | exist; extend so they work on drafting objects and accept typed displacement and direct distance | COPY: Multiple, Array | exists |
| `ROTATE` (`RO`) | pick; base; angle | Copy (C), Reference (R) | 1 |
| `SCALE` (`SC`) | pick; base; factor | Copy (C), Reference (R) | 1 (note: `SCALE` is today an alias of `APPEARANCE`; remove it and keep `LOOK`, `INTERFACE`) |
| `MIRROR` (`MI`) | pick; first and second axis point; delete source? | Yes/No; text is not mirrored (as `MIRRTEXT` = 0) | 1 |
| `OFFSET` (`O`) | distance; pick object; side point; repeat | Through (T), Erase (E), Layer (L), Multiple (M) | 1 (lines, arcs, circles, polylines); 2 (ellipse, spline) |
| `TRIM` (`TR`) | cutting edges (Enter selects all); pick the part to remove | Fence (F), Crossing (C), eRase (R), Undo (U), Edge (E: extend or no extend), Project; Shift while picking extends | 2 |
| `EXTEND` (`EX`) | boundary edges; pick the end to extend | as TRIM; Shift trims | 2 |
| `FILLET` (`F`) | pick first, pick second | Radius (R), Polyline (P), Trim (T), Multiple (M); radius 0 makes a sharp corner | 2 |
| `CHAMFER` (`CHA`) | pick first, pick second | Distance (D), Angle (A), Polyline (P), Method (M), Trim (T), Multiple (U) | 2 |
| `ARRAY` (`AR`) with `ARRAYRECT`, `ARRAYPOLAR`, `ARRAYPATH` | pick; type; rows, columns, spacing | Path array along a drafting polyline **or an alignment** (spacing by station), Associative, Base point, Rotate items | 2 (non-associative rect and polar); 3 (path along alignment, associative) |
| `STRETCH` (`S`) | crossing window; base; displacement | vertices inside the window move, others stay | 2 |
| `JOIN` (`J`) | pick lines, arcs, polylines that touch | fuzz distance | 2 |
| `EXPLODE` (`X`) | pick polylines, inserts, hatches | polyline to lines and arcs; insert to its entities; hatch to its lines | 2 |
| `PEDIT` (`PE`) | pick polyline | Close, Open, Join, Width, Edit vertex, Fit, Spline, Decurve | 3 |
| `BREAK` (`BR`), `LENGTHEN` (`LEN`), `ALIGN` (`AL`), `OVERKILL` | | | 3 |

**Blocks**

| Command (alias) | Interaction | Increment |
|---|---|---|
| `BLOCK` (`B`) | name; base point; pick objects; keep, convert to insert, or delete | 2 |
| `INSERT` (`I`) | choose block (list or palette); insertion point; scale; rotation | 2 |
| `WBLOCK` (`W`) | write a block to a `.afrobim` or DXF file | 3 |
| `BEDIT`, dynamic blocks | not planned | later |

**Properties**

| Command (alias) | Interaction | Increment |
|---|---|---|
| `PROPERTIES` (`PR`, `CH`, `MO`, Ctrl+1) | opens the existing right-hand properties panel (`app.rs:2093`). For one drafting object: layer, colour, linetype, lineweight, linetype scale, and the geometry fields (Line: start X, Y, end X, Y, length, angle; Circle: centre, radius, diameter, circumference, area; Polyline: vertices with bulge, closed, total length, area; Arc: centre, radius, start and end angle, length; Hatch: pattern, scale, angle, area). Editing a field edits the entity. Several objects: shared fields only, "*VARIES*" otherwise. | 1 (layer, colour, linetype, lineweight, read-only geometry); 2 (editable geometry) |
| `MATCHPROP` (`MA`) | source object; destination objects | Settings (S): which properties | 1 |
| `CHPROP`, `LAYER` set current, `LAYMCUR`, `LAYISO` | small helpers | 2 |
| Linetype and lineweight in the ribbon "Properties" panel (Layer, Colour, Linetype, Lineweight drop-downs, ByLayer default) | | 1 |

**Aids:** `OSNAP` (`OS`) dialog and F3, `ORTHO` (F8), `POLAR` (F10), `GRID` (F7), `SNAP` (F9), `DYN` (F12), `UNITS`, `LIMITS` (not needed), `ID`, `DIST` (exist in annotate measure).

## 5. Interaction details that need a decision

* **Enter, space and right click** all end or accept. Right click opens a context menu (Enter, Cancel, Recent options) as in AutoCAD.
* **Command repetition:** Enter or space on an empty command line repeats the last command (not yet in the app; one line in the command line handler).
* **Cancel:** Escape ends the command and removes the rubber band. Nothing is stored for a half-made LINE except the segments already accepted, as AutoCAD does.
* **Preselection:** with objects selected, MOVE, COPY, ROTATE, SCALE, MIRROR, ERASE use them, as `MOVE` does now.
* **Layer on creation:** the current layer. Add a "current layer" control to the ribbon Home tab (the layer manager exists). New drafting layers default to a `DRAFT-` prefix only if the user chooses; AutoCAD does not force one.
* **Precision:** all editing commands compute in `f64` millimetres with a tolerance of 1e-6 mm for coincidence and 1e-3 mm for gap closing (JOIN default fuzz 0).

## 6. Export to DXF and DWG

Today's writer is R12. R12 cannot hold LWPOLYLINE, ELLIPSE, SPLINE, HATCH, INSERT with a block table, a linetype table or lineweights. Plan:

1. **Add an R2000 (AC1015) writer** to `afrobim-dxf`, used when the document contains drafting entities or the user ticks "AutoCAD 2000 or later". Keep the R12 path **byte-identical** (the existing golden tests in `afrobim-dxf/tests/golden.rs` must not change). Entities written natively:

| Drafting entity | DXF entity | R12 fallback (when the user insists on R12) |
|---|---|---|
| Line | LINE | LINE |
| Polyline (with bulges) | LWPOLYLINE (flags, constant or per-vertex width, bulge) | POLYLINE/VERTEX with bulge (group 42), as the sign writer does |
| Arc | ARC (centre, radius, start and end angle, in 2D with extrusion 0,0,1; a reflected arc is written with the correct direction) | ARC |
| Circle | CIRCLE | CIRCLE |
| Ellipse | ELLIPSE (centre, major axis vector, ratio, start and end parameter) | polyline of 5 degree chords |
| Spline | SPLINE (degree 3, knots, control and fit points) | polyline through evaluated points |
| Hatch | HATCH (boundary paths, pattern name and angle and scale, solid flag, associativity and the boundary handles) | the pattern lines as LINEs; solid as SOLID |
| Block, Insert | BLOCKS section entries and INSERT (scale X, Y, rotation) | explode on export |
| Text, MText, dimension, leader | existing annotation export | existing |

2. **Layer table** written from the annotation layer settings (colour, lineweight, locked, visible), plus the LTYPE table: CONTINUOUS, DASHED, DASHDOT, CENTER, HIDDEN, PHANTOM defined in our own words. Per-entity `colour` (62 or true colour 420), `linetype` (6), lineweight (370), linetype scale (48).
3. **DWG** keeps going through `dxf_to_dwg`, with the R2000 DXF as its input. **Unverified:** whether acadrust 0.6.8 reads every entity above; an Increment 1 test must round-trip each entity kind DXF to DWG to DXF and compare, before the DWG option is advertised for drafting. If an entity fails, the DWG path falls back to the explode rule above and says so in the export report.
4. **Read:** a later improvement is "import DXF or DWG as editable drafting entities" (today it is a read-only underlay). The reader already expands LINE, POLYLINE, SPLINE and HATCH into shapes; mapping those shapes into `DraftEntity` is a small step. Increment 3.
5. **PDF and sheets:** `DraftGeometry` goes into the sheet drawing the same way annotation does (`afrobim_annotate::sheet_drawing`), so drafting appears on sheets and in the PDF.

## 7. Keeping drafting out of the model, and keeping it right

### 7.1 Filters, and what each does for drafting

| Place | Rule for `draft` and `draft-block` | Test |
|---|---|---|
| `ifc_exportable` (`app.rs:3373`) | Return false. Do **not** put drafting in `imagery::is_backdrop`: backdrops are not selectable, drafting is. Add `&& !afrobim_drafting::is_drafting_category(c)`. | `draft_never_reaches_ifc` |
| Sign DXF loop (`app.rs:948`) and `gis_io.rs:120`, `anno_glue::pick_model` (`:180`), `canvas.rs:124-171`, `app.rs:1303`, `1667` | Mirror every place that tests `is_annotation`; drafting gets its own export, paint and pick branch. | one test per place, found by grep, listed in the test plan |
| 5D quantities and bill (`cost_glue::collect`), civil quantities | Ignore drafting. | `bill_unchanged_by_drafting` |
| Green BIM (`green_glue`) | Ignore drafting: no material key, no factor, no total change. | `green_total_unchanged_by_drafting` |
| Clash detection and 4D | Drafting has no solids and no schedule link, so it is ignored. | `clash_ignores_drafting` |
| Visibility, supports, sheets of signs | Ignore, except sheets and PDF which **include** drafting as annotation does. | existing sheet tests plus one |

### 7.2 Quantities

By default a drafting object contributes nothing to any quantity. Two deliberate ways in:

* **Convert** it into a model object (section 8); the model object has the quantities.
* **Measure to bill** (Increment 3): `MEASURE` takes selected drafting polylines or hatches and adds a 5D bill line with `quantity = length` or `area`, `source = draft:<element id>`, so the bill line updates when the drawing changes. Green BIM stays untouched; a material key must be typed by the user.

PROPERTIES shows length, area and perimeter of any drafting object as read-only values, so the engineer can read a quantity without making it a bill line.

### 7.3 TRANSFORM and move

* **MOVE, COPY, drag, paste:** correct with no change, because geometry is local to `origin` and `Document::translate` moves `origin`.
* **TRANSFORM (CRS change, site calibration):** teach `transform_doc::how` a new class, `How::Draft`. For each `draft` element it computes absolute points (`origin + local`), maps each through the plan (full turn and scale, not the origin-only `Rigid` class annotation has), re-normalises the element and records it in the transform log. Because arcs, circles, ellipses and inserts are stored as points, the map is exact for a turn, scale and reflection and a close approximation for a general projection over a drawing-sized area (the plan already states its accuracy). Bulge and hatch pattern angle are kept. `draft-block` is `NotGeographic`, and an `Insert` is moved through its two stored points.
* If `How::Draft` were missing, the existing rule would refuse the **whole-document** transform whenever a drafting element exists. A test in `afrobim-app/src/a3_tests.rs` style ("every category is taught") must therefore include `draft`.
* Round trip test: transform then reverse returns every point to within a few millimetres, the same bar the existing test sets.

## 8. From drafting to model objects

A single command family, `CONVERT` (with direct aliases), takes selected drafting objects and creates a model object. The drafting source is kept by default on layer `DRAFT-SOURCE` (so the user can compare) or erased on request; the new object stores `properties["from_draft"] = <name>` so the link is visible.

| Command | Takes | Makes | How |
|---|---|---|---|
| `FEATURELINE` (also `CONVERT > Feature line`) | LINE, PLINE, ARC (chained) | a feature line (grading, `civil_grading`) | Plan vertices from the polyline; arcs tessellated to a stated chord tolerance (default 5 mm sagitta); elevation by *drape on surface* (name the surface), *constant*, or *from the existing Z*. Rules entry `FEATURE` (`line` array with x, y, z) already makes TRANSFORM work. |
| `ALIGNMENTFROMPOLY` (`ALIGNFROMOBJECTS`) | a polyline | an alignment (`afrobim-alignment`) | Straight segments become tangents; bulge segments become arcs; optional fillet radius at each PI for a polyline of tangents only, and optional spiral length at each curve. Report any tangent break (non-tangent joins), as Civil 3D does. Station start asked. Then it behaves like an imported LandXML alignment (profile, corridor, station placement). |
| `DRAINPATH` | a polyline | a drain run or pipe network run (`afrobim-drainage`, `afrobim-pipes`) | Structures at vertices and ends; pipe size and cover typed or defaulted from the design basis; inverts from the surface. Rules `PIPENET` already carries coordinates. |
| `PARCELFROMOBJECTS` | a closed polyline, or lines and arcs forming a closed loop | a parcel (`civil_parcels`) | Boundary taken as bearings and distances; area and closure error shown; the existing parcel deed and topology checks run. Rules `PARCEL`. |
| `SURFACEBOUNDARY`, `BREAKLINE` | a closed polyline; a polyline with Z | surface outer boundary or a breakline | Feeds `afrobim-terrain`. |
| `EDGEOFPAVEMENT` | a polyline | a marking or kerb run along it | Later. |

The reverse, **plan geometry to drafting** (`DRAFTEXTRACT`), makes drafting polylines from an alignment, feature line or parcel, for sheets and annotation, and keeps no link to the model. Increment 3.

Conversion is one undo step. It refuses (with a message) objects that are not on a plane the target accepts: a spline is first tessellated, a hatch is not convertible.

## 9. Phasing, effort and tests

Effort is in working days of agent-assisted building, including tests and a human review of the result. It is an estimate with no measured velocity behind it; revise after Increment 1. The `afrobim-drafting` crate and its geometry kernels are the main risk, followed by the DXF R2000 writer.

### Increment 1: draw the basics and leave cleanly (about 14 days)

Contents: `afrobim-drafting` crate with `entity`, `geom`, `tess`, `session`; commands LINE, PLINE (line and arc segments, close, undo, width), ARC (3 point, start-centre-end, start-end-radius), CIRCLE (centre-radius, centre-diameter, 2P, 3P), RECTANGLE (corners, dimensions, rotation), ROTATE, SCALE, MIRROR, MATCHPROP, PROPERTIES (layer, colour, linetype, lineweight, read-only geometry); the kernel pick hook; object snap additions (Perpendicular, Nearest, Tangent, Insertion); `How::Draft` in TRANSFORM; filters of section 7.1; R2000 writer for LINE, LWPOLYLINE, ARC, CIRCLE with layers, linetypes and lineweights; canvas painting through `AnnGeometry`; ribbon "Draw" and "Modify" panels filled; `SCALE` alias moved off APPEARANCE.

Tests:

* entity: serde round trip; `normalize` keeps absolute points within 1e-6 mm; `map_points` for turn, scale and reflection leaves arcs, circles and bulges correct (compare tessellated points);
* geometry: distance to line, arc, circle, polyline; intersections of every pair; perpendicular and tangent points, with tolerance cases (parallel, tangent, coincident);
* sessions: scripted inputs per command with the exact prompts, options, undo inside LINE and PLINE, Escape leaves the document unchanged;
* pick: a diagonal line is not picked in the corner of its box; crossing selects only by geometry; locked and hidden layers are not picked;
* TRANSFORM: forward and reverse within a few millimetres; the "every category taught" test includes `draft`;
* filters: IFC, bill, Green BIM, clash, sheets, one test each (section 7.1);
* DXF: golden files for each entity; the R12 golden tests unchanged; DXF read back through `afrobim-dxf::reader` gives the same geometry; **DXF to DWG to DXF round trip through acadrust per entity kind**;
* app: a headless run that draws a rectangle, offsets nothing yet, saves, reloads, exports DXF, and compares.

### Increment 2: edit and reuse (about 18 days)

Contents: OFFSET, TRIM, EXTEND, FILLET, CHAMFER, STRETCH, JOIN, EXPLODE, ARRAYRECT and ARRAYPOLAR (non-associative), POLYGON, ELLIPSE, remaining ARC and PLINE options, CIRCLE TTR, RECTANGLE fillet and chamfer, HATCH (pick point, select objects, SOLID and six patterns, associative, islands normal), BLOCK and INSERT; dynamic input for LINE, PLINE, CIRCLE, RECTANGLE; spatial index; grip editing; editable PROPERTIES geometry; CHPROP and layer helpers; ELLIPSE and HATCH, blocks in the R2000 writer; the CONVERT family (FEATURELINE, ALIGNMENTFROMPOLY, PARCELFROMOBJECTS).

Tests:

* edit kernels: OFFSET of a line, arc, circle, closed polyline with concave corners, with self-intersection removed; TRIM and EXTEND against lines, arcs, circles, with no intersection, tangent and several cuts; FILLET and CHAMFER on all pairs of line and arc, parallel lines, radius too large; JOIN gap and tolerance; EXPLODE then JOIN returns the same length; ARRAY counts and spacing;
* hatch: pattern clipped to a concave loop with an island, area of the filled region equals loop area minus island area, associative hatch follows a moved boundary;
* blocks: insert with scale and rotation, nested insert depth limit, EXPLODE equals the transformed geometry, `draft-block` survives save and reload;
* conversion: feature line vertex count and chord tolerance, alignment from a polyline of three tangents and two arcs reproduces length within 1 mm, tangent break reported, parcel area equals polygon area, one undo step restores;
* property tests (random polylines, `proptest`-style): OFFSET then OFFSET back; TRIM never grows the length; MIRROR twice is identity;
* performance: 20 000 entities, pick and snap under 16 ms per frame on a laptop-class machine (measure, do not assume).

### Increment 3: polish, links and interchange (about 14 days)

Contents: SPLINE, DONUT, POINT, XLINE, RAY, PEDIT, BREAK, LENGTHEN, ALIGN, OVERKILL; path array along an alignment and associative arrays; object snap tracking (F11) and Extension and Parallel snaps; QSELECT; fence and polygon selection; HATCHEDIT, `.pat` import, gradient not planned; WBLOCK and a block palette; `MEASURE` to bill; `DRAFTEXTRACT`; import DXF and DWG as editable drafting entities; dynamic input for all commands; spline, hatch and insert fully in the DWG round trip.

Tests: spline through fit points passes through every fit point within 1e-6 mm and exports as a valid SPLINE read by acadrust; PEDIT fit and decurve; path array positions equal the alignment stations; `MEASURE` bill line updates after a vertex move; import of a real DXF (the repository's `examples/out/*.dxf` and a Civil 3D export) gives the same extents as the underlay; the whole drafting suite re-run on Windows and macOS CI.

### Total and order

About 46 working days of agent-assisted effort for the full set. The order matters: Increment 1 is what makes the app a usable drafting tool and unblocks DWG interchange; Increment 2 is what engineers actually use daily (OFFSET, TRIM, FILLET, HATCH); Increment 3 can follow the beta.

## 10. Risks and open questions

1. **Geometry kernels are easy to get subtly wrong** (OFFSET of concave polylines, TRIM with tangents, hatch islands). Mitigation: keep each kernel in `afrobim-drafting` with property tests, and no GUI types, so they can be run on thousands of random cases.
2. **acadrust coverage** of ELLIPSE, SPLINE, HATCH and INSERT on DWG write is not verified. The Increment 1 round-trip test decides; the fallback is explode-on-export for DWG.
3. **R2000 writer** is new code in the exporter that every sign sheet uses. Keep the R12 path untouched and golden-tested.
4. **Unit and scale of linetypes and hatch patterns** in millimetre models: model-space scale versus paper-space scale. Start with a per-document "drawing scale" setting (default 1:1000 for roads) used for linetype and hatch spacing, until paper-space layouts exist (roadmap, later).
5. **Block library and licensing:** do not copy AutoCAD's hatch patterns or blocks; write our own and keep the licence note in `NOTICE`.
6. **Decision for the owner:** whether to accept `afrobim-drafting` as a separate crate (recommended) and the SCALE alias change.
