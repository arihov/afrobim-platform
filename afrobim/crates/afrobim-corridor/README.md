# afrobim-corridor

A road corridor: a typical cross-section (template) carried along an alignment, cut and filled
against an existing ground surface, with cross-sections and earthwork volumes.

This crate is **geometry and measurement only**. It holds no design values: every lane width,
cross-fall, side slope, ditch size, formation depth, section interval and bulking or shrinkage
factor is entered by the engineer. Nothing is taken from a design manual, and nothing here checks
a design against one.

**Not tested against Civil 3D.** No surface, section or volume in the tests came from Civil 3D or
any other design package. The LandXML test files were written by hand to the schema, and the
survey point file of the tests (`tests/survey/points.csv`) is made up. Every
expected value in the tests is worked by hand from plane geometry. Before relying on the volumes
for payment or a bill of quantities, compare a real job against the package you trust.

## What it does

| Part | Type or function |
|---|---|
| Ground surface (TIN) | `Tin`: elevation at a point, section cut along a line, bounds, boundary edges |
| LandXML surface import | `read_landxml_surfaces`, `parse_landxml_surfaces` |
| Survey point file to surface | `read_survey_points`, `parse_survey_points`, `build_survey_surface`, `import_survey_surface` |
| Coordinate system of a surface | `CoordinateSystem` (WGS 84 / UTM zones), `surface_crs` |
| Template | `Template`, `TemplateSide`, `Link`, `Slope`, `Daylight`, `TemplateRegion`, `CorridorDefinition` |
| Sections | `Corridor::build`, `section_at` → `Section` |
| Areas | `cut_fill_areas`, `polygon_area` |
| Volumes | `earthworks` → `Earthworks`, `earthworks_csv` |
| Drawings and 3D | `section_drawing`, `corridor_meshes`, `plan_lines`, `surface_display_mesh` |
| Kernel elements | `SurfaceModule` (category `surface`), `CorridorModule` (category `corridor`), `add_surface`, `import_landxml_surfaces`, `add_corridor`, `refresh_corridor`, `set_corridor_definition`, `corridor_of`, `build_corridor`, `surface_of`, `surfaces`, `corridor_element` |

## Conventions

- **Units**: metres; plan X = easting, Y = northing; elevations in metres. Drawings and meshes
  handed to the kernel are in millimetres.
- **Stations** are the alignment's internal (continuous) stations. Labels (`Section::label`, the
  CSV) are displayed stations through the alignment's station equations.
- **Offset**: square to the alignment, **positive to the right** looking up-station, negative to
  the left. A section is drawn looking up-station, and every line in it is listed left to right.
- **Slope**: **positive rises away from the centreline**, on either side, the same sense as the
  cross-fall stored on an alignment. A crowned lane is negative on both sides.
  `Slope::Percent(p)` rises `p/100` m per metre; `Slope::Ratio { run, rise }` rises `rise/run`.
- **Side slopes** (`Daylight`) are entered as horizontal distance per unit of height, always
  positive: `2.0` is 1 vertical in 2 horizontal. The direction (up for cut, down for fill) is
  decided at each section.
- **Cut and fill**: cut is where the ground is **above** the earthworks line, fill where it is
  **below**. Both are reported as positive areas and volumes.
- **Mass ordinate**: cut counts positive, fill negative.

## The ground surface

`Tin` is points `[x, y, z]` and triangles of point indices. The surface is the union of the
triangles, each a plane; a point in no triangle has no elevation.

- **Elevation at a point**: barycentric interpolation in the triangle that holds the point,
  `z = w₀z₀ + w₁z₁ + w₂z₂` with `wᵢ` the ratio of the sub-triangle's plan area to the triangle's.
- **Section cut**: the line is clipped by each triangle it crosses (the three edges as
  half-planes), which is the same as intersecting it with the triangle edges. Each triangle
  gives one straight piece; the pieces are joined in order into **runs**. One run where the line
  stays on the surface, several where it crosses a hole or leaves and re-enters.
- **Spatial index**: a uniform grid over the plan bounds with about one cell per triangle, built
  on first use and not stored. A point query looks in one cell; a section cut visits only the
  cells under the line.
- **From code**: `Tin::new(points, triangles)` or `Tin::from_grid(...)`. Indices must be in
  range, coordinates finite and every triangle must have plan area.

