# afrobim-alignment

Horizontal road alignments for AfroBIM: geometry and stationing, LandXML import, the rules for
standing a sign beside an alignment, and the alignment as a kernel element drawn in plan.

## Conventions

| | |
|---|---|
| Lengths, stations, offsets, radii | metres |
| Angles in the geometry | radians |
| Plan axes | X east, Y north |
| Heading | counter-clockwise from +X (east), in the direction of increasing station; `point_at` returns it in [0, 2π) |
| Curvature | 1/m, positive turning left (counter-clockwise), negative turning right; 0 on a tangent |
| Station | `start_station_m` + distance along the elements; no station equations |
| Offset | square to the alignment, **positive to the right** looking up-station, negative to the left (the `SignLocation` convention) |
| Station text | kilometres + metres: `1+250.000` (`format_station`) |
| Sign rotation | about +Z, counter-clockwise from +X. A sign's local +X is its width and its face looks towards local −Y, so rotation = heading of the traffic it serves − 90°. Radians in `PlacementPoint`, degrees in `PlacedPosition` |
| Kernel hand-over | `PlacedPosition` is in **millimetres** and degrees, as `afrobim_kernel::SitePlacement` stores them; station and offset stay in metres |

The point at station `s`, offset `d`, where the alignment is at (x, y) on heading θ, is
(x + d sin θ, y − d cos θ).

## Geometry

An `Alignment { name, start_station_m, elements }` is a chain of `AlignmentElement`s. Each element
holds its start point, start heading, length and the curvature at its two ends; curvature varies
linearly between them. Equal zero curvatures make a tangent, equal non-zero ones a circular arc,
and anything else a clothoid (Euler spiral): tangent to curve, curve to tangent, or curve to curve
(a compound spiral).

- `point_at(station)` → `StationPoint { x, y, heading_rad }`; `point_at_offset(station, offset)`.
- `station_offset(x, y)` → `Some((station, offset))` for the nearest point of the alignment, or
  `None` when the point is behind the start or beyond the end.
- `polyline(max_chord_m)` for drawing; `length()`, `end_station_m()`, `min_radius()`,
  `joints()` / `discontinuities(..)` to check smoothness.
- `Alignment::from_pis` builds tangent, spiral, curve, spiral, tangent through a list of PIs with
  a radius and optional spiral lengths (equal or unequal) at each. Left and right curves, and
  reverse curves that meet without a tangent, come out of the PI positions.
- `AlignmentBuilder` lays elements end to end, which is how to make compound curves and anything
  else a PI list cannot say. `Alignment::from_elements` takes finished elements as they are.

Arcs and tangents are evaluated in closed form. Clothoids are Fresnel integrals, evaluated by
8-point Gauss-Legendre quadrature on panels of at most 0.5 rad of heading change, which is
accurate to rounding; the formulae, their source and the tolerance are stated at the top of
`src/geometry.rs`, and the tests hold the result to 1e-12 × length against the standard series
(`clothoid_xy`) and to 1e-6 m against step-by-step integration of the heading.

```rust
use afrobim_alignment::{place, Alignment, Pi, SignLocation, TrafficSide};

// Three PIs; a 300 m curve with 60 m spirals at the middle one. Station 1+000 at the start.
let road = Alignment::from_pis(
    "MC01",
    1000.0,
    &[Pi::point(0.0, 0.0), Pi::spiral_curve(600.0, 0.0, 300.0, 60.0, 60.0), Pi::point(1000.0, 500.0)],
)?;
let p = road.point_at(1250.0).unwrap();              // x, y (m) and heading (rad)
let (station, offset) = road.station_offset(p.x + 3.0, p.y).unwrap();

// A sign 3.5 m to the left at 1+250, left-hand traffic.
let location = SignLocation { alignment: Some("MC01".into()), station: Some(1250.0), offset: Some(-3.5), side: None };
let placed = place(&road, &location, TrafficSide::Left)?;
for position in &placed.positions {
    let site = position.site_placement(&road.name); // afrobim_kernel::SitePlacement, mm and degrees
}
```

## LandXML

`read_landxml(path)` / `parse_landxml(text)` return `LandXmlImport { alignments, notes }`.

Read, following the LandXML 1.2 schema (elements are matched by local name, so the 1.0, 1.1 and
2.0 namespaces read the same way):

- `Units/Metric@linearUnit` (`meter`, `millimeter`, `centimeter`, `kilometer`) and
  `Units/Imperial@linearUnit` (`foot`, `USSurveyFoot`, `inch`, `mile`); everything is converted to metres.
