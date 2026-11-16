# afrobim-terrain

The surface object of a civil design package, as an engine with no user interface.

A surface has a **name**, a **role** (existing ground, design, subgrade, formation, other), a
**source record**, a **status** (draft, for review, approved, superseded) and a **definition**:
an ordered list of operations that rebuilds the surface when replayed. The definition is plain
serde data, so a document stores the list, not the triangles, and rebuilds on load. The result is
the `Tin` type of `afrobim-corridor`.

Metric, `f64`. Coordinates are true eastings and northings (500 000 m and up, northings up to
10 000 000 m). Points keep exactly the coordinates they were given: the triangulation works on
indices, and any arithmetic that needs precision (triangulation, overlay clipping) is done after
a shift to the south-west corner of the data, with results returned in source coordinates. Only
points *created* by the build (where two lines cross) are computed, as `local + origin`.

## Contents

| Module | What it does |
| --- | --- |
| `definition`, `build` | `Surface`, `SurfaceDefinition`, `Operation`, `build() -> BuiltSurface { tin, report }` |
| `contour` | minor and major contours as polylines; optional Chaikin smoothing (cosmetic) |
| `analysis` | slope and aspect, elevation and slope bands, slope arrows, flow paths, spot elevation, sections, extremes, areas |
| `volume` | exact TIN-to-TIN volume (cut, fill, net), bounded volume, grid method, volume against a level, stage-storage |
| `grid` | regular grid and Esri ASCII grid to TIN; point-reduction simplification |
| `display` | triangles, wireframe, outline, contour and breakline polylines as plain `[f64; 3]` data |

## Definition: operations and their order

| Operation | Effect |
| --- | --- |
| `AddPoints { group, points }` | point group: ids and `x, y, z` |
| `AddBreaklines { lines }` | `Standard` (3D line, its vertices are surface points) or `Proximity` (plan line draped on the surface of the other data) |
| `AddBoundary { name, kind, vertices }` | `Outer` (keep inside), `Hide` (remove inside), `Show` (restore inside) |
| `AddContourData { name, lines, thin_distance, max_segment }` | vertices of contour lines as points; optional thinning and supplementing |
| `MaxTriangleEdge { length }` | remove triangles with a plan side longer than `length` |
| `ExcludeElevations { below, above }` | drop points outside the limits |
| `AddPoint`, `DeletePoint { Id or Near }`, `RaiseLower { delta }` | edits of the data |
| `Paste { source }` | another definition or a finished TIN; it wins where it overlaps |
| `DeleteTriangle { x, y }`, `SwapEdge { a, b, tolerance }` | edits of the triangulation |

Order rules (stated once, and kept in the type documentation):

* **Data operations** (points, breaklines, boundaries, contour data, exclusions, add and delete
  point, raise or lower, paste) act on the data gathered up to their place in the list. A point
  added after a `RaiseLower` is not raised.
* **Triangulation operations** (maximum edge length, boundaries, delete triangle, swap edge) act
  on the triangulation of *all* the data, whatever their place among the data operations. Among
  themselves: maximum edge length first, then boundaries in list order, then delete and swap
  edits in list order. A `Show` boundary can bring back triangles the maximum edge length or a
  `Hide` removed.
* A later `MaxTriangleEdge` replaces an earlier one.

