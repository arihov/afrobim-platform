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

Projection series: the "JHS" formulas of IOGP Publication 373-7-2 (Geomatics Guidance Note 7, part 2,
September 2019), section 3.2.3.1, which are the Krüger series in the third flattening n (Karney 2011,
J. Geodesy 85, 475-485) to fourth order, coefficients h1 to h4 and h1' to h4' as the note gives them;
the reverse latitude is found by the note's iteration on Q''. Tests: the note's British National Grid
example (forward and reverse; the note prints E 577 274.99, N 69 740.50 and recomputing them from its
own printed intermediates gives 577 274.984 and 69 740.492, which is what the code returns to
0.002 m); forward then inverse then forward over Arc 1960 / UTM 36S (worst misclosure 2e-8 m); the
central meridian equals 0.9996 times the meridian arc by Simpson integration (independent of the
series); one degree of longitude on the equator matches the closed form; point scale and grid
convergence match finite differences of the forward projection.

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

* **Datum shifts** exist only as the explicit operation described under "Reprojection" below. Nothing
  in `ProjectCrs`, `CrsCheck` or the model changes a coordinate. `CrsCheck` still only reports.
* **Geographic systems** (EPSG:4326, 4210, 4209, 4201, 4222, 4148) are first class in the transform
  pipeline and are `CrsKind::Geographic` since round 6 (`ProjectCrs::geographic(code)`,
  `ProjectCrs::geographic_systems()`; `ProjectCrs::parse("EPSG:4326")` returns one). A document that stored
  such a system as `NamedOnly` (an older version) still reads as `NamedOnly`; both take part in plans by code.
  They have no projection, so `to_geographic` and the area-of-use warnings of a projected system do not apply.
* **Named only:** every other code or WKT. National grids and other East African datums not listed
  (see "Left out" below). A system is added only after its definition is fetched.
* **Heights.** The vertical datum is a label. Heights are never converted. See "Heights" below.
* **Accuracy of the projection.** Fourth-order Krüger series with the note's Q'' iteration: round trip
  under 1e-7 m in a zone, the note's own worked examples to their printed digits. Points more than 80
  degrees from the central meridian return `None`. The distortion of a transverse Mercator far from
  its central meridian is a property of the projection, not an error of the series (GN7-2 3.2.3.1).
* **Area of use** is the EPSG area (a country box for Arc 1960), a coarse test. Outside it is a
  prompt to look, not proof of an error. Boxes overlap: Nairobi lies in the Kenya and in the Tanzania
  box of Arc 1960.
* **Convergence and scale** are derived from the series derivative and verified by finite differences
  in the tests; the paper's equation numbers were not read, and GN7-2 gives no formula for them.
  They are not defined for the south-orientated Lo systems and return `None` there.
* The unit heuristics cannot tell a US survey foot from an international foot (2 parts per million).
* Metric only: `LinearUnit` has one value.

## Reprojection (`crs_transform`, `crs_plan`)

The rule of the application stands: **nothing is reprojected silently.** A transform is something the
user asks for, sees the accuracy of, and finds in a log. `plan_transform` returns candidate plans; the
caller chooses one; `apply` changes numbers; a `TransformLog` records what was done and serialises
with serde so the document can keep it. The kernel never applies a plan on its own and reads no clock
(the caller gives `when`).

```rust
use afrobim_kernel::crs::ProjectCrs;
use afrobim_kernel::crs_plan::{plan_transform, DataExtent, TransformLog};

let from = ProjectCrs::from_epsg(21036).unwrap();   // Arc 1960 / UTM zone 36S
let to = ProjectCrs::from_epsg(32736).unwrap();     // WGS 84 / UTM zone 36S
let extent = DataExtent::new((700_000.0, 9_310_000.0), (702_000.0, 9_312_000.0));
let plans = plan_transform(&from, &to, Some(extent)).unwrap();   // every candidate, none chosen
for p in &plans { println!("{:?} {:?} m {:?}", p.transformation_codes(), p.accuracy_m, p.warnings); }
let plan = &plans[0];                                            // the user picks
let moved = plan.apply(701_000.0, 9_311_000.0, 1100.0).unwrap(); // heights come back unchanged
let batch = plan.apply_batch(&[(701_000.0, 9_311_000.0, 1100.0)]);
let log = TransformLog::from_batch("Survey points", plan, Some("2026-11-16T09:30:00Z".into()), &batch);
println!("{}", log.summary());
```

