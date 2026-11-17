# afrobim-pointcloud

Point cloud support for a road BIM application, as an engine with no user interface: survey
scans, drone photogrammetry and lidar as reference data and as a source for ground surfaces.

Metric, `f64`. Coordinates are kept exactly as the file gives them after its scale and offset
(`raw * scale + offset`, true UTM magnitudes, tested at northing 9 960 000 m). Stored data is
never recentred. A local origin is used only in a `DisplayBuffer` (positions as `f32` relative to
an origin that is returned with them).

No engineering default anywhere: cell sizes, slopes, heights, window sizes, tolerances, bands and
colour limits are inputs. The parameter structs of the ground filters have no `Default`.

## Formats

| Format | Read | Write | Through | Notes |
| --- | --- | --- | --- | --- |
| LAS 1.0 to 1.4 | yes | 1.2 (formats 0 to 3), 1.4 (formats 6 and 7) | `las` 0.11.2 (MIT) | tested: 1.2 formats 0, 1, 3 and 1.4 format 7, plus one public sample file |
| LAZ (LAS 1.0 to 1.4) | yes | yes | `las` with feature `laz`, which uses `laz` 0.13.0 (Apache-2.0, pure Rust port of LASzip) | tested: LAZ 1.2 and 1.4 written and read back, plus one public sample file |
| E57 (ASTM E2807) | yes | no (tests write with the `e57` crate) | `e57` 0.11.13 (MIT, pure Rust, no unsafe) | Cartesian points, intensity, colour, scan pose, `coordinateMetadata` text |
| Delimited text | yes | yes (XYZ and attribute columns) | this crate | XYZ, XYZ + intensity, XYZRGB, PTS, any column order, header line, comments |

All dependencies are pure Rust, permissively licensed, and build from crates.io with no C, C++ or
system library, so the crate builds unchanged on Windows and macOS (checked with
`cargo metadata`: the whole resolved tree is MIT, Apache-2.0, ISC, Zlib, Unlicense or
combinations; none has a `links` key). `chrono` (pulled in by `las`) has platform time zone
helper crates on Windows and macOS that were not built here.

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| `las` | 0.11.2 | MIT | LAS and LAZ reading and writing, VLRs, GeoTIFF keys |
| `laz` | 0.13.0 (through `las`) | Apache-2.0 | LAZ compression |
| `e57` | 0.11.13 | MIT | E57 |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | SHA-256 of the source for the cache check |
| `afrobim-corridor`, `afrobim-terrain` | workspace | | `Tin`, `SurfaceDefinition`, `simplify_points` |

### Header facts without loading points

`read_header(path, &SourceOptions) -> HeaderInfo`: kind, version, point format, point count,
bounds, scale, offset, which attributes exist, compression, generating software, and the
coordinate system as plain data (`CrsInfo`): the WKT text, an EPSG code and where it came from,
the vertical EPSG code, and the raw GeoTIFF keys. No CRS type, no transformation.

* LAS and LAZ: WKT from the WKT coordinate system VLR (LAS 1.4); GeoTIFF keys from the
  GeoKeyDirectory; EPSG from the WKT root authority (`AUTHORITY["EPSG","n"]` or `ID["EPSG",n]`,
  a text scan) else from ProjectedCRSGeoKey (3072) or GeodeticCRSGeoKey (2048) when the value is in
  1024 to 32766 (the GeoTIFF range for EPSG codes). Not checked against any registry.
* E57: the `coordinateMetadata` text is returned as `wkt` whatever it contains (the standard does
  not fix its form); an EPSG code is found only when it is WKT with an EPSG authority.
* LAS point counts and bounds are what the header states; writers can be wrong. `bounds_exact` is
  `false` for LAS and E57 and `true` for text (the facts of a text file come from one pass).
* Text files have no header; `read_header` reads the whole file once.

### Class codes

`classes.rs` carries the meanings from *LAS Specification 1.4* (OGC Community Standard 17-030r1,
ASPRS document version 1.4-R13, published 2018-03-01), fetched from
`https://docs.ogc.org/cs/17-030r1/17-030r1.pdf`: **Table 9** (point data record formats 0 to 5)
and **Table 17** (formats 6 to 10). They differ (for example class 8 is "Model Key-point (mass
point)" in Table 9 and "Reserved" in Table 17, and classes 10, 11 and 13 to 18 exist only in
Table 17), so `HeaderInfo.class_table` says which applies. For text and E57 no table applies and
classes show as numbers only. Class 2 is "Ground" in both tables.

### Text files

`detect_text_format(path, preview_rows)` reads the first 80 lines and proposes a delimiter (tab,
semicolon, comma or white space, the first that gives the same column count of three or more on
every line), a header line, a layout (`Xyz`, `XyzIntensity`, `XyzRgb`, `Pts`, `XyzIntensityRgb`,
`Named` from header names, `Custom`), a column mapping, and a preview of the first rows with notes
about each guess. Six columns of fractions within -1 and 1 are taken as normals and ignored. It is
a proposal: show the preview, let the user change `TextFormat`, then read.

