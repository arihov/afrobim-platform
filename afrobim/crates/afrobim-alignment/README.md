# afrobim-alignment

Road alignments for AfroBIM: horizontal geometry and stationing, the vertical profile, stored
superelevation, station equations, LandXML import of all four, the rules for standing a sign
beside an alignment, and the alignment as a kernel element drawn in plan.

**The LandXML reader is not tested against Civil 3D exports in general.** The test files were
written by hand to the LandXML 1.2 schema. One real export is read in the tests,
`tests/landxml/civil3d_sr15_export.xml` (Autodesk Civil 3D 2023, US survey feet, one alignment with
a design profile and an existing-ground profile, 14 feature lines; it validates against the schema).
It exercises the units, the coordinate system, the horizontal geometry, `ProfSurf`, `ProfAlign`
and `PlanFeatures`, and nothing else: no surface, parcel, point, cross-section or superelevation
from Civil 3D has been read. See the end of the LandXML section.

## Conventions

| | |
|---|---|
| Lengths, stations, offsets, radii | metres |
| Angles in the geometry | radians |
| Plan axes | X east, Y north |
| Heading | counter-clockwise from +X (east), in the direction of increasing station; `point_at` returns it in [0, 2π) |
| Curvature | 1/m, positive turning left (counter-clockwise), negative turning right; 0 on a tangent |
| Station | The **internal** station: `start_station_m` + distance along the elements, continuous. Every function takes internal stations unless it says otherwise. The **displayed** station is the internal one taken through the station equations |
| Elevation, grade | metres; grade as a ratio (m/m), positive rising up-station |
| Cross-fall | percent, one value for each side of the centreline, **positive where the surface rises away from the centreline**, negative where it falls (a crowned road is negative on both sides) |
| Offset | square to the alignment, **positive to the right** looking up-station, negative to the left (the `SignLocation` convention) |
| Station text | kilometres + metres: `1+250.000` (`format_station`) |
| Sign rotation | about +Z, counter-clockwise from +X. A sign's local +X is its width and its face looks towards local −Y, so rotation = heading of the traffic it serves − 90°. Radians in `PlacementPoint`, degrees in `PlacedPosition` |
| Kernel hand-over | `PlacedPosition` is in **millimetres** (x, y and z) and degrees, as `afrobim_kernel::SitePlacement` stores them; station and offset stay in metres, and the station is the internal one |

The point at station `s`, offset `d`, where the alignment is at (x, y) on heading θ, is
(x + d sin θ, y − d cos θ).

## Geometry

An `Alignment { name, start_station_m, elements, profile, superelevation, station_equations }` is a
chain of `AlignmentElement`s with three optional parts described further down. Each element
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

## Vertical profile

`Profile { name, pvis }` is a list of `Pvi { station_m, elevation_m, curve }`: straight grades
between PVIs, and at any interior PVI a `VerticalCurve`: `None`, `Parabola { length_m }`,
`AsymmetricParabola { length_in_m, length_out_m }` or `Circular { radius_m }`. Stations are
internal stations.

- `elevation_at(station)`, `grade_at(station)`, `point_at(station)` (both). Outside the first and
  last PVI they return `ProfileError::OutOfRange`; nothing is extrapolated.
- `range()` is the first and last station covered; `curve_extents()` the PVC and PVT of each curve.
- `Profile::new` / `validate()` require at least two PVIs, finite numbers, strictly increasing
  stations, no curve on the first or last PVI, positive lengths and radii, and curves that do not
  run into each other or past the neighbouring PVI (they may touch).

How it is computed, at a PVI at station `s`, elevation `z`, grade `g1` in and `g2` out, `A = g2 − g1`:

