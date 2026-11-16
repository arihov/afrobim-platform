# afrobim-kernel

The document every module and front end shares: elements, layers, selection, view, drafting aids,
the canvas controller and undo. The kernel knows nothing about signs, markings or roads.

This README covers the parts added for project-wide settings and the speed band rule.

## Project settings (document level)

A setting that belongs to the whole document, not to one element, is stored in one element of
category `project_settings` (layer `Project`), whose `data` is a JSON object with one key per
setting. This follows the pattern the sheets crate uses for `ProjectInfo` (one `project_info`
element). The kernel had no earlier store, so this is a minimal one.

```rust
use afrobim_kernel::{project_setting, set_project_setting, Document};
let mut doc = Document::default();
set_project_setting(&mut doc, "my_key", serde_json::json!(42));
assert_eq!(project_setting(&doc, "my_key"), Some(serde_json::json!(42)));
```

The element is added the first time a setting is written, other keys are kept, and it is saved
with the document. It draws nothing. A front end should register `ProjectSettingsModule` so the
element does not report "no module installed", and may hide the category from its element lists
as the app does for `project_info`.

## Speed band rule (`speed_band`)

UK requirements are stated in mph bands ("50 mph or more"). Uganda posts km/h limits, and
80 km/h is 49.71 mph. The project decides once how a limit meets a threshold; the engines are not
changed. The app calls `resolve` for each check and passes the result to the engine.

Conversion: 1 international mile = 1.609344 km exactly. That is a definition, so mph = km/h / 1.609344.

### The rule, stored as a project setting

`SpeedBandRule` is serde, stored under the key `speed_band_rule` of the project settings:

| Rule | Meaning |
|------|---------|
| `RoundUp { tolerance }` | The project default. See below. |
| `Exact` | Use the exact converted value. |
| `AskEachTime` | Return the exact value and an `Ask`; the caller asks the user (see `Resolved::answered`). A question is returned only where the default rule would have rounded up. |
| `Table(Vec<(km/h limit, mph to use)>)` | Look the km/h limit up in the user's table. An unlisted limit, or a limit given in mph, uses the exact value and is marked `Basis::TableMiss`. |

`speed_band_rule(&doc)` returns the stored rule, or `RoundUp` with `ConversionRounding` when none
is stored or the stored value cannot be read. `set_speed_band_rule(&mut doc, &rule)` stores it.

### RoundUp, exactly

Let `v` be the exact converted value in mph and `t` the smallest threshold that is greater than
or equal to `v`.

* If `v == t`, `t` is used (nothing is rounded).
* If `v < t` and `t - v` is within the tolerance, `t` is used: the requirement of the band at `t` applies.
* Otherwise the exact value `v` is used.

The tolerance is a field of the rule, `RoundUpTolerance`:

* `ConversionRounding` (the default): `t - v <= 0.5` mph. This is the case where `v` rounds to `t` at the nearest whole mph (halves round up). So 80 km/h (49.71 mph) is treated as 50, and 70 km/h (43.50 mph) is not treated as 50. It applies to a limit given in km/h only, because it is about conversion; a limit already in mph is used exactly.
* `Mph(x)`: `t - v <= x` mph, a wider band the user chooses. Applies to either unit.
* `NextBandUp`: any `v < t` uses `t`, always the next band up. Applies to either unit. A value above every threshold has no band above it and is used exactly.

The tolerance is not invented here: 0.5 mph is the definition of "rounds to the threshold at the
nearest whole mph", and anything wider is the user's choice.

### Calling it

The thresholds are never built in. The calling engine supplies the ascending mph thresholds of
its own check.

```rust
use afrobim_kernel::speed_band::{resolve, speed_band_rule, SpeedUnit};

// `doc` is the project document; `limit` is the posted limit in km/h.
let rule = speed_band_rule(&doc);
let r = resolve(80.0, SpeedUnit::Kmh, &[30.0, 40.0, 50.0, 60.0], &rule);
assert_eq!(r.mph_used, 50.0);
assert_eq!(r.band_note, "80 km/h = 49.71 mph; project rule 'round up to the next band' treats it as 50 mph");
// Pass r.mph_used to the engine; print r.band_note in the calculation.
```

`Resolved` holds `mph_used`, `exact_mph`, `band_note` (the sentence for the calculation),
`basis` (`Exact`, `RoundedUp`, `Table`, `TableMiss`, `NeedsAnswer`) and `ask`. With `AskEachTime`,
when `r.ask` is `Some`, put the choice to the user, then call `r.answered(use_band)`; the result
carries the chosen value and a note saying the user chose it.

### Limits