* Intensity is stored as `round(value * intensity_scale + intensity_offset)` clamped to 0..65535,
  both inputs (PTS intensity runs from -2048 to 2047 in many files; others use 0..255 or 0..1).
* Colour is stored as `value / rgb_max * 65535`; `rgb_max` is an input, guessed as 255 or 65535
  from the preview rows and said so in the notes.
* PTS: the first line (the point count) is skipped, then `x y z intensity r g b`.
* Limits: decimal point only (no decimal comma), one point per line, `#` and `//` comment lines.

## Reading in chunks

`open_source(path, &SourceOptions) -> Box<dyn PointSource>`; `read_chunk(max, &mut PointBuffer)`
appends up to `max` points and returns the count (0 at the end). Memory is bounded by the chunk.
Statistics (`DensityGrid`, `ClassHistogram`, `Histogram`) and `export_stream` work chunk by chunk,
so they run on files larger than memory. E57 is read on a helper thread that owns the reader
(the `e57` point iterator borrows it) and sends chunks of 65 536 points. `read_all` loads a whole
(small) file.

E57 notes: points with invalid Cartesian state are skipped; spherical coordinates are converted by
the `e57` crate; the scan pose is applied by default (`SourceOptions::e57_apply_pose`); intensity and
colour are normalised by the `e57` crate to 0..1 using the file's limits (or the type range) and
stored as 16-bit fractions; return numbers and classes are not read from E57 (stored as 0); header
bounds with a rotating pose are a conservative box of the rotated corners.

## Index and level of detail

An **implicit octree on Morton-ordered points** (`PointIndex`). Each point gets a 63-bit Morton key
(21 bits per axis, inside the cube that encloses the data), points are stored sorted by key, and an
octree node at depth `d` is a contiguous range found by binary search. Reasons: the tree costs only
the 8-byte key per point; the build is one sort; it adapts to the uneven density of a long thin
corridor scan (a grid pyramid would waste cells); level of detail is free (a node at depth `d`
stands for its points). Memory per point in the finished index: **36 bytes** (24 position, 2
intensity, 1 class, 1 returns, 8 key), plus 6 with colour and 8 with GPS time (tested: exactly
`36 * n`). The peak during the build is higher, measured at about 61 bytes per point.

Queries (all return indices into `index.points()`, which is in Morton order, not file order):

| Call | Returns |
| --- | --- |
| `query_box(&Bounds3)` | points in a box (`Bounds3::plan` for a plan box) |
| `query_polygon(ring, z_range)` | points in a plan polygon, optional elevation range |
| `query_polyline(line, distance)` | points within a plan distance of a polyline (corridor swath), each once |
| `lod(&view, max_points)` | at most `max_points` points spread evenly over a view box, for drawing |
| `nearest(p, k, plan_only)` | the `k` nearest points with distances, 3D or plan |
| `display_buffer(indices, colours)` | `f32` positions relative to a returned `f64` origin, plus colours |

`lod` returns every point when the view holds `max_points` or fewer. Otherwise it cuts the octree at
the shallowest depth where the nodes in view outnumber the budget, takes each node's first point
inside the view, and thins that list evenly to the budget (result between half the budget and the
budget; tested to be inside the view, unique and spread over all four quarters). All queries are
tested against brute force on 20 000 to 60 000 points at true UTM magnitudes.

### Cache beside the source

`open_indexed(path, &SourceOptions, &IndexOptions)` writes `<source file name>.afrocloud` next to
the source: a JSON block (format version, SHA-256 of the source, its size, the options, the header
facts) and the columns in index order. On open the source is hashed; if hash, size and options all
match the columns are read back with no sorting (`CacheStatus::Hit`); otherwise the index is
rebuilt and the cache replaced through a temporary file and a rename (`Built` or `Rebuilt` with the
reason). A damaged or truncated cache is rebuilt, never trusted. A cache that cannot be written is
reported in `cache_write_error`. The cache is about the size of the index in memory (36 bytes per
point plus attributes, 360 MB for 10 million points).

### Larger than memory

The spatial index is in memory. For a file larger than memory: stream it (`PointSource`) for
statistics, filtering and export, or build an index from a systematic sample with
`IndexOptions { max_points: Some(n), overflow: Overflow::KeepEveryNth }` (every k-th point in file
order, `thinning_stride()` reports k). There is **no on-disk spatial index**: queries on the full
data of a file larger than memory are not supported.

## Measured performance

Machine: 4 cores, 16 GB RAM, Linux, files in the page cache (warm). Synthetic cloud of 10 000 000
points over 2 km x 200 m at northing 9 960 000 (`examples/bench.rs`; each mode its own process; peak
is VmHWM of the process). Single-threaded.