### Public API

* `crs_plan::plan_transform(&ProjectCrs, &ProjectCrs, Option<DataExtent>) -> Result<Vec<TransformPlan>, NoPath>`.
  Same system: one `PlanKind::Identity` plan. Same datum: one `PlanKind::ProjectionOnly` plan ("same
  datum, projection change only"; no datum accuracy figure applies). Different datums: one plan for
  every registry transformation (or pair of them through WGS 84) that links the datums, whether or not
  it covers the data, with a warning where the extent leaves its area of use; plans that cover the data
  first, then by stated accuracy. `NoPath` for a local grid, a named-only system, or datums with no
  recorded transformation (the sentence says nothing is invented).
* `TransformPlan`: `from`, `to`, `kind`, `steps` (`StepRecord`: inverse projection, datum
  transformation with EPSG code, parameters with units, area, revision date, URL, accuracy; forward
  projection), `accuracy_m` (sum of stated accuracies of the datum steps) and `accuracy_basis`,
  `warnings`, `notes`; `apply(first, second, height) -> TransformedPoint`, `apply_batch`, `apply_3d`,
  `reversed()`, `with_height_assumption`, `height_sensitivity`, `point_effects`, `baseline_effect`,
  `transformation_codes`.
* `crs_plan::TransformLog` (`what`, `from`, `to`, `kind`, `steps`, `accuracy_m`, `accuracy_basis`,
  `warnings`, `height_treatment`, `when`, `points_in`, `points_transformed`, `points_failed`;
  `from_batch`, `with_counts`, `summary`).
* `crs_plan::SiteCalibration::fit(name, &target, &[ControlPair], when)` with `to_target`, `to_local`,
  residuals, RMS and largest residual, warnings; serde.
* `crs_plan::{elevation_factor, combined_scale_factor, MEAN_EARTH_RADIUS_M}`.
* `crs_transform`: `geographic_to_geocentric`, `geocentric_to_geographic`, `Helmert` (both
  `RotationConvention`s), `MolodenskyBadekasParams`, `DatumMethod`, `Transformation` (the registry
  record), `transformations()`, `transformation(code)`, `transformations_between(from_geog, to_geog)`,
  `transformations_containing(from_geog, to_geog, min_lat, min_lon, max_lat, max_lon)` (the helper that
  lists those whose area of use contains the data; it lists, it does not choose), `geographic_crs(code)`.
* `TransverseMercator::new_with_origin` (latitude of origin and the south-orientated variant),
  `ProjectCrs::geographic_epsg`.

Axis order: a projected system is easting then northing in metres (westing then southing for the Lo
systems, the order the registry gives); a geographic system is latitude then longitude in degrees (the
EPSG:4326 page: "Axes: latitude, longitude. Orientations: north, east").

### Sources

All fetched on 2026-11-16.

* **IOGP Publication 373-7-2, Geomatics Guidance Note 7 part 2, "Coordinate Conversions and
  Transformations including Formulas", revised September 2019** (162 pages),
  <https://www.iogp.org/wp-content/uploads/2019/09/373-07-02.pdf>, read in full where cited (GN7-2).
  The epsg.org page lists a newer version (74, July 2026) but offers it only through a form asking for
  a name and e-mail address; the form was not filled in. The 2019 revision is the one the publisher
  serves openly; the method sections used (3.2.3.1, 3.2.3.3, 4.1.1, 4.1.4, 4.2.2 to 4.2.4, 4.3.1,
  4.7.3) were read, the 2026 text was not.
* **EPSG registry through epsg.io**: `https://epsg.io/<code>` for each transformation and each
  geographic CRS; `.wkt` and `.json` for each projected CRS and ellipsoid. The fetch tool summarises a
  page rather than returning it raw, so a mistaken digit in a summary is possible. The codes and
  numbers are in `crs_transform.rs` and `crs.rs` for review against the registry.
* **Combined scale factor** (not in GN7-2): "Scale Factors - Geodesy for Surveyors",
  <https://www.surveybible.com/learn/geodesy/scale-factors> (no author or date shown; cites Ghilani and
  Wolf, Elementary Surveying, 13th ed.), and S. Billings, "Ground versus Grid - Low Distortion
  Projections - Part 2", The American Surveyor, 28 September 2013,
  <https://amerisurv.com/2013/09/28/ground-versus-grid-low-distortion-projections-part-2/>.