| | |
|---|---|
| Grade line | `z_i + g_i (x − s_i)`, `g_i = (z_{i+1} − z_i)/(s_{i+1} − s_i)` |
| Symmetric parabola, length `L` on plan | PVC at `s − L/2`, `z_pvc = z − g1 L/2`; `y = z_pvc + g1 x + A x²/(2L)`, grade `g1 + A x/L`, `x` from the PVC |
| Asymmetric parabola, `L1` in, `L2` out, `L = L1 + L2` | two parabolas with a common grade at the PVI station: rates of change of grade `r1 = A L2/(L L1)` then `r2 = A L1/(L L2)`. They meet at `z + A L1 L2/(2L)` with grade `g1 + A L2/L` |
| Circular, radius `R` | the arc tangent to both grade lines in the station/elevation plane at 1:1. `T = R tan(abs(atan g2 − atan g1)/2)` along each grade, so it runs from `s − T cos(atan g1)` to `s + T cos(atan g2)`; `z = z_c ∓ √(R² − (x − x_c)²)` about its centre |

At a PVI without a curve the grade jumps; `grade_at` returns the grade ahead (the grade behind at
the last PVI). **No design check is made**: no K value, sight distance or limiting grade.

## Superelevation

**Stored data only.** `Superelevation { sections }` holds, in each `SuperelevationSection`, a list
of `CrossFallPoint { station_m, left_pct, right_pct, kind }`: the cross-fall stated for each side
at a station. `cross_fall_at(station)` returns `CrossFall { left_pct, right_pct }`:

- each side is interpolated **linearly between the two stated stations either side** of the
  station, inside one section, and only when both of them give that side;
- at a stated station it is the stated value;
- anywhere else — a side not stated at one of the two stations, between sections, before the first
  or after the last station — it is `None`.

Nothing is computed from speed or radius, and the crate holds no table of design rates, maximum
rates or runoff lengths. No normal crown is assumed. If a value is not in the data it is absent.

## Station equations

`StationEquations` is a list of `StationEquation { internal_m, back_m, ahead_m, direction }`. At
internal station `internal_m` the displayed station stops being `back` and becomes `ahead`, then
runs on metre for metre (down, for `Decreasing`). `back_m` is optional because it follows from
what is before; when given it must agree to 1 mm. The equations cut the alignment into **regions**:
0 before the first equation, `i` after equation `i − 1`. In region 0 displayed = internal.

- `alignment.displayed_station(internal)` → `DisplayedStation { station_m, region }`. A station
  exactly on an equation gets the ahead station.
- `alignment.station_label(internal, decimals)` → `1+250.000`, through the equations.
- `alignment.internal_station(displayed, region)`:
  - with `Some(region)` it looks in that region only;
  - with `None` the station must occur exactly once. In a **gap** (ahead greater than back) the
    displayed stations between them are nowhere: `StationEquationError::NotOnAlignment`. In an
    **overlap** (ahead less than back) a displayed station occurs twice and is **ambiguous**:
    it is refused with `StationEquationError::Ambiguous { candidates }`, which lists every region
    and internal station it could mean. It is never resolved silently.
- `alignment.internal_station_candidates(displayed)` returns all of them.

Both ends of a region belong to it, so the back station of an equation is found from behind and
its ahead station from ahead. Two places in an overlap have the same label; keep the region with
a displayed station wherever that matters. The profile, the superelevation, the geometry and the
`station_m` stored in a `SitePlacement` all use internal stations, which are never ambiguous.

## LandXML

`read_landxml(path)` / `parse_landxml(text)` return `LandXmlImport { alignments, notes }`.

### Whole file

`read_landxml_file(path)` / `parse_landxml_file(text)` return a `LandXmlFile`:

| Field | What it holds (LandXML 1.2 element) |
|---|---|
| `header`, `units` | `LandXML` attributes; `Units/Metric` or `Imperial`, every attribute, with the schema defaults for the angular, direction, lat-long and elevation units, and the metres-per-unit factor that was applied |
| `coordinate_system` | `CoordinateSystem`, every attribute of the schema as an optional string (`epsg_code`, `ogc_wkt_code`, `horizontal_datum`, `vertical_datum`, `datum`, `ellipsoid_name`, the `*_coordinate_system_name`s, `file_location`, `desc`, `name`) and `rotation_angle` as a number in the file's angular unit. No CRS type is involved; the application maps it |
| `project`, `applications` | `Project`, `Application` (`LandXmlFile::is_civil_3d()` tests the text for "Civil 3D": a heuristic, not proof) |
| `alignments` | `Alignment`, as `parse_landxml`; an `IrregularLine` or `Chain` in a `CoordGeom` is now read as straight segments between its points, with a note |
| `ground_profiles` | `Profile/ProfSurf`, the `PntList2D` runs of station and elevation |
| `cross_sections` | `CrossSects/CrossSect` per station, `CrossSectSurf` (offset and elevation runs) and `DesignCrossSectSurf/CrossSectPnt` (`dataFormat="Offset Elevation"` only) |
| `cg_points` | `CgPoints` groups, nested as the schema nests them, with `CgPoint` name, code, desc, state, oID, `pointGeometry`, `DTMAttribute`, latitude, longitude, northing, easting, elevation |
| `parcels` | `Parcels/Parcel`: name, `area` and `area_m2` (by `areaUnit`), desc, state, owner, `parcelType`, class, `Center`, `CoordGeom` (lines, curves, spirals, irregular lines, chains), nested `Parcels` |
| `plan_features` | `PlanFeatures/PlanFeature` with its `CoordGeom`s |
| `surfaces` | `Surfaces/Surface`: `Definition@surfType` (`TIN` or `grid`), `Pnts/P`, `Faces/F` (three or four ids, `i="1"` kept as `invisible`), and `SourceData`: `Breaklines`, `Contours`, `Boundaries`, `DataPoints` (`PntList3D`) |
| `summary` | per class (`alignments`, `cg_points`, `parcels`, `plan_features`, `surfaces`): count and plan extent (min and max easting and northing, metres); counts of ground profiles, cross-sections and sub-objects; `overall_extent()`; `skipped` (elements not read, by path, with how many) |
| `warnings` | the alignment notes, everything dropped or assumed, and one line per kind of skipped element |

The whole-file entry point is in this crate because `afrobim-corridor` and `afrobim-design` both
depend on it, so no dependency cycle arises. It carries raw surface points and faces; the corridor
crate turns them into a `Tin` (`parse_landxml_surfaces`) and the design crate writes a
`LandXmlFile` back (`write_landxml_file`, and `surface_from_tin`).

**Coordinates are kept exactly as the file gives them.** Point text is `northing easting
[elevation]` and becomes X = east, Y = north; the only arithmetic is the multiplication by the
metres per `linearUnit` (1 for a metric file, so nothing happens to the numbers). Nothing is
shifted, normalised or rounded. `Units/@elevationUnit` is reported; if it differs from the linear
unit a warning is made and elevations are still read in the linear unit. Angles
(`rotationAngle`, the direction units) are never converted. The tests check UTM-sized values
(easting about 450 000 m; northing about 35 000 m and about 9 960 000 m) for exact equality.

Unknown elements are never an error. Every element the reader does not read (`Roadways`,
`PipeNetworks`, `Survey`, `Feature`, `Alignment/Start`, `AlignPIs`, `Cant`, `Parcel/Title`, ...) is
counted in `warnings` and `summary.skipped`; a value that cannot be read is dropped with a warning.
It errors only for a file that is not XML, whose root is not `LandXML`, or whose `linearUnit` is
not in the schema's list.

Every element and attribute name below was checked against `LandXML-1.2.xsd` (elements are
matched by local name, so the 1.0, 1.1 and 2.0 namespaces read the same way). Everything in the
file's linear unit is converted to metres.

- `Units/Metric@linearUnit` (`meter`, `millimeter`, `centimeter`, `kilometer`) and
  `Units/Imperial@linearUnit` (`foot`, `USSurveyFoot`, `inch`, `mile`).
- `Alignments/Alignment` with `@name` and `@staStart`.
- `CoordGeom/Line`: `Start`, `End`.
- `CoordGeom/Curve`: `Start`, `Center`, `End`, `@rot` (`cw` or `ccw`). The radius is Start to Center.
- `CoordGeom/Spiral` with `@spiType="clothoid"`: `Start`, `End`, `@length`, `@radiusStart`,
  `@radiusEnd` (`INF` at the tangent end), `@rot`.
- Points as text, **northing then easting** (an elevation after them is ignored), or as a `pntRef`
  to a `CgPoint`. They become X = easting, Y = northing.