| Step (10 million points) | Release | Debug (unoptimised) |
| --- | --- | --- |
| Build index from points in memory | 1.37 s, peak 614 MB | 9.0 s, peak 615 MB |
| Index held | 360 MB (36 bytes per point) | same |
| Box query, 200 m x 100 m, 500 803 points | 2.0 ms | 8.2 ms |
| `lod` 100 000 of that view | 51 ms | 555 ms |
| `lod` 100 000 of the whole cloud | 121 ms | 309 ms |
| Corridor swath 2 km x 10 m, 501 804 points | 25 ms | 334 ms |
| 1000 x 8-nearest | 15 us each | 128 us each |
| Write LAS (200 MB) | 1.75 s | 11.5 s |
| SHA-256 of the 200 MB LAS | 0.17 s | 3.7 s |
| First open of the LAS (hash, read, build, write cache) | 2.08 s, peak 625 MB | 15.3 s |
| Reopen with cache hit | 0.39 s | 5.7 s |
| Write LAZ (86 MB) / first open / cache hit | 3.4 s / 5.0 s / 0.28 s | not measured |

Reading is fast because the data was in the page cache; a cold disk or network share will be
slower. A real scan is less uniform than this synthetic one. Run `cargo run --release -p
afrobim-pointcloud --example bench -- build 10000000` to repeat; the 10 million point test
`ten_million_points_index` is `#[ignore]`d.

## Attributes, filters, colours, statistics

* Columns: class, intensity, return number and number of returns, RGB (16-bit), GPS time where the
  source has them. Not kept: scan angle, user data, point source id, NIR, waveform, extra bytes, the
  withheld, key-point, synthetic and overlap flags.
* `PointFilter` (serde): class set and excluded classes, elevation range, intensity range, return
  selection (first, last, only, first of many, last of many, intermediate, number). Points with an
  unknown number of returns (0) match only `Any` and `Number`. No limit has a default.
* `colorize(buf, indices, &ColorMode)` returns `[u8; 3]` colours as data: by elevation (min, max,
  ramp), intensity (min, max, ramp), class (palette), RGB (`Bits16`, `Bits8` or `Auto`, a
  heuristic that treats colour as 8-bit when no value exceeds 255), or fixed. The ramps and the
  class palette are cosmetic choices of this crate.
* `DensityGrid` (points per square metre on a grid, summary of mean, maximum and empty cells),
  `ClassHistogram`, `Histogram` (elevation or any value): all accumulate chunk by chunk.

## Ground and surface

**(a) Existing classification**: `ground_from_classes(buf, indices, &[2])`.

**(b) Ground filters for unclassified clouds** (`ground.rs`, every parameter an input):