**No independent geodetic software was used to check any result.** The check values are the worked
examples of GN7-2, the published example of the combined factor, and round trips. Nothing in this
environment provided check values.

### Systems added

| Code | Name | Method | Area of use (S W N E) |
|------|------|--------|-----------------------|
| 4326 | WGS 84 (geographic; a datum ensemble in the registry, accuracy 2.0 as the page shows it) | | world |
| 4210 | Arc 1960 (geographic) | | -11.75 28.85 4.63 41.91 |
| 4209 | Arc 1950 (geographic), Clarke 1880 (Arc) | | -26.88 19.99 -8.19 35.93 |
| 4201 | Adindan (geographic), Clarke 1880 (RGS) | | 3.4 21.82 22.24 47.99 |
| 4222 | Cape (geographic), Clarke 1880 (Arc) | | -34.88 16.45 -17.78 32.95 |
| 4148 | Hartebeesthoek94 (geographic), WGS 84 ellipsoid | | -50.32 13.33 -22.13 42.85 |
| 20135 to 20138 | Adindan / UTM zones 35N to 38N | TM 9807 | 4.21 23.99 22.01 30.01; 3.49 29.99 22.24 36; 3.4 36 22.01 42; 4.11 42 12.85 47.99 |
| 20934, 20935, 20936 | Arc 1950 / UTM zones 34S, 35S, 36S | TM 9807 | -26.88 19.99 -10.86 24; -25.84 24 -8.31 30; -22.42 30 -8.19 35.93 |
| 2046 to 2055 | Hartebeesthoek94 / Lo15 to Lo33 (odd) | TM South Orientated 9808 | per zone, in `OTHER_PROJECTED` |
| 21035 to 21037, 21095 to 21097 | Arc 1960 / UTM zones (the registry lists exactly these six: a search for "Arc 1960 UTM" returned them) | TM 9807 | as before |

### Transformations recorded

Every row is "Geocentric translations (geog2D domain)" (EPSG method 9603, GN7-2 4.2.4) from the
source CRS to WGS 84 (EPSG:4326), read from `https://epsg.io/<code>` on 2026-11-16. Accuracy is the
registry's operation accuracy as shown (the pages did not always show a unit; the remarks give the
per-axis accuracies in metres and the figure is read as metres). Translations are in metres.