- `Alignment/StaEquation`: `@staInternal`, `@staAhead`, `@staBack` (optional), `@staIncrement`
  (`increasing` or `decreasing`; increasing when absent). A `staBack` that disagrees with the
  stationing before it is noted and the derived value used.
- `Alignment/Profile/ProfAlign`, children in order, each with the text `station elevation`:
  `PVI`, `ParaCurve@length`, `UnsymParaCurve@lengthIn @lengthOut`, `CircCurve@radius`.
  `CircCurve@length` is only compared with the curve the radius gives (the schema does not say
  whether it is the arc or its plan length). Elevations are in the linear unit. Only the first
  `ProfAlign` is read; others are named in a note. `ProfSurf` (existing ground) is not read.
- `Alignment/Superelevation`, one per curve: `BeginRunoutSta`, `BeginRunoffSta`, `FullSuperSta`,
  `RunoffSta`, `StartofRunoutSta`, `EndofRunoutSta`, `FullSuperelev` (percent), `AdverseSE`.

**What a LandXML superelevation does and does not give.** The 1.2 schema carries six stations and
*one* number, the full rate. It gives no cross-fall at the other stations and no normal crown. So
each `Superelevation` becomes one section holding its stations, and:

- `FullSuperSta` and `RunoffSta` get the full rate on both sides as one plane: the inside of the
  horizontal curve falls at the rate, the outside rises at it (the schema's own description: "the
  road surface slopes toward the inside of the curve"); the other way round for
  `AdverseSE = adverse`. The inside is read from the curvature of the imported horizontal
  geometry midway between the two stations. Every assignment is recorded in a note.
- the other four stations are stored with their `kind` and **no cross-fall**. Through the runoff
  and runout, and on the tangents, the cross-fall is therefore absent, not guessed.
- if the alignment is straight there, the rate is negative (the schema defines no sign) or
  `FullSuperelev` is missing, no side gets a value and a note says why.

Assumptions the schema leaves open, each made explicit: **PVI stations are taken to be internal
stations** (a note is made when the alignment also has station equations); superelevation
stations are internal stations (the schema's `station` type says so); `Units@elevationUnit` is
not applied to PVI elevations (the schema reserves it for elevation attributes).

The direction and other redundant attributes (`dir`, `dirStart`, `dirEnd`, `delta`, `chord`,
`tangent`, `theta`, …) are not used: the geometry is taken from the points. Where `length` or
`radius` is given and disagrees with the points by more than 1 mm, a note says so. A note is also
made for every joint where the elements do not meet smoothly, and when the profile stops short of
the alignment.

An alignment whose horizontal geometry contains something unsupported is left out and a note says
why; the rest of the file is still imported. A profile, set of equations or superelevation that
cannot be read is left off its alignment with a note, and the alignment is still imported.

The test files in `tests/landxml/` were written by hand to the schema and validate against
`LandXML-1.2.xsd`: `site.xml` and `unsupported.xml` (horizontal), `corridor.xml` (profile with all
four PVI kinds, a gap and an overlap in the station equations, superelevation on left, right and
adverse curves) and `corridor_feet.xml` (feet; a profile, equation and superelevation with faults).
Beyond the one Civil 3D file above (taken from the public MIT-licensed repository RustedGeom/rusted-geom,
`showcase/public/landxml/C3DProfileExample2.xml`, unmodified), **no Civil 3D export was available to test
against**, so how the importer copes with what Civil 3D writes elsewhere (which stations it puts in a
profile when there are station equations, what it writes in `CircCurve@length`, whether it writes
`Superelevation` in this form, how it writes surfaces, parcels, points and cross-sections) is not
yet proven. The element and attribute names come from the schema file, which was fetched
(`LandXML-1.2.xsd` dated 2008-07-29 in its header) and used to validate every test file here.

## Sign placement

`resolve`, `rotation`, `normalize_rad` and `location_report::write` are a port of
`SignPlacement` and `LocationReport` in `src/SignCore.Cli/SignPlacement.cs`: which side a sign
stands on, the offset it is placed at, the traffic it faces, its rotation, the warning when
`side` overrides the offset's sign, and the location schedule CSV. `tools/GoldenPlacement` runs
the C# over 4256 cases and five reports and writes the results to `tests/golden/`;
`tests/golden.rs` compares every one (numbers within 1e-12, warning and error text and the CSV
byte for byte). To regenerate: `dotnet run --project tools/GoldenPlacement`. Nothing below changes
any of it for an alignment without a profile, superelevation or station equations.

`place(&alignment, &location, traffic_side)` takes the point and heading at the location's
station, applies `resolve`, and returns `Placement { positions, warning }` with one
`PlacedPosition { side, x_mm, y_mm, z_mm, rotation_deg, station_m, offset_m, cross_fall_pct }` per
sign (two for `side = both`, left first).

- **Level.** When the alignment has a profile, `z_mm` is the profile level at the station: the
  **centreline** level. A station outside the profile is an error (`PlacementError::Profile`), not
  a level of 0. Without a profile `z_mm` is 0, as before. `site_placement` passes it on as `z`.
- **Station.** `place` takes the location's station as an internal station.

`place_with(&alignment, &location, traffic_side, &PlaceOptions { station, cross_fall })` makes the
two choices explicit; `PlaceOptions::default()` is `place`.

- `station: StationReference::Internal` (default) or `Displayed { region }`. A displayed station
  is taken through the station equations; with `region: None` it must occur exactly once, and one
  in an overlap fails with the list of candidates (`PlacementError::Station`). A station is never
  reinterpreted silently: the same number means different places as an internal and as a displayed
  station, and the caller says which it is. `PlacedPosition::station_m` is always the internal one.
- `cross_fall: CrossFallAdjust::Off` (default), `WhereKnown` or `Required`. The adjustment adds
  `|offset| × cross-fall / 100` for the sign's side to the centreline level.
  **This is an approximation that is only valid within the carriageway**: it treats the ground as
  one plane from the centreline to the sign. Signs usually stand beyond the carriageway edge, on a
  shoulder, verge or side slope, where the ground does not follow the carriageway cross-fall, and
  there the result is not a ground level. `WhereKnown` adjusts where a cross-fall is stored for
  that side at that station and otherwise keeps the centreline level; `Required` fails instead
  (`CrossFallUnknown`, or `NoProfile`). `PlacedPosition::cross_fall_pct` records what was applied.

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

- Computing superelevation: no rate from speed and radius, no design tables, no runoff or runout
  lengths, no normal crown. Only what the data states is returned.
- Design checks on the profile (K values, sight distance, grades).
- Cross-sections, cant, existing-ground profiles (`ProfSurf`), and any ground level away from the
  centreline other than the opt-in cross-fall approximation above.
- The plan drawing and the kernel module's derived values take no account of the profile,
  superelevation or station equations: the ticks are at internal stations.
- The location report (`location_report::write`) is the C# port and prints the station it is
  given; choosing internal or displayed stations for a schedule is the caller's.
- A step in cross-fall at one station (two values at the same station in one section).
- Spirals that are not clothoids (Bloss, cubic parabola, sinusoid, …) in an alignment (parcel and
  plan-feature spirals of any type are read as data). An `IrregularLine` or `Chain` in an
  alignment becomes straight segments.
- LandXML elements the whole-file reader does not read (they are counted, never an error):
  `Roadways`, `PipeNetworks`, `Survey`, `Monuments`, `GradeModel`, `Amendment`, `FeatureDictionary`,
  `Feature`, `Alignment/AlignPIs`, `Cant`, `Surface/Watersheds`, `SurfVolumes`, `SourceData/Chain`
  and `PointFiles`, `Parcel/Title`, `Exclusions`, `LocationAddress`, `VolumeGeom`, `CrossSectPnt` in
  `Slope Distance` format.
- A `grid` surface is read as raw points and four-point faces only; converting it to a TIN is the
  corridor crate's option.
- Station labels and any other text in the plan drawing.
- Writing LandXML (it is `afrobim-design`'s `write_landxml_file`).
- `station_offset` for a point as far from a curve as its centre has no single answer and
  returns one of the candidates; a point that close to the evolute of a sharp curve may be
  matched to a neighbouring foot.
- Anything about how Civil 3D writes these elements: untested (see LandXML above).
