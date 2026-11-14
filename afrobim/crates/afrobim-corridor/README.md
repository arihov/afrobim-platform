# afrobim-corridor

A road corridor: a typical cross-section (template) carried along an alignment, cut and filled
against an existing ground surface, with cross-sections and earthwork volumes.

This crate is **geometry and measurement only**. It holds no design values: every lane width,
cross-fall, side slope, ditch size, formation depth, section interval and bulking or shrinkage
factor is entered by the engineer. Nothing is taken from a design manual, and nothing here checks
a design against one.

**Not tested against Civil 3D.** No surface, section or volume in the tests came from Civil 3D or
any other design package. The LandXML test files were written by hand to the schema. Every
expected value in the tests is worked by hand from plane geometry. Before relying on the volumes
for payment or a bill of quantities, compare a real job against the package you trust.

## What it does

| Part | Type or function |
|---|---|
| Ground surface (TIN) | `Tin`: elevation at a point, section cut along a line, bounds, boundary edges |
| LandXML surface import | `read_landxml_surfaces`, `parse_landxml_surfaces` |
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

Not done: **triangulation**. The crate does not build a surface from scattered survey points or
contours (no Delaunay). It takes triangles that already exist. Overlapping triangles are not
detected.

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

- The ground must be a TIN that already exists. No triangulation, no contours.
- Sections are straight lines square to the alignment. On tight curves with wide sections,
  neighbouring section lines can cross on the inside; nothing checks for that.
- The daylight is the first meeting of slope and ground. There are no benches, no slope
  rounding, no variable slopes by height, no ditch added in cut only.
- A side with no links daylights from the centreline.
- The cut or fill decision is made at the outer end of the template only.
- Template changes are steps. No width or slope transitions.
- Meshes and plan lines join sections with straights.
- Superelevation is whatever the alignment stores; the pivot is always the centreline.

## Not in this cut

Subassemblies beyond the link list (kerbs, medians, benches, retaining walls); widening and
intersections; pavement layer volumes (the pavement crate does layers); surface creation from
survey points or contours; material strata; haul and haul cost; slope staking reports;
prismoidal correction; an external (out-of-document) surface store.

## Tests

`cargo test -p afrobim-corridor`: 32 tests. A timing test is ignored by default:

```text
cargo test -p afrobim-corridor --release --test module -- --ignored --nocapture
```

On a synthetic 200 000-triangle surface (5 km × 1 km) with 501 sections, on a 4-core cloud
machine, release build: spatial index 15 ms, 100 000 point queries 19 ms, all sections 18 ms,
stored form 6.4 MB written in 24 ms and read in 28 ms.