| Code | Name | Source | tX tY tZ | Area of use (box S W N E) | Accuracy | Revision |
|------|------|--------|----------|---------------------------|----------|----------|
| 1284 | Arc 1960 to WGS 84 (2) | 4210 | -157 -2 -299 | Kenya - onshore (-4.72 33.9 4.63 41.91) | 6 m | 2020-03-14 |
| 1285 | Arc 1960 to WGS 84 (3) | 4210 | -175 -23 -303 | Tanzania - onshore (-11.75 29.34 -0.99 40.48) | 15 m | 2020-03-14 |
| 1122 | Arc 1960 to WGS 84 (1) | 4210 | -160 -6 -302 | Kenya; Tanzania (-11.75 29.34 4.63 41.91) | 35 m | 2020-03-14 |
| 3998 | Arc 1960 to WGS 84 (4) | 4210 | -153 -5 -292 | Burundi (-4.45 28.98 -2.3 30.86) | 35 m | 2025-03-16 |
| 1114 | Arc 1950 to WGS 84 (2) | 4209 | -138 -105 -289 | Botswana (-26.88 19.99 -17.78 29.38) | 7 m | 2020-03-14 |
| 1116 | Arc 1950 to WGS 84 (4) | 4209 | -125 -108 -295 | Lesotho (-30.66 27.01 -28.57 29.46) | 10 m | 2025-01-06 |
| 1121 | Arc 1950 to WGS 84 (9), replaced by 6906 | 4209 | -142 -96 -293 | Zimbabwe (-22.42 25.23 -15.61 33.08) | 15 m | 2020-03-14 |
| 6906 | Arc 1950 to WGS 84 (10) | 4209 | -145 -97 -292 | Zimbabwe (-22.42 25.23 -15.61 33.08) | 17 m | 2020-03-14 |
| 1118 | Arc 1950 to WGS 84 (6) | 4209 | -134 -105 -295 | Eswatini (-27.32 30.79 -25.72 32.14) | 26 m | 2025-01-06 |
| 1117 | Arc 1950 to WGS 84 (5) | 4209 | -161 -73 -317 | Malawi (-17.14 32.68 -9.37 35.93) | 27 m | 2020-03-14 |
| 6904 | Arc 1950 to WGS 84 (11) | 4209 | -179 -81 -314 | Malawi (-17.14 32.68 -9.37 35.93) | 29 m | 2020-03-14 |
| 1120 | Arc 1950 to WGS 84 (8) | 4209 | -147 -74 -283 | Zambia (-18.08 21.99 -8.19 33.71) | 41 m | 2020-03-14 |
| 1113 | Arc 1950 to WGS 84 (1) | 4209 | -143 -90 -294 | Botswana; Eswatini; Lesotho; Malawi; Zambia; Zimbabwe (-30.66 19.99 -8.19 35.93) | 44 m | 2025-01-06 |
| 1103 | Adindan to WGS 84 (4) | 4201 | -165 -11 206 | Ethiopia (3.4 32.99 14.89 47.99) | 6 m | 2020-03-14 |
| 1106 | Adindan to WGS 84 (7) | 4201 | -161 -14 205 | South Sudan; Sudan - onshore (3.49 21.82 22.24 38.66) | 7 m | 2020-03-14 |
| 1100 | Adindan to WGS 84 (1) | 4201 | -166 -15 204 | Eritrea; Ethiopia; South Sudan; Sudan (3.4 21.82 22.24 47.99) | 9 m | 2020-03-14 |
| 1128 | Cape to WGS 84 (1) | 4222 | -136 -108 -292 | South Africa - mainland onshore (-34.88 16.45 -22.13 32.95) | 9 m | 2020-03-14 |
| 1129 | Cape to WGS 84 (2) | 4222 | -134.73 -110.92 -292.66 | South Africa - mainland onshore (same box) | 15 m | 2021-09-23 |

The names are as each transformation's own page showed them; the code is the identifier.

The caller chooses. For Arc 1960 in Tanzania the candidates that cover the data are 1285 (15 m) and
1122 (35 m); in Kenya, 1284 (6 m), 1285 (its box reaches 0.99 degrees south, so it covers the Nairobi
area on paper) and 1122. The list is in order of stated accuracy and is a list: the application should
show it and let the user pick. **The registry has no Arc 1960 transformation named for Uganda or
Rwanda.** The 1122 box covers them geographically but its text says Kenya and Tanzania, so for Ugandan
data it appears as a candidate by box only and the plan carries no warning. The application should show
the area text, which is in every step record.

**All of these are three-parameter shifts with accuracies of 6 m to 44 m. For none of the systems read
did the registry page show a grid-based transformation or a more accurate option, so none is
implemented. If a survey needs better than the stated figure, a local transformation from control
points is the route (see site calibration), or a national grid file, which this kernel does not
read.** Not recorded: Arc 1950 to WGS 84 codes 1115 (Burundi, 35 m) and 1119 (Democratic Republic of
the Congo, 25 m), listed deprecated on the registry page and not opened; the West African Adindan
transformations 1101, 1102, 1104, 1105 (44 m each). Hartebeesthoek94: its page lists no transformation
to WGS 84, so a Lo system has no path to another datum (the plan says so rather than assuming zero);
the Cape to WGS 84 (2) remark treats Hartebeesthoek94 and WGS 84 as equivalent to within 15 m, which
is the registry's statement about that transformation, not a transformation of its own.