* The thresholds, and which checks use which, belong to the engines and are not checked here. An unsorted list is sorted.
* Only the two units km/h and mph are known.
* The table matches a km/h limit to within 0.000001 km/h.
* The rule decides how a limit meets a threshold. Whether a requirement should apply at that threshold is the standard's question, not this module's.

## Coordinate systems (`crs`)

The project coordinate system is a document-level setting, as a Civil 3D drawing setting is. It is
stored in the project settings element under the key `project_crs` (`project_crs(&doc)`,
`set_project_crs(&mut doc, &crs)`). No format version change: a file without the key reads as
"undeclared" (`None`).

**No model data is ever reprojected.** Objects keep the coordinates of their source. The system
records what those coordinates mean. Model space stays millimetres, so a UTM northing of
9 885 072.123 m is 9 885 072 123 mm; use `m_to_mm`, `mm_to_m`, `model_point_from_crs`,
`crs_from_model_point`, `elevation_m_to_mm`, `elevation_mm_to_m` instead of writing `* 1000.0`.
f64 spacing at 1e10 mm is 1.9e-6 mm, tested to 0.001 mm.

### What a `ProjectCrs` holds

Optional EPSG code, name, kind (`Projected`: maths implemented here; `NamedOnly`: a code or name,
nothing computable; `LocalGrid`: no geodetic meaning), horizontal and vertical datum names (free
text, user input, never interpreted), linear unit (metre only), optional WKT carried verbatim, and
where it came from (`Typed`, `LandXml`, `Ifc`, `Survey`). Parse helpers: `ProjectCrs::from_epsg`,
`parse` ("EPSG:32636", "32636", "WGS 84 / UTM zone 36N", "UTM 36S" read as WGS 84, "Arc 1960 / UTM
zone 36S"), and `from_wkt`, a tolerant reader that takes the name and the EPSG authority code of the
root node (not of its base system), the datum name, and keeps the text.

### Systems implemented, with their sources

All parameters below were fetched from the EPSG registry mirror epsg.io, not typed from memory.

| System | EPSG | Fetched from |
|--------|------|--------------|
| WGS 84 / UTM zones 1 to 60 N and S | 32601 to 32660, 32701 to 32760 | ellipsoid, scale 0.9996, false easting 500 000 m, false northing 0 or 10 000 000 m, from `https://epsg.io/32736.wkt`; area of use from `https://epsg.io/32636.json` and `32736.json`. Only zones 36N and 36S were fetched; the other zones apply the same 6 degree rule, so their area of use (6 degree band, equator to 84 N or 80 S to equator) is that rule, not a fetch |
| Arc 1960 / UTM zones 35S, 36S, 37S, 35N, 36N, 37N | 21035, 21036, 21037, 21095, 21096, 21097 | `https://epsg.io/<code>.wkt` (parameters, ellipsoid Clarke 1880 (RGS) a = 6 378 249.145 m, 1/f = 293.465) and `https://epsg.io/<code>.json` (area of use, method EPSG:9807) |

Projection series: Karney (2011), "Transverse Mercator with an accuracy of a few nanometers", J.
Geodesy 85, 475-485, Krüger series in the third flattening, third order, both directions. The
corridor crate's inverse was copied; the forward series is new. Tests: forward and inverse
round trip to under a millimetre over twelve East African systems; the central meridian
equals 0.9996 times the meridian arc by Simpson integration (independent of the series); one
degree of longitude on the equator matches the closed form; point scale and grid convergence match
finite differences of the forward projection.

`ProjectCrs::to_geographic`, `from_geographic`, `scale_and_convergence_at`, `area_of_use`.
Convergence is the angle from grid north to true north, counterclockwise positive.

### Dataset check

`CrsCheck::run(&project, &IncomingDataset { label, crs, extent })` returns plain-sentence findings:
same system, different system (both named, no reprojection), undeclared, extent outside the EPSG
area of use, likely swapped easting and northing, likely millimetres (values divided by 1000
fit), likely feet (values times 0.3048 fit). The unit and swap heuristics only run when the
projection is implemented; for a named-only system the result is `CannotCheck`.

### Additions (U2)

`ProjectCrs::utm_wgs84(zone, south)`, `ProjectCrs::search(query, limit)` (an EPSG code prefix or part
of a name, case and spacing ignored), `ProjectCrs::doubts(min, max)` (the UTM plausibility
sentences the survey import used to get from its own type), `clear_project_crs`. A `ProjectCrs` reads
the old corridor value `{"epsg": 32736, "name": "WGS 84 / UTM zone 36S"}` (no `kind`) as the system
the kernel knows by that code, or as named-only. `afrobim-corridor` no longer has a type of its own:
`CoordinateSystem` is this type.

### Limits, stated plainly

* **No datum shift.** Arc 1960 and WGS 84 are different datums. The epsg.io WKT lists a three
  parameter `TOWGS84` for Arc 1960, but the EPSG transformation code, its accuracy and its area
  were not fetched, so no shift is implemented. A dataset in Arc 1960 and a project in WGS 84 is
  reported as different and left alone. Geographic coordinates from these functions are on the
  datum of the system itself.
* **Named only:** EPSG:4326, EPSG:4210 and every other code or WKT. Also Arc 1960 / UTM zones other
  than the six above, and other East African systems not listed (national grids, other
  datums). A system is added only after its definition is fetched.
* **No combined (grid and elevation) scale factor.** The point scale factor is given. The
  reduction of a distance to the ellipsoid by ellipsoidal height needs a cited formula and none
  was fetched, so it is left out.
* **Heights.** The vertical datum is a label. Heights are never converted.
* **Accuracy.** Third-order Krüger series: sub-millimetre inside a UTM zone at the latitudes of
  East Africa (tested), not the nanometre accuracy of Karney's higher orders. Points more than 80
  degrees from the central meridian return `None`.
* **Scale and convergence formulae** are derived from the series derivative and verified by
  finite differences in the tests; the paper's equation numbers were not read.
* **Area of use** is the EPSG area (a country box for Arc 1960), a coarse test. Outside it is a
  prompt to look, not proof of an error.
* The unit heuristics cannot tell a US survey foot from an international foot (2 parts per million).
* Metric only: `LinearUnit` has one value.

## View: no fixed zoom limit, exact at true coordinates

`ViewTransform::MIN_SCALE` and `MAX_SCALE` are gone. The remaining guards keep `scale` finite
and inside 1e-24 to 1e24 px/mm (`GUARD_MIN_SCALE`, `GUARD_MAX_SCALE`, far beyond any view) and every
stored number finite; a zoom or pan that would break them is ignored. `zoom_to_bounds` (also on the
controller, with a margin), `CanvasController::zoom_to_selection`, and ZOOM Previous
(`remember_view`, `zoom_previous`, last 50 views; zoom window and zoom to selection record
themselves, a front end calls `remember_view` before `zoom_extents`, which takes `&self`).

### How the view holds its position

The view stores one model point, the **anchor** (`anchor_x`, `anchor_y`, millimetres, f64), and the
screen position of that point (`screen_x`, `screen_y`, pixels):

    screen = screen_anchor + (world - anchor) * scale

A screen position is computed from the difference between a model position and the anchor, never
from the model position alone, so precision does not depend on distance from (0, 0). Zooming about
the cursor makes the model point under the cursor the new anchor. Panning moves the screen position
of the anchor and, once it has wandered 1e5 px, moves the anchor instead (`RENORMALISE_PX`).
`to_screen_local(origin, local)` places element-local coordinates by `(origin - anchor) + local`, so a
mesh held as element-local numbers is placed without adding a small number to a large one.
`from_origin` and `centred_on` build views; `origin_x()` and `origin_y()` give the old pixel origin
for display only.

### The limit, stated

The only rounding is the spacing of f64 at the model coordinate: `2.2e-16 * |coordinate|`, which is
1.9e-6 mm at 1e10 mm (a UTM northing of 10 000 km) and 6e-8 mm at 5e8 mm. A point cannot be told
from its neighbour closer than that, whatever the zoom. Tested at x = 5e8 mm, y = 1e10 mm:

* zooming in about the cursor from 10 m across to past 1e5 px/mm keeps the model point under the
  cursor within 1e-3 mm, and 1 mm fills a 1000 px screen with a pixel resolved to a thousandth of a
  millimetre; a 1e-5 mm step at y = 1e10 mm is resolved to within one f64 spacing;
* zooming out until a 10 000 km extent fits works; a model coordinate round trips within a
  micrometre there;
* panning a million pixels and back changes a position by less than 1e-3 px.

Element meshes are tessellated in f32 relative to the element's lower-left corner, so a vertex is
good to about 1e-7 of the element's size (0.5 mm on a 5 km alignment). That is display only; the
element origin and the drawing coordinates the modules generate are f64, and the application's
snaps and measures use those. When a view is zoomed so far in that part of an element is beyond
4e5 px from the screen, the application clips triangles and lines to that box in f64 before
converting to f32, so what is on screen keeps its shape.

The 3D camera follows the same rule: `Camera::from_eye` forms `(p - target) + forward * distance`,
so large coordinates cancel before anything small is added; the near plane is
`distance * 1e-4`; the distance is guarded only for finite and positive.