- `Alignments/Alignment` with `@name` and `@staStart`.
- `CoordGeom/Line`: `Start`, `End`.
- `CoordGeom/Curve`: `Start`, `Center`, `End`, `@rot` (`cw` or `ccw`). The radius is Start to Center.
- `CoordGeom/Spiral` with `@spiType="clothoid"`: `Start`, `End`, `@length`, `@radiusStart`,
  `@radiusEnd` (`INF` at the tangent end), `@rot`.
- Points as text, **northing then easting** (an elevation after them is ignored), or as a `pntRef`
  to a `CgPoint`. They become X = easting, Y = northing.

The direction and other redundant attributes (`dir`, `dirStart`, `dirEnd`, `delta`, `chord`,
`tangent`, `theta`, …) are not used: the geometry is taken from the points. Where `length` or
`radius` is given and disagrees with the points by more than 1 mm, a note says so. A note is also
made for every joint where the elements do not meet smoothly.

An alignment that contains something unsupported is left out and a note says why; the rest of the
file is still imported.

The test files in `tests/landxml/` were written by hand to the schema and validate against
`LandXML-1.2.xsd`. **No file exported by Civil 3D was available to test against**, so how the
importer copes with what Civil 3D actually writes is not yet proven.

## Sign placement

`resolve`, `rotation`, `normalize_rad` and `location_report::write` are a port of
`SignPlacement` and `LocationReport` in `src/SignCore.Cli/SignPlacement.cs`: which side a sign
stands on, the offset it is placed at, the traffic it faces, its rotation, the warning when
`side` overrides the offset's sign, and the location schedule CSV. `tools/GoldenPlacement` runs
the C# over 4256 cases and five reports and writes the results to `tests/golden/`;
`tests/golden.rs` compares every one (numbers within 1e-12, warning and error text and the CSV
byte for byte). To regenerate: `dotnet run --project tools/GoldenPlacement`.

`place(&alignment, &location, traffic_side)` is new. It takes the point and heading at the
location's station, applies `resolve`, and returns `Placement { positions, warning }` with one
`PlacedPosition { side, x_mm, y_mm, rotation_deg, station_m, offset_m }` per sign (two for
`side = both`, left first).

## Kernel module

`AlignmentModule` owns the category `"alignment"`; the element's `data` is the `Alignment`
serialised with serde. `generate` returns:

- a plan drawing in millimetres, Y up (north), unrotated, at 1000 units per metre, with its
  lower-left extent on the origin. The centreline is hairlines (`Primitive::Line`, width 0), with
  curves chorded to within 5 mm; station ticks are hairlines square to it at a round interval
  (at most 50 ticks), longer on the major stations. **There are no station labels**: the draw
  crate has glyph outlines but no text primitive.
- derived values: `length_m`, `start_station`, `end_station`, `start_station_m`, `end_station_m`,
  `elements`, `lines`, `arcs`, `spirals`, `min_radius_m`, `tick_interval_m`, `plan_origin_x_m`,
  `plan_origin_y_m`.
- a warning note for each joint that is not smooth.

**Where the drawing sits.** The drawing origin is the plan point (`plan_origin_x_m`,
`plan_origin_y_m`): the lower-left corner of the box around the centreline and its ticks. A
drawing point (u, v) in millimetres is the plan point (`plan_origin_x_m` + u/1000,
`plan_origin_y_m` + v/1000) in metres. To lay the drawing over site coordinates in millimetres,
translate it by 1000 × the origin. `plan_drawing(&alignment)` returns the same drawing with the
origin as numbers (`PlanDrawing`), which is easier than parsing the derived strings.

## Not supported

- Vertical profiles, superelevation, cant and cross-sections. This crate is plan only; elevation
  is always 0 in what it hands to the kernel.
- Station equations. They would be simple arithmetic on their own, but they make a station name
  ambiguous (the same value can occur twice), which needs a decision about how the schedule and
  `SitePlacement` refer to a station; that decision is not this crate's to take. LandXML
  `StaEquation` is ignored with a note.
- Spirals that are not clothoids (Bloss, cubic parabola, sinusoid, …), and LandXML
  `IrregularLine` and `Chain`.
- Station labels and any other text in the plan drawing.
- Writing LandXML.
- `station_offset` for a point as far from a curve as its centre has no single answer and
  returns one of the candidates; a point that close to the evolute of a sharp curve may be
  matched to a neighbouring foot.
