# afrobim-cogo

The points and parcels objects of a civil design package (COGO points, point groups, description
keys, parcels) as an engine: no user interface, no rendering. Display helpers return plain geometry
and label text.

Metric, f64. Coordinates are stored exactly as given (easting, northing, elevation in metres). Tested
at true UTM magnitude (northing near 10 000 000 m) with no loss at 0.0001 m. Bearings are whole circle
bearings in degrees clockwise from grid north; degrees-minutes-seconds and quadrant (N45-30-00E) parse
and format helpers are in `angle`.

## Scope

| Module | What it does |
|---|---|
| `point` | `CogoPoint`, `PointStore`: unique numbers, renumber, locking, duplicate detection by number and by position tolerance. No bulk transform except `apply_transform` (translate, rotate or scale) which needs an explicit list of point numbers and a reason and writes a log entry with the old coordinates. |
| `pointfile` | Delimited point files: `PNEZD PENZD NEZD ENZD PNEZ PENZ NEZ ENZ`, comma, space, tab or semicolon. `detect_format` ranks the orders with a confidence and a preview. Export. |
| `deskey` | Description keys (`TR*`, `EP $1`) to full description, point style, label style, layer and a classification slot. |
| `group` | Point groups with include and exclude rules (number range, name, raw description, elevation range), a surface-points flag, order, a built-in `_All Points` group; groups refresh when the store changes. |
| `calc` | Inverse, direct, turned angle, bearing-bearing / bearing-distance / distance-distance intersections (all solutions), perpendicular offset, station and offset to a polyline, point on line and at station. |
| `traverse` | Open, loop and known-end traverses; linear misclosure, precision ratio, Bowditch (compass) and transit adjustment; angular misclosure and equal-share angle adjustment. |
| `parcel` | Boundaries of lines and circular arcs, area, perimeter, centroid, point in parcel, deed-style course lists with closure, status and land fields. |
| `topology` | Shared boundaries, gaps and overlaps for a site. |
| `split` | Split a parcel by a right-of-way edge, offset of a polyline, land acquisition schedule, parcel area table, CSV. |

Everything derives serde `Serialize` and `Deserialize`.

## Point files

Column codes match `afrobim-corridor/src/survey.rs`: `PNEZD` is point, **northing**, easting, level,
description. Differences from that reader: `NEZ`, `ENZ`, `PNEZ`, `PENZ` are separate orders without a
description (there `NEZ` is an alias of `NEZD`); this reader keeps the point number and reads an empty
level (or the format's `missing_elevation` marker) as "no elevation" instead of skipping the line.

Units are metres only. A file in other units is rejected (`unit_factor("ft", None)` is an error)
unless the caller supplies a factor to metres.

The order cannot be known from numbers alone. `detect_format` scores each order by how many sampled
lines read, whether the first column looks like a unique whole point number, whether trailing fields
look like text, and a weak axis hint (if the second numeric column is larger than the first on at
least 90 % of lines, the order that makes it the northing gains 0.05, as is true of southern
hemisphere UTM and not in general). The confidence is `0.5 + 2.5 x margin` between the best two
scores, times the best parse fraction. These numbers are heuristics of this engine, not a standard.
Always check the preview, and set the order yourself when `ambiguous` is true.

## Methods and citations

- Plane COGO formulae (inverse, direct, intersections, offsets): standard plane coordinate geometry,
  as in Ghilani and Wolf, *Elementary Surveying: An Introduction to Geomatics*, and Schofield and
  Breach, *Engineering Surveying*. Each is derived in the source comments. No scale factor, grid
  convergence or earth curvature is applied: distances are grid distances.
- Traverse adjustment: the compass (Bowditch) rule and the transit rule as in the same textbooks;
  derivations in `traverse.rs`.
- Arc area (shoelace plus circular segments), arc centroid, point in region with arcs: derived in the
  comments of `parcel.rs` (Green's theorem and a polar integral for the segment moment).
- The acre: `ACRE_M2 = 4840 x 0.9144 x 0.9144 = 4046.8564224 m2` from the definitions 1 acre = 4840
  square yards and the international yard = 0.9144 m (1959 agreement). This is the international acre
  (not the US survey acre). It was derived from those two definitions and not read from a fetched
  document. Acres only appear as an explicitly derived display column.

### Worked example

**No published numerical example is reproduced.** Attempts to fetch open textbook examples failed
(pages refused the request or were not readable as text). The traverse tests use closed-form cases
whose answers follow from the formulae by hand: a 100 m by 50 m rectangle with a 0.02 m error in one
course (Bowditch corrections `-0.02 L_i / 300.02`, transit corrections `-0.02 |dN_i| / 100.02`), at
northing 1000 m and at northing 9 999 950 m.

## Inputs that are yours

Nothing in this crate decides what is acceptable. Tolerances, closure classes and legal requirements
are inputs: the duplicate-position tolerance, the topology snap, arc and minimum-area tolerances, the
deed closure tolerance, the precision ratio asked of a traverse (`Misclosure::meets_ratio`), the
angular tolerance (`AngleAdjustment::within`). No default values are supplied for them. The
classification slot on a description key holds a scheme and code of your choosing and is not checked
against any published table.

## Limits

- Plane grid only. No datum or projection handling, no scale factor, no least squares network
  adjustment (only the compass and transit rules), no resection.
- Description keys: first matching key in set order wins. Parameters are positional words after the
  code. Not a copy of any vendor's key file format.
- Point groups are rules, not stored lists; a point deleted from the store leaves its groups at the
  next refresh. The "surface points" flag selects points with an elevation from a flagged group; no
  surface is built here.
- Parcel boundaries must not cross themselves; `Boundary::check` reports crossings on the flattened
  ring but does not repair anything. Parcels have no holes (islands).
- A deed arc is given by radius, direction, chord (or arc length and tangent bearing). Compound and
  reverse curves are written as consecutive calls; spirals are not supported.
- Topology and split work on boundaries flattened to a chord tolerance you give. Areas of overlap, gap
  and take pieces are short by at most (2/3) x arc length x tolerance for each arc involved (the bound
  is returned). The take plus the remainder always equal the exact parcel area.
- Gaps are found only when fully enclosed by parcel boundaries. A sliver open to the outside, or a
  strip between a parcel and nothing, is not reported. Vertices closer than the snap tolerance merge,
  so a snap larger than the real misfit of the data hides gaps of that width.
- `offset_polyline` uses mitre joins with a bevel past a limit you give and refuses (error) offsets
  that reverse a segment or cross themselves, instead of trimming them. Arcs must be flattened first.
- Rotation of points is clockwise like bearings. Scale applies to plan only; elevations change only with
  a translate.
- Point file import reads whole files as text; the detection samples the first 200 data lines.
