# afrobim-grading

Feature lines, sites and grading objects of a civil design package, as an engine with no user
interface. Metric, `f64`. Coordinates are true eastings and northings (tested near 450 000 m,
9 960 000 m): arithmetic that mixes points is done relative to a local origin and results come
back in source coordinates.

**No engineering value is built in.** Cut and fill slopes, bench or batter sizes, platform
crossfall, shrink and swell factors, ray spacing, facet angle, tolerances and search limits are all
required arguments; there is no `Default` for any of them.

## Contents

| Module | What it does |
| --- | --- |
| `featureline` | `FeatureLine`: PI and elevation-point vertices, line and arc spans (DXF bulge); creation, editing, grade table |
| `site` | `Site`: lines that share one elevation where they cross, by a priority rule, with a report |
| `criteria` | `Slope`, `Direction`, `Criterion` (target surface, distance, elevation, relative elevation) |
| `rays` | nodes along a footprint and the projection rays (normal, miter, radial fan) |
| `grading` | `Grading`: projection, regions and transitions, clean-up, faces, infill |
| `group` | `GradingGroup`: one surface, terrain definition, rebuild data, volumes, raise and lower, auto-balance |
| `helpers` | building pad, pond, lay-by tie-in, ditch or berm by a stepped template |
| `output` | slope pattern ticks, group geometry, CSV tables |

Everything is `serde` serialisable. Volumes and stage-storage come from `afrobim-terrain`, the
surface type is the `Tin` of `afrobim-corridor`.

## Feature lines

Vertices are **PIs** (define plan geometry) or **elevation points** (on a span, only an elevation).
A span between two PIs is a line, or an arc if the first PI has a bulge `tan(theta/4)` (positive
counter-clockwise). Elevation is linear in station between vertices.

* Create: `from_points`, `closed_from_points`, `with_bulges`, `from_polyline_on_surface` (drape,
  optionally adding elevation points at triangle edges), `from_alignment_profile` (plain arrays:
  plan samples, their stations, and a profile as `[station, elevation]` points).
* Edit: `set_elevation`, `set_grade` (hold start or end), `set_slope_ratio`, `raise_lower`,
  `raise_lower_range`, `quick_edit`, `insert_elevation_point`, `insert_pi`, `delete_vertex`,
  `set_elevations_from_surface`, `reverse`, `join`, `break_at`, `trim`/`extract`, `fillet`, `weed`.
* Table: `grade_table()` gives per segment stations, length, dz, grade percent, slope ratio, 3D
  length; `grade_table_csv` writes it.
* Offsets: `offset_stepped(offset, ElevationStep, facet_angle)` makes a new feature line at a plan
  offset (positive left) with an elevation difference or a grade (`dz = grade/100 * |offset|`),
  mitered at the vertices, arcs flattened to straight spans. `sweep_template` sweeps a template of
  offset and dz steps (ditch, berm) and returns one polyline per template point plus triangles; it
  uses the same rays as a grading, so corners are mitered inside and fanned outside.

## Sites and crossings

`Site::resolve_crossings(facet_angle)` finds every crossing of two lines in plan and gives both
lines the elevation of the winner: higher `style_priority`, else the larger `last_edited` counter
(kept by `Site::add` and `Site::edit`), else the line added first (reported as a tie). Each
crossing is reported with both elevations before and after (`crossings_csv`).

## Grading criteria and slope format

A `Slope` is a positive magnitude, `Ratio(h)` (h horizontal to 1 vertical) or `GradePercent(g)`
(rise per 100 of run). Direction comes from the criterion or a separate `Direction`.

* `TargetSurface { cut_slope, fill_slope, max_distance }`: at each ray the slope rises (cut slope)
  where the ground at the footprint is above the footprint, falls (fill slope) where below; the
  daylight is the first exact intersection with the piecewise linear ground.