- **From a file of survey points**: `read_survey_points` then `build_survey_surface`, or
  `import_survey_surface` for both and the element. See "Survey points" below.

Overlapping triangles are not detected. Contours are not read.

### Survey points

A text file, one point a line: an optional point number, northing and easting in the order the
caller states (`ColumnOrder`: `PNEZD`, `PENZD`, `NEZD`, `ENZD`), a level and an optional
description. The delimiter is the comma, semicolon or tab the first lines use, otherwise blanks.
A first line whose coordinates are not numbers is a heading; any other unreadable line is left
out and reported by line number. **The column order is never guessed.**

`build_survey_surface` then:

1. leaves out the points whose description is on `SurfaceOptions::exclude` (without regard to
   case; a trailing `*` matches any ending). The list is empty unless the engineer fills it;
2. merges points at one plan position, to the millimetre, keeping the first read, and reports how
   many and the largest difference in level among them;
3. triangulates by Delaunay's rule with the [`delaunator`](https://crates.io/crates/delaunator)
   crate (ISC licence; exact predicates from `robust`), on coordinates measured from the
   south-west corner;
4. removes triangles with a side longer than `SurfaceOptions::max_side_m`, when one is given;
5. drops the points no triangle uses.

Not done: **breaklines**. A triangle side can cross a kerb, a channel or a top of bank.

Tests: the reader's cases; the triangle count Euler's formula requires and the levels of a plane on
a grid; Delaunay's rule checked the slow way (no point inside the circle of any triangle) on 400
points; exclusions, merging and the side limit; and the errors.

### Coordinate system

`CoordinateSystem` is an EPSG code and a name, kept on the surface element (`crs`). Only the
WGS 84 / UTM zones are known (EPSG:32601 to 32660 and 32701 to 32760). **Nothing is
reprojected**: the system records what the stored coordinates mean. `to_geographic` gives the
latitude and longitude of a coordinate (Krüger series after Karney 2011, third order in the third
flattening), which the import uses to say where the survey is, and `doubts` says when
coordinates cannot be in the system: eastings outside 100 000 to 900 000 m, northings outside 0
to 10 000 000 m, or an extent more than three degrees from the zone's central meridian.
`import_survey_surface` refuses a file with such doubts.

The inverse projection is tested on worked values only: the zone's origin, one degree along the
central meridian (meridian arc 110 574.4 m, to a decimetre) and one degree along the equator. It
was not compared with PROJ or another package.

### LandXML

Reads `LandXML/Surfaces/Surface/Definition`. These names were checked against
`LandXML-1.2.xsd` (<http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd>):