* `progressive_morphological_filter`: Zhang, K., Chen, S.-C., Whitman, D., Shyu, M.-L., Yan, J. and
  Zhang, C. (2003), "A progressive morphological filter for removing nonground measurements from
  airborne LIDAR data", *IEEE Transactions on Geoscience and Remote Sensing* 41(4), 872-882.
  Implemented **from the open description in the PDAL documentation of `filters.pmf`**
  (`https://pdal.io/en/stable/stages/filters.pmf.html`, which cites the paper). The paper itself was
  not read, so the details are this crate's reading: a minimum-elevation grid (empty cells take the
  nearest filled cell by breadth-first fill); for each iteration the surface is opened (erosion then
  dilation, square window) with that iteration's window; a point more than that iteration's height
  threshold above the opened surface in its cell is nonground; the opened surface feeds the next
  iteration. The comparison is per point. `PmfParams::from_series` takes thresholds from the PDAL
  prose (slope x cell size x window change + initial distance, capped; the first iteration uses the
  initial distance, which is this crate's reading). Window series helpers `linear_windows` and
  `exponential_windows` take explicit numbers; no series is claimed to be the paper's.
* `grid_minimum_slope_filter`: **this crate's own method, not from a paper.** Grid minimum; a cell
  is kept when no occupied neighbour within `search_cells` is lower by more than `max_slope` times
  the distance; a point is ground if its cell is kept and it is within `tolerance` of the cell
  minimum. Limits: a low outlier spoils its cell and its neighbours (remove noise classes first);
  a flat roof wider than the search window passes as ground.

Both were checked on constructed scenes (slope, a building, a tree line) and not against any
benchmark data set. Judge them on your data.

**(c) Thinning**: `thin_to_grid(buf, indices, cell, CellStat::{Min, Mean, Median})` (min and median
return real points; mean averages offsets from the first point of the cell, so no precision is lost
at 7-digit northings); `decimate_to_tolerance(points, tol)` wraps
`afrobim_terrain::simplify_points` (greedy insertion into a Delaunay triangulation; the vertical
tolerance holds at the dropped input points, not between them; slow for very many points, so thin
to a grid first; points with identical x and y are reduced to the first). Output as
`surface_points`, `surface_operation` (an `afrobim-terrain` `AddPoints` operation) and
`tin_from_points` (a `Tin`). `ground_surface(buf, ground, &SurfaceSpec)` runs grid, decimation and
TIN and returns counts at each stage and a `FitReport`: point minus surface for **all** the input
ground points (mean, RMS, maximum absolute, minimum, maximum, and how many fall outside the
surface). On the test scene (39 513 ground points, 2 m lowest-point grid, 0.02 m tolerance): 2 475
points, then 461 points, RMS 5.4 mm, maximum 20.2 mm, 1 191 points outside the hull (the surface
from cell minima does not reach the outer ground points).

**(d) Sections**: `section_slice(index, line, thickness, filter)`: points within `thickness / 2` in
plan of a section polyline, as offset along the line, elevation, signed lateral distance and class,
sorted by offset. The line is not extended; points beyond the ends within the half thickness are
included with offset 0 or the line length.

**(e) Cloud against surface**: `compare_to_tin(buf, indices, &Tin, hist_min, bin_width, bins,
&DifferenceBands)`: point z minus surface z per point (`NaN` and band 255 outside the surface),
statistics, histogram, band counts, and `colors(&BandColors)`. Volumes are left to the terrain
crate.

## Clip, crop and export

`export_las` (a subset of a buffer or of index query results), `export_stream` (a source through
a `PointFilter` and a `ClipRegion` box or polygon, chunk by chunk), `export_text`. LAS 1.2 or 1.4,
LAS or LAZ. With `ExportOptions::like_source` the scale and offset are the source's and every
coordinate is written back exactly (tested, rounding error 0). Otherwise the scale is an input (no
default) and the report gives the largest rounding error; the storage offset, if not given, is the
floored minimum of the data (a storage detail that readers add back). Coordinate system records
can be copied from a source LAS (`copy_crs_from`), or WKT written for LAS 1.4. Notes in the
report list what could not be written as it was (return numbers above 7 and classes above 31 in
LAS 1.2).

## Serde and the document reference

`PointCloudReference` (source path, SHA-256, `HeaderInfo`, `CrsInfo`, text format, `PointFilter`,
`DisplaySettings`) is plain serde data with no points; `from_file`, `check_source` (unchanged,
changed, missing), `open` (hash checked, cache used), `to_json`, `from_json`. `IndexOptions`,
`PointFilter`, `TextFormat`, `PmfParams`, `GridMinSlopeParams`, `SurfaceSpec`, `DifferenceBands`,
`BandColors`, `ColorMode`, `ExportOptions` and the result structs derive serde. Infinite values are
not valid JSON: use large finite numbers in stored `ClipRegion` boxes.

## Sources fetched and read

* LAS Specification 1.4, OGC CS 17-030r1 (ASPRS 1.4-R13): classification Tables 9 and 17.
* LAZ 1.4 Community Standard, OGC 24-070r1 (2026-06-30), `https://docs.ogc.org/cs/24-070r1/24-070r1.pdf`:
  opened for reference; LAZ is implemented by the `laz` crate, not by this crate.
* PDAL `filters.pmf` documentation (above).
* The E57 standard (ASTM E2807) was **not** fetched; E57 is handled entirely by the `e57` crate.
* Sample files, committed in `tests/data`: `autzen.las` (4 962 bytes) and `32-1-472-150-76.laz`
  (32 364 bytes), copied from the test data in the `las` 0.11.2 crate package (MIT licensed). The
  licence of the point data itself was not separately verified; they are small, and can be
  deleted without affecting any other test.

## Limits

* In-memory index only; no out-of-core queries (see above). At most 4 294 967 295 points.
* LAS 1.0 to 1.4 are read as the `las` crate reads them (all point formats 0 to 10); only formats
  0 to 3 (version 1.2) and 6, 7 (version 1.4) are written. LAS flags, scan angle, NIR, waveforms and
  extra bytes are not carried. COPC files read as plain LAZ.
* Index order is Morton order; file order is not kept, so indices cannot be mapped back to a file
  position.
* The WKT EPSG finder is a text scan; a WKT with the code only in a nested node is not matched
  except for a compound CRS (first node one level below the root). No CRS is interpreted or
  transformed.
* Text: decimal point only; the E57 reader keeps neither class nor returns.
* The ground filters are small implementations tested on built scenes only. The tolerance
  decimation is slow for very many points. Surfaces from cell minima do not cover the outer cells.
* Build peak memory is about 61 bytes per point (10 million points: 614 MB), against 36 held.
* `lod` takes the first point of each node in Morton order, not the point nearest its centre.
* Colour ramps and the class palette are cosmetic, not a standard.