Build steps: (1) replay data operations; (2) merge points closer than
`settings.duplicate_tolerance` (default 1 mm; policy first, lowest, highest or average; a point
on a breakline always takes the breakline's elevation); (3) cut crossing lines at the crossing;
(4) constrained Delaunay triangulation; (5) filters; (6) edits; (7) drop points no triangle uses.

`BuildReport` records: points in, used, merged (with the largest elevation difference among
merged points), excluded, deleted, replaced by a paste and unused; every line crossing found
(position, the elevation each line carried, the one used, and the names); triangles formed,
removed (maximum edge, boundary), restored (show), deleted; edges swapped; zero-area triangles;
and plain-language warnings (an edit that found nothing, a swap that was refused, and so on).

### Interpretations that are this crate's own

* **Crossing breaklines.** Where two breaklines cross in plan the crossing becomes a point on both.
  Its elevation is the mean of the elevations the two lines have there, and both values are
  reported so a bad crossing is visible. For a line crossing a boundary the breakline's elevation
  is used. This is a choice, not a standard; check the report.
* **Proximity breakline.** The term is used for lines whose own elevations are not trusted. Here:
  each vertex takes the elevation of the surface built from the other data (nearest data point
  outside its hull), and the line then acts as a standard breakline. No vendor algorithm is
  copied.
* **Paste.** Loose points inside the pasted surface's extent are removed, and every edge of the
  pasted triangulation becomes a breakline, so the pasted triangles reappear. A pasted surface
  with holes or a concave outline has them filled by the triangulation unless a hide boundary
  covers them.
* **Boundary elevations.** Boundary vertices take their elevation from the surface of the data
  (or from a data point if they coincide with one).
* **Maximum triangle edge** is a plan length, applied to every triangle, interior ones too.

## Methods and citations

* **Constrained Delaunay triangulation.** L. P. Chew, "Constrained Delaunay triangulations",
  Algorithmica 4, 1989; incremental construction as in F. Anglada, "An improved incremental
  algorithm for constructing restricted Delaunay triangulations", Computers & Graphics 21(2),
  1997. Implemented by the `spade` crate (2.15). Predicates are exact: J. R. Shewchuk,
  "Adaptive precision floating-point arithmetic and fast robust geometric predicates", Discrete &
  Computational Geometry 18, 1997 (`spade` uses them internally; this crate also uses the
  `robust` crate for its own orientation tests and for the winding of every triangle).
* **Why `spade`.** `delaunator` (used by `afrobim-corridor` for survey surfaces) has no
  constraints, so it cannot honour breaklines; `spade` has constrained triangulation with
  exact predicates and splitting of crossing constraints. `survey.rs` of the corridor crate is
  not reused to triangulate for that reason, but its `Tin` type is the output, and a survey
  point list can be turned into `SurfacePoint`s by the caller.
* **Contours.** Marching triangles, the two-dimensional member of the family of W. E. Lorensen
  and H. E. Cline, "Marching cubes", SIGGRAPH '87 (Computer Graphics 21(4)). Segments are chained
  by the identity of the edges, so closed contours close exactly. A corner exactly at a level
  counts as above it, so a horizontal facet at exactly a contour level has no line, and a level
  that only touches a vertex gives none. Contours run with the higher ground on the left.
  Smoothing is Chaikin corner cutting (G. M. Chaikin, "An algorithm for high speed curve
  generation", Computer Graphics and Image Processing 3(4), 1974) and is **cosmetic**: the
  smoothed line is flagged `smoothed`, is not on the TIN, and nothing in this crate uses it.
* **Slope and aspect.** Exact per triangle from the plane. Aspect is the bearing of steepest
  descent clockwise from grid north (the +y axis), as in P. A. Burrough and R. A. McDonnell,
  *Principles of Geographical Information Systems*, OUP 1998.
* **Bands.** Each triangle is clipped against the break values (Sutherland and Hodgman,
  "Reentrant polygon clipping", Communications of the ACM 17(1), 1974).
* **Flow paths.** Steepest descent across facets, along valley edges, and through vertices,
  after the TIN flow-path construction of N. L. Jones, S. G. Wright and D. R. Maidment,
  "Watershed delineation with triangle-based terrain models", Journal of Hydraulic Engineering
  116(10), 1990, without their treatment of dispersion. Pits are not filled.
* **Esri ASCII grid.** Esri, "Esri ASCII raster format" (ArcGIS documentation).
* **Simplification.** Greedy insertion, after M. Garland and P. S. Heckbert, "Fast polygonal
  approximation of terrains and height fields", CMU-CS-95-181, 1995 (earlier: R. J. Fowler and
  J. J. Little, "Automatic extraction of irregular digital terrain models", Computer Graphics
  13(2), 1979).
* **Summation.** Neumaier's improved compensated summation (A. Neumaier, "Rundungsfehleranalyse
  einiger Verfahren zur Summation endlicher Summen", ZAMM 54, 1974) in the volume code.

## Volumes: method

`volume_between(existing, design, region)` is **exact** for the TIN surfaces, not a grid
estimate. Overlay the two triangulations: every triangle of one is clipped by every overlapping
triangle of the other (convex clipping, at most a hexagon). On each piece both surfaces are
planes, so the difference `d = z_existing - z_design` is affine. The integral of an affine
function over a triangle is the area times the value at the centroid, which is the mean of the
three corner values:

    V = A (d0 + d1 + d2) / 3

(the prismoid formula for a triangular prism with plane top and bottom; exact because `d` is
affine, not an approximation). A piece is fanned into triangles. Where `d` changes sign inside a
piece it is cut along `d = 0`, the positive part is **cut**, the negative part (sign changed) is
**fill**. The same machinery against a horizontal level gives `volume_against_level`, and
`stage_storage` evaluates the volume below each of a list of levels (and the water-surface
area).

Conventions: the first surface is existing ground, the second the design. Cut = existing above
design. `net = cut - fill` (positive is surplus cut). No swell, shrinkage or compaction factor is
applied. Only plan area where both surfaces exist counts; the areas covered by only one are
reported (`first_only_area`, `second_only_area`) so a mismatch is not silent. `region` is a
simple polygon, convex or not (it is triangulated by CDT; a self-crossing polygon is refused).

`volume_between_grid` is the grid method: sample both at cell centres, sum `d * cell^2`. It is a
check, not a substitute. Its error is of the order of the cell size times the length of the
surface edges and creases, and it steps the outline; the exact method has neither effect.

Tests compare against closed forms: a prism (30 000 m3), a wedge (25 000 m3, also at an easting
of 512 345 and a northing of 9 876 543), a pyramid (800 m3), a balanced cut and fill on a
crossing plane (6 250 m3 each), planes on unrelated triangulations, an L-shaped and a triangular
region, and the stage-storage of an inverted pyramid (`400 h^3 / 108`). Agreement is at 1e-10
relative or better.

## Limits (stated plainly)

* **Left out:** wall breaklines; exact watershed and catchment delineation (only the flow path
  from a point); depression filling; point-group and survey-file ingestion (the caller gives
  coordinate arrays with ids; `afrobim-corridor::parse_survey_points` can supply them);
  LandXML export; a swell or shrinkage factor; natural-neighbour or kriging surfaces;
  georeferencing (the coordinate system is a text field in the source record; the engine does not
  interpret it); rendering (display helpers return data only, and the `afrobim-draw` crate is
  not used).
* Breakline vertices are not filtered by `ExcludeElevations`, and are not affected by
  `DeletePoint` (only loose points are).
* Contour data become points only; they are not breaklines. A surface built from contour lines
  alone has flat triangles along them, the known artefact of contour-derived surfaces.
* Where the rounding of a computed crossing leaves two pieces crossing, the build cuts one of
  them again at the new crossing, with the elevation taken along that piece. This is reported
  only through the point count.
* Vertex merging is by distance within the tolerance, using the first point read as the
  position. Points that chain (A near B, B near C, A not near C) merge into the first only.
* Flow paths stop in every pit, however small, and never split. A path that ends on a vertex on
  the edge of the surface with nothing lower on the surface reports `Pit`.
* `simplify_*` guarantees the vertical error at the dropped *input points*, not between them,
  does not respect breaklines, and keeps the outline only at the vertex level.
* A `Tin` is limited to what `afrobim-corridor` accepts (finite coordinates, triangles with
  plan area). Overlapping triangles in a *pasted* TIN built by hand are not detected.

## Dependencies

| Crate | Licence | Used for |
| --- | --- | --- |
| `spade` 2.15 | MIT OR Apache-2.0 | constrained Delaunay triangulation, exact predicates |
| `robust` 1.2 | MIT OR Apache-2.0 | exact orientation test |
| `serde`, `serde_json` (with `float_roundtrip`), `thiserror` | MIT OR Apache-2.0 | already in the workspace |
| `afrobim-corridor` | workspace | the `Tin` type |

`serde_json`'s `float_roundtrip` feature is enabled so a stored definition reads back the same
coordinates bit for bit (without it the default float parser can differ in the last digit).
Cargo unifies features across the workspace, so other crates also get exact float parsing.

## Tests

`cargo test -p afrobim-terrain` from `afrobim/`. Degenerate inputs covered: collinear points
(also at 500 000 m / 9 000 000 m), all points coincident, non-finite coordinates, duplicates
(all four policies), crossing breaklines (also at survey magnitude), nearly collinear points
with one off-line point, empty and invalid operations, holes, and a swap that is refused.