| Name | Use |
|---|---|
| `Units/Metric@linearUnit`, `Units/Imperial@linearUnit` | converts northing, easting **and** elevation to metres (`meter`, `millimeter`, `centimeter`, `kilometer`, `foot`, `USSurveyFoot`, `inch`, `mile`) |
| `Surfaces/Surface@name` | the surface name |
| `Definition@surfType` | must be `TIN`; `grid` (the other value the schema allows) is left out with a note |
| `Pnts/P@id` and its text | the text is `northing easting elevation`, **northing first** (the schema's words). X = easting, Y = northing |
| `P@pntRef` | a `P` with no text takes the coordinates of the `CgPoint` of that name |
| `Faces/F` text | three `P` ids |
| `F@i` | `1` means "part of the triangulation but invisible" (schema). Such faces are **skipped** |

A `Definition` may hold several `Faces` groups; all are read. Not read: `F@n` (neighbours),
`F@b` (breakline sides), `SourceData`, `Watersheds`, `SurfVolumes`. `Units@elevationUnit` is not
applied (by the schema it is for elevation attributes; the alignment reader does the same).
Faces without exactly three ids, naming a missing point, or with no plan area are dropped and
counted in the import notes.

`tests/landxml/surface.xml` and `surface_feet.xml` validate against the schema
(`xmllint --noout --schema LandXML-1.2.xsd`). They were written by hand.

### Storage in a document

A surface element stores its TIN **inline** in the document JSON, as base64 of little-endian
binary (`TinEncoded`): points as `f64` x y z (24 bytes), triangles as `u32` a b c (12 bytes).
Exact, nothing rounded. That is **32 characters per point and 16 per triangle**, not compressed.
A survey of 100 000 points and 200 000 triangles adds about **6.4 MB** to the file. There is no
option yet to keep the surface in a separate file.

## The template

Per side, an ordered list of `Link`s from the centreline outward, then a `Daylight` rule.

- A **link** has a `code` (free text: `lane`, `shoulder`, `ditch_foreslope` …), a horizontal
  `width_m` greater than zero, a `slope`, and two flags.
- `follows_superelevation`: the link takes the cross-fall the alignment stores for its side at
  that station (`Superelevation::cross_fall_at`), **where one is stored**. Where none is stored it
  keeps its entered slope. No rate is ever computed.
- `paved`: the formation lies `formation_depth_m` below this link.

Going outward from the centreline level `z₀` (the profile at the station):

```text
offsetᵢ = offsetᵢ₋₁ ± widthᵢ          (+ right, − left)
levelᵢ  = levelᵢ₋₁ + slopeᵢ × widthᵢ
```

**Formation and the earthworks line.** Earthworks are measured to the *earthworks line*: the
finished surface, lowered vertically by the formation depth under paved links, with a vertical
step wherever a paved link meets an unpaved one, the centreline or the side slope. So the area
between finished surface and formation is exactly `depth × paved width` (a vertical-sided box).
The side slopes start from the **finished** outer edge. Sub-base carried out to the side slope,
or a formation with its own cross-fall, is not modelled.

**Daylight.** At the outer end of the template (offset `h`, level `z_h`) the ground level `g(h)`
decides the slope: above the ground → fill, falling outward at `1/fill_run_per_rise`; below →
cut, rising outward at `1/cut_run_per_rise`; on the ground (within 1e-9 m) → no slope. The slope
line `z_h ∓ |o − h| / n` is followed outward along the ground line to the **first** point where
it meets the ground. If the ground line ends first, the section has **no daylight** on that
side: this is reported in the section's notes, and the section then has no areas.

**Regions.** A corridor has one or more `TemplateRegion`s, each from its start station to the
next. The template changes in a **step**: widths are not interpolated between regions. At a
region boundary there are two sections at the same station, one with each template, so the
volume either side uses the right template.

## Sections

`Corridor::build` puts a section at:

- every whole multiple of the interval, and the start and end;
- the start of every horizontal element (tangent points, spiral ends);
- every PVI and the start and end of every vertical curve;
- every station where the alignment stores a cross-fall;
- every template region boundary.

Stations within 1 mm of one already taken are left out. The corridor runs over the overlap of
the alignment, its profile and the entered limits. The alignment must have a profile.

Each `Section` holds the finished points, the earthworks line, the ground (over the section
width plus a quarter each side), the two daylight points, the cut and fill areas and the offset
of each area's centroid.

**Areas.** The width between the daylight points is cut into strips at every vertex of the
earthworks line and of the ground, and again wherever the two cross inside a strip. With
`d = ground − earthworks` at the two ends of a piece of width `w`:

```text
area   = w (d₀ + d₁) / 2                       cut if positive, fill if negative
moment = w (a (2d₀ + d₁) + b (d₀ + 2d₁)) / 6   about offset 0, piece from a to b
centroid offset = Σ moment / Σ area
```

`w (d₀ + d₁) / 2` is the shoelace formula `½ |Σ (xᵢ yᵢ₊₁ − xᵢ₊₁ yᵢ)|` applied to the piece's four
corners. Because a new piece starts at every crossing, the lines may cross any number of times.
`polygon_area` is the plain shoelace formula; the tests check the two against each other.

## Volumes

Between consecutive sections, for cut and fill separately (**average end area**):

```text
V = L (A₁ + A₂) / 2          L = difference of internal stations
```

Cumulative cut and fill are running sums. The **mass ordinate** is the running sum of

```text
cut × cut_factor − fill × fill_factor
```

Both factors are entered and default to **1.0, which applies nothing**. No bulking or shrinkage
is assumed. Cumulative cut and fill are never factored.

**Curvature correction** (optional, off by default): each end area is multiplied by `1 + κe`,
with `κ` the horizontal curvature on that side of the interval (1/m, positive turning left) and
`e` the centroid offset of that area (positive right). This is Pappus's theorem. It is exact for
a constant section on a circular arc (tested: 2030 m³ against 2000 m³ uncorrected) and the usual
approximation otherwise. **With it off, volumes on curves are those of the centreline length.**

**No prismoidal correction** is offered.

Where a section has no areas, the volumes either side of it are blank, add nothing to the
totals, and the length is reported in `Earthworks::unmeasured_length_m`.

CSV (`earthworks_csv`), three decimals, blank where not computed:

```text
Station,Cut area (m2),Fill area (m2),Cut volume (m3),Fill volume (m3),Cumulative cut (m3),Cumulative fill (m3),Mass ordinate (m3)
```

Each row's volumes are for the length from the row before.

## Assemblies, the corridor model, sample lines and quantities (round 5)

The object model of Civil 3D, tuned to BIM, **beside** the template engine above. Old documents
and `Corridor::build` are unchanged; an old `Template` maps to an assembly with
`Assembly::from_template`, and an assembly that is a plain template maps back with
`Assembly::to_template` (anything else is refused, naming the part).

**No built-in values.** Every width, slope, depth, run, height, spacing and factor is a
parameter. Defaults exist only for structural zeros (baseline offset and elevation 0, no
benching, no layers). The library constructors in `library` take all dimensions as arguments.

### Subassemblies and assemblies

A `Subassembly` has a name, a `SubKind` with typed parameters, a `Classification` and an optional
`code_map` that renames output codes. Parts are described going outward from their attachment
point (`u` outward, `v` up), so the same part serves both sides. Output: points with point codes,
links with link codes, shapes with shape codes, and the datum line the earthworks are measured to.

| Part (`library::`) | Parameters | Codes out |
|---|---|---|
| `lane`, `shoulder`, `footway` | width (fixed or to an offset target), slope, layers (shape code, depth) | point `ETW`, `ESh` or `EFw`; links `Top`, `Pave`, `Datum`; shapes are the layer codes |
| `verge` | width, slope | point `EVg`; link `Top` |
| `kerb` | height, top width, bed depth | `Face`, `Top`, `Datum`; shape `Kerb`; the next part attaches at the top of the back |
| `kerb_and_channel` | channel width and slope, kerb as above | as kerb, plus `Channel` |
| `median_raised` (half) | height, half width, bed depth | as kerb, plus `Back`; the next part attaches at the foot of the back |
| `median_depressed` (half) | half bottom width, depth, side run | `Median_Bottom`, `Median_Slope`; the baseline point is the invert |
| `side_ditch_v`, `side_ditch_trapezoid` | depth, (bottom width), inner and outer run | `Top`, `Ditch_In`, `Ditch_Bottom`, `Ditch_Out` |
| `lined_drain` | ditch, lining thickness (vertical) | as ditch, plus `Datum`; shape `Lining` |
| `retaining_edge` | thickness, depth below | `Wall_Top`, `Datum`; shape `Wall` |
| `link_width_slope`, `link_to_elevation`, `link_to_offset` | your own link and point codes; width and slope, or an elevation target, or an offset target | as entered |
| `daylight`, `daylight_benched` | cut and fill run per rise; optional bench (step height, width, slope) in cut and in fill | links `Daylight`, `Daylight_Cut` or `Daylight_Fill`; point `Daylight` |

`Assembly` is a baseline point (offset and elevation from the alignment and profile) and
`left` and `right` lists of parts from the baseline outward. `Assembly::symmetric` and
`mirror_right_to_left` copy a side. Daylight, if present, must be last on its side. A target given
as a fixed signed offset is not mirrored.

**Classification.** `Classification { ifc_entity, predefined_type, material, cost_item_code }`
on every subassembly and on every layer; a shape carries the classification of its layer, or of
its subassembly when the layer has none. Nothing is validated against a schema. **IFC names:** the
IFC 4.3 documentation site could not be fetched from the build environment (it refused the
request), so only names that `afrobim-ifc` already writes are provided as constants
(`ifc::PAVEMENT` is `IfcPavement`, `ifc::COURSE` is `IfcCourse` with `PAVEMENT`;
`Layer::as_course` sets the latter). Names such as `IfcKerb`, `IfcEarthworksCut` and
`IfcEarthworksFill` are **not verified here**: type them into the classification after checking
the standard.

### Targets and superelevation

`OffsetTarget`: `Fixed`, `Table` of (station, offset), or a plan `Polyline` (use
`polyline_from_alignment` to sample another alignment). `ElevationTarget`: `Fixed`, `Table`, or a
`Profile`. A table gives no value outside its stations (an error, not an extrapolation). A strip
width can run to an offset target (lane widening); a link can run to an offset or an elevation.
The ground ("surface target") is the corridor's ground TIN, used by `Daylight`.

A strip with `follows_superelevation` takes the alignment's stored cross-fall for its side at the
station (`Superelevation::cross_fall_at`); where the alignment has none for that side, the
baseline's `crossfall_table` (station, left and right percent) is read the same way; where neither
has one, the entered slope is kept. No rate is computed.

### Corridor model

`CorridorModel { baselines, volumes }`; `Baseline { alignment name, regions, crossfall_table }`;
`AssemblyRegion { start, end, assembly, frequency }`; regions are contiguous. `Frequency` holds
the spacing on tangents, curves and spirals (each optional, **none defaulted**; a kind with no
spacing has no regular sections), `at_geometry_points` (element starts, PVIs, vertical curve
ends), `at_superelevation_critical` (every station where cross-fall is stored) and
`extra_stations`. Regular sections stand on whole multiples of the spacing inside the elements of
that kind. A region boundary has two sections. `build_model`, `build_baseline` and
`design_section_at` give `ModelSection { section, design, region }`, where `section` is the
ordinary `Section` (so `section_drawing` and the volume code work on it) and `design` is the
`DesignGeometry`.

- `corridor_surface(built, &["Top"], boundary)`: a surface from the points of links with those
  codes. Between consecutive sections the two rows of points, in offset order, are joined in a
  triangle strip; zero-area triangles are dropped. A vertical link left out of the codes is
  bridged by a sloping face. `SurfaceBoundary::Daylight` removes triangles whose centre is
  outside the ring of daylight points (`daylight_boundary`).
- `feature_lines(built, "ETW")`: a 3D line per side through the points with a point code, broken
  where a section lacks the point.

### Sample lines and section views

`SampleLineGroup` on an alignment: `add_stations` with `SampleStations::Interval` (from the start,
every interval, and the end), `Listed`, `GeometryPoints` (element starts and the end) or
`SuperelevationStations`; `add_line`, `set_swath`, `rename_line`, `remove_line`. Each line has its
own left and right swath. `section_views(group, alignment, surfaces, corridor)` returns a
`SectionView` per line: the ground runs of each named surface cut to the swath, the profile level,
and, when a baseline is given, the `DesignView` (design points, links and shapes with offsets and
elevations, the finished surface, earthworks line, daylight and areas). The design is not cut to
the swath. A surface that does not cover the line gives no run and a note.

### Quantities

- `earthwork_table(ends, mids, options)`: average end area `V = L (A1 + A2) / 2` or, with
  `VolumeMethod::Prismoidal`, `V = L (A1 + 4 Am + A2) / 6` where `Am` is the area of a section
  built at the middle station of the interval (`baseline_earthworks` and `surface_earthworks`
  build them). The prismoidal formula is exact for areas that vary at most as a quadratic along
  the interval; the test shows `A = 2 s^2` over 10 m: 666.667 against 1000 by average end area.
  Factors and the Pappus curvature correction are those of `VolumeOptions` (see Volumes above).
- `surface_earthworks`: cut and fill between an existing and a proposed TIN over the swaths of
  the sample lines (cut where the existing surface is above the proposed one).
- `material_volumes`: volume per shape code between consecutive sections or views by average end
  area on the centreline length, with a cumulative column and totals carrying each code's
  classification and cost item code. No curvature correction and no prismoidal option here.
- `mass_haul(stations, ordinates, free_haul_m)`: for a piecewise linear mass curve, the slice of
  volume `dc` at ordinate `c` is hauled the width `L(c)` of the loop of the curve at that level
  (the balance line). Crossings of a level are paired from the start (first with second ...);
  the last unpaired crossing is the net volume leaving or entering. Because station is linear in
  `c` between vertex ordinates the integrals are exact: total moment `sum dc (L0 + L1) / 2` and
  overhaul moment `sum dc * integral of max(L - F, 0)`. Outputs: balanced volume, free-haul and
  overhaul volumes, both moments, residual. The pairing from the start is a convention: a curve
  that could be balanced either way is balanced the forward way. `F` is the entered free haul
  distance.

### Limits of this part

- Everything is straight links in section; no curved or variable-width transitions except by
  targets, and a target is evaluated at each section only (between sections the surface is the
  straight strip).
- The earthworks line is the datum chain of the parts with the side slope; a part with no layers
  has its surface as datum. A datum with a vertical step (kerb bed, layer depth change) is
  followed exactly; there is no sub-base carried out to the side slope.
- Daylight benches are in the side slope only (cut and fill each their own).
- A kerb or median is a rectangle; no batter, no radius, no kerb type library.
- Shape polygons overlap if the parts overlap; nothing checks.
- The corridor surface bridges vertical links; it is not a closed solid, and quantities do not
  come from it.
- Not stored in a kernel document: the new objects have serde and are meant to be held by the
  caller; the document element (`CorridorElement`) still holds the old `CorridorDefinition`.
- Nothing here was compared with Civil 3D. Hand calculations are in `tests/assembly.rs` with the
  working in comments, all with arbitrary example numbers.
- No IFC export of shapes yet; the classification slots are ready for it.

## Outputs

- **`section_drawing(section, vertical_exaggeration)`**: hairlines in millimetres at full size
  (1 m = 1000 units), lower-left on the origin: ground (green), earthworks line (red), finished
  surface (black), centreline (grey). The draw crate has no text primitive, so the text comes
  beside the drawing as `SectionLabel`s: station, offset / level of every finished and daylight
  point, ground level on the centreline, cut and fill areas.
- **`corridor_meshes(corridor)`**: three `Mesh3` (top, cut slopes, fill slopes), millimetres from
  a stated plan origin with Z the elevation itself. Flat strips between consecutive sections,
  triangles counter-clockwise from above. These are open surfaces, not solids.
- **`plan_lines(corridor)`**: centreline, the two template edges, and the daylight lines (broken
  where a section has no daylight), in metres. One point per section, so chords on curves.
- **`SurfaceModule`**: plan drawing (boundary; all triangle edges up to the display cap) and one
  mesh. Above `DISPLAY_TRIANGLE_CAP` (20 000, a display limit, changeable) the mesh is a regular
  grid **resampled** from the TIN for looking at only. Calculations always use the whole TIN.
- **`CorridorModule`**: plan drawing of the stored summary and derived totals.

A kernel module sees one element, not the document, so the corridor element stores a summary
(plan lines and totals) written by `add_corridor` / `refresh_corridor` / `set_corridor_definition`.
**After the alignment or surface changes the summary is stale until `refresh_corridor` is
called.** Nothing detects that automatically.

## Limits

- The ground is a TIN from LandXML or triangulated from survey points. No breaklines, no contours.
- Sections are straight lines square to the alignment. On tight curves with wide sections,
  neighbouring section lines can cross on the inside; nothing checks for that.
- The daylight is the first meeting of slope and ground. (Benches exist in the assembly model
  only.) No slope rounding, no variable slopes by height, no ditch added in cut only.
- A side with no links daylights from the centreline.
- The cut or fill decision is made at the outer end of the template only.
- Template changes are steps. No width or slope transitions.
- Meshes and plan lines join sections with straights.
- Superelevation is whatever the alignment stores; the pivot is always the centreline.

## Not in this cut

Intersections and lane transitions beyond targets; the new assembly objects as a kernel
document element; IFC export of shapes; material strata; haul cost and the choice of borrow and
waste sites; slope staking reports; breaklines and contours in surfaces; an external
(out-of-document) surface store.

## Tests

`cargo test -p afrobim-corridor`: 68 tests (22 of them for assemblies, the model, sample lines and quantities, in `tests/assembly.rs`). A timing test is ignored by default:

```text
cargo test -p afrobim-corridor --release --test module -- --ignored --nocapture
```

On a synthetic 200 000-triangle surface (5 km × 1 km) with 501 sections, on a 4-core cloud
machine, release build: spatial index 15 ms, 100 000 point queries 19 ms, all sections 18 ms,
stored form 6.4 MB written in 24 ms and read in 28 ms.