WGS 84 (EPSG:4326) is itself a datum ensemble in the registry with a stated accuracy of 2.0 (page
shows no unit). It is not added to a plan's total; the figure is in the `accuracy_basis` sentence.
A plan through two transformations (for example Arc 1960 to Adindan, by WGS 84) shows the **sum** of
the two accuracies, the cautious figure; the registry gives no rule for combining them.

### Formulas and the worked examples that test them

| Item | GN7-2 | Test |
|------|-------|------|
| Geographic to geocentric, and back (closed form then iteration) | 4.1.1 | The note's North Sea example (X 3 771 793.968, Y 140 253.342, Z 5 124 304.349 to 53 48 33.820 N, 2 07 46.380 E, h 73.0 m); round trips including the poles and negative heights |
| Geocentric translations | 4.2.4 | The note's WGS 84 to ED50 example (tX 84.87, tY 96.49, tZ 116.95) |
| Helmert 7-parameter, Position Vector and Coordinate Frame | 4.2.3 | The note's WGS 72 to WGS 84 example in both conventions (rZ = +0.554" Position Vector, -0.554" Coordinate Frame; same coordinates); the wrong convention is shown to miss by over 0.5 m |
| Molodensky-Badekas (both conventions) | 4.2.2, 4.3.1 | The note's La Canoa to REGVEN example, all steps (geographic to geocentric, the transformation, geocentric to geographic) to the printed 0.01 m and 0.001" |
| Transverse Mercator, general case | 3.2.3.1 | The note's British National Grid example, forward and reverse |
| Transverse Mercator (South Orientated) | 3.2.3.3 | The note's Hartebeesthoek94 / Lo29 example: westing 71 984.49, southing 2 847 342.74, axis order westing then southing |
| Similarity transformation | 4.7.3 | The note's ED50 to ETRS89 UTM 31N example, forward and reverse (299 905.060 E, 4 499 796.515 N); the fit recovers its parameters from points generated with them |
| Combined scale factor | not in GN7-2 | The published example, 2500 ft above the ellipsoid with k 0.99990: elevation factor 0.999880, combined 0.999780, 1000.000 ft ground is 999.780 ft grid |

Rotation sign convention, as the note states it (4.2.3): in the Position Vector convention the matrix
is `[[1 -rZ +rY] [+rZ 1 -rX] [-rY +rX 1]]`, in the Coordinate Frame convention the rotations have the
opposite sign. A parameter set from a source must be used with the convention it was published in;
`Helmert` carries the convention with the parameters. None of the recorded registry transformations
needs a Helmert or Molodensky-Badekas step (all are three-parameter), so those methods are
implemented and tested on the note's examples but no registry record uses them yet.

Reverse of a transformation: translations are exact (change the signs). For Helmert and
Molodensky-Badekas the reverse is the same formula with all signs changed, which is how the EPSG
dataset documents reversibility; the note puts the misclosure of a round trip at no more than 2 cm in
practical circumstances, and the tests assert it.

### Heights, stated plainly

The kernel has no geoid model. A datum transformation between geographic systems runs through
geocentric coordinates and so needs a height (GN7-2 4.3.1, 4.1.4). `TransformPlan::apply(first,
second, height)`:

1. uses the height as given **as an assumed ellipsoidal height** of the input in the horizontal
   transformation (or zero, with `HeightAssumption::Zero`: GN7-2 4.1.4 says the assumed height is "usually
   either set to the gravity-related height ... or set to zero");
2. transforms the horizontal position;
3. returns the height **unchanged**. Elevations are never converted, and `TransformLog.height_treatment`
   says so in a sentence.

The effect of the assumption on the horizontal result follows from the formulas. Changing the input
height by dh moves the geocentric point by dh along the local vertical. The datum step adds a constant
translation t; the angle of the target point is then changed by about `t_horizontal * dh / r^2`, so the
horizontal position moves by `t_h * dh / r` where r is about 6.37e6 m and t_h is the part of the
translation across the local vertical. The largest translation recorded has length 370.4 m (EPSG:6904),
so **a height error of 1000 m moves the horizontal result by at most 0.058 m, 100 m by 0.006 m, 10 m
by 0.6 mm**, and a height error of 30 m (a large geoid separation) by under 2 mm. Measured with
`height_sensitivity` at Dodoma with EPSG:1285: 0.0041 m per 100 m, 0.0406 m per 1000 m, 0.122 m per
3000 m (linear, as the formula says). If horizontal position matters to better than a centimetre and the height is uncertain by more than
about 150 m, give the plan an ellipsoidal height.
This is why the two-dimensional round trip is not exact: the height after the shift (14 m at Dodoma)
differs from the height put in, so `apply` then `reversed().apply` misses by the effect of that
difference, 0.3 mm to 0.6 mm in the test; `apply_3d` returns the height above the target ellipsoid and
round-trips to 1e-5 m. That height is not an elevation.

### Bearings and distances

A transformation between grids changes what a bearing and a distance read as. `point_effects(first,
second, height)` reports, from the projection formulas, the point scale factor and grid convergence of
both systems, the ratio of the scale factors (multiply a source grid distance by it) and the change of
grid bearing of a line (`gamma_source - gamma_target`, where gamma is the angle from grid north to true
north, counterclockwise positive, and a line's true azimuth is its grid bearing plus gamma).
`baseline_effect` measures the same by transforming a baseline.

Measured at Dodoma (Arc 1960 / UTM 36S to WGS 84 / UTM 36S): the projection formulas give a bearing
change of 0.00019 degrees and a scale ratio of 1.0000005. The transformed baseline differs from this
by 0.29 to 0.31 arc-seconds in bearing and 1.2 to 2.2 parts per million in length at 100 m, 1 km
and 10 km, in every azimuth tried: that is the effect of the datum step itself (different ellipsoid,
shifted origin), which `point_effects` does not contain. So: **the grid terms are small here, the
datum step turns lines by about 0.3 arc-second and changes lengths by a few parts per million (a few
millimetres per kilometre), and the application should quote the measured baseline when it matters.**
Across a UTM zone change on one datum (36S to 37S at 3 degrees south, 35.9 east) the grid bearing
changes by about 0.3 degrees, and the prediction agrees with a 200 m baseline within the test's
tolerances of 0.0002 degrees and 3 ppm.
No bearing change is given for a geographic or a south-orientated end.

### Combined scale factor

`combined_scale_factor(point_scale, ellipsoidal_height_m, radius_m) = k * R / (R + h)`, and a ground
distance times it is the grid distance (the two sources above). `h` is an **ellipsoidal** height: an
orthometric elevation used in its place is wrong by the geoid separation over R, about 1.6 parts per
million per 10 m. R is a mean radius, 6 371 000 m, "approximately", from the open reference; it is a
parameter. The example of that reference is a test.

### Site calibration

`SiteCalibration::fit` fits the similarity transformation of GN7-2 4.7.3 (`XT = XTO + M (XS cos t + YS
sin t)`, `YT = YTO + M (-XS sin t + YS cos t)`) from control point pairs by least squares. The note
gives the model; the fit is derived in the doc comment (centroids, then `a = M cos t` and `b = M sin t`
from the normal equations), and the tests check that it recovers the note's own example parameters
from points generated with them and that no nearby parameter set has a smaller sum of squares. The
record has the origin, M, rotation, the residual of every control point (observed minus fitted), the
RMS and the largest residual, and warnings: two points fit exactly and prove nothing; a scale that
differs from 1 by more than 1000 ppm deserves a look; heights are not calibrated. It serialises with
serde, and `to_target` and `to_local` are exact inverses.

### Left out, and why

* **Grid-based transformations** (NTv2 and the like): no fetched transformation needs one, and the
  kernel reads no grid files.
* **Other datums of the region** (Mozambique, Angola, Namibia, Madagascar and others) were not fetched.
  Add one by fetching its geographic CRS page and each transformation page, then a row in
  `crs_transform.rs`.
* **Other projected systems**: national grids and Gauss-Krüger or Lo systems of other datums were not
  fetched. Lo systems were added only because GN7-2 gives a worked example for the Lo29 axis
  convention. Methods other than transverse Mercator were not needed by any fetched system.
* **Time-dependent Helmert, point motion**: no fetched transformation is time-dependent.
* **Vertical transformations and geoid models**: none; see Heights.
* **Newer GN7-2 text** (the July 2026 version): behind a form, not read.

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