* `TargetDistance { distance, slope, direction }`.
* `TargetElevation { elevation, cut_slope, fill_slope }`: cut slope when the target is above the
  footprint, fill slope when below (this crate's reading of the two slopes).
* `TargetRelativeElevation { delta, slope }`.

## Grading object

`Grading::project` uses nodes at every vertex, region boundary and range end, and at most
`ray_spacing` apart (and at most `facet_angle` apart on arcs). Rays: normal on straight runs and
arcs; at an **interior corner** one ray on the bisector of the two normals with miter factor
`1/cos(phi/2)`, so the point at perpendicular distance `t` lies on both offset lines; at an
**exterior corner** a radial fan of `ceil(phi/facet_angle)` equal steps. Clean-up: normal rays
that overshoot an interior corner's miter ray are dropped, then loops in the daylight polyline are
collapsed onto their crossing. Faces are triangles (quads split on the shorter diagonal),
counter-clockwise. Regions give a station range with one criterion or a `Transition` that blends
the daylight distance and elevation of two criteria linearly with station; where two regions meet
the node's rays are emitted once per region. A ray with no daylight is reported (status, warning,
missing faces), never hidden. Infill of a closed footprint (or of a closed daylight line, for a
pond floor) uses the terrain engine's constrained Delaunay triangulation.

## Groups, volumes, balance

`GradingGroup` holds gradings, infills, a `shift` and the projection parameters. `build(site,
ground)` welds all triangles into a `Tin`; `GroupBuild::to_definition()` gives an
`afrobim_terrain::SurfaceDefinition` (points, every mesh edge as a breakline, outer and hide
boundaries) that rebuilds the same surface. `RebuildData` fingerprints the footprints, the ground
and the group settings; `stale_reasons` says what changed. `volume_against` calls
`afrobim_terrain::volume_between(base, group)` (cut where the base is higher). `raise_lower(step)`
moves the footprints of the whole group (the daylight moves with them). `auto_balance` finds the
shift giving a target net volume (after required shrink and swell factors) by the Illinois
variant of regula falsi inside a bracket you give, with a tolerance and an iteration limit you
give; every evaluation is returned. `stage_storage` goes through `afrobim_terrain::stage_storage`.

## Helpers

`building_pad` (outline, `PlatformSurface::Level` or a `Plane` with explicit crossfall, batters to
ground, platform infill), `pond` (rim line, floor level, one inside slope, floor infill; use
`rim_region` with `stage_storage`), `layby_tie_in` (grading from an edge line, open or closed),
`sweep_template` (ditch or berm).

## Tests

The tests compare with closed forms, shown in comments: a rectangular pad on flat ground (box,
prisms and the discretised quarter-cone fans) in fill and in cut; a straight edge on planar
ground with cross and long-fall (integral of triangular sections); an L-shaped pad with a reflex
corner (the max of two slopes, `(2/3) d r^2` in the corner square); a square pond (prismatoid
volumes at two levels through stage-storage); balance on a symmetric tilted site.

## Limits

* Arcs: ray positions on arcs follow the facet angle, crossings of sites are found on arcs
  flattened at the facet angle given (error is one sagitta), and draping does not add elevation
  points along arc spans. `fillet` needs two straight spans; `weed` leaves arcs alone; `insert_pi`
  and `delete_vertex` straighten the spans they touch.
* The interior-corner miter assumes the neighbouring walls project with the same slope; with
  different criteria on the two walls the faces meet along the bisector but are not tied exactly.
  Dropping rays near an interior corner means the daylight there follows the remaining rays, so on
  uneven ground use a smaller `ray_spacing`.
* The loop clean-up trims loops by collapsing points; it does not resolve overlap between
  different gradings of a group (the `Tin` does not detect overlapping triangles either).
* Where the ground leaves the surface or no crossing is found within `max_distance`, the ray
  fails and the faces beside it are missing; the group then does not close.
* A footprint with a self-intersection, or an interior corner sharper than the miter allows
  (the normals nearly opposite), is not supported. A footprint that reverses on itself is refused.
* Transitions blend along the same ray, so both criteria must find daylight there.
* Ponds assume the rim is the highest line of the basin; stage-storage does not test for spill.
* Not included: smoothing of feature lines, stepped offset that keeps arcs as arcs, grading to a surface by a corridor-style station
  interval criterion, optimisation of grading by slope limits, detection of overlapping gradings,
  daylight cut and fill slopes that differ per side of a ditch.
