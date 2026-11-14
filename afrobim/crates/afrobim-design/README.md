# afrobim-design

Concept design of road alignments for AfroBIM: alignment design from scratch (work package C1)
and comparison of alignment options (C2). Metres and radians, plan X east and Y north, stations
are internal stations, as in `afrobim-alignment`.

The output of a design is an ordinary `afrobim_alignment::Alignment` with its `Profile`, stored
in the document the way an imported alignment is, so signs, markings, drainage, corridor and
sheets work on it unchanged.

**This crate contains no standard's values.** Every limit, friction factor, reaction time,
height, rate and weight is an input. See "Standards" below.

## Contents

| part | what it does |
|---|---|
| `HorizontalDesign` | PIs (`afrobim_alignment::Pi`: x, y, radius, spiral in, spiral out) to tangents, spirals and arcs, with all curve data and stations |
| `VerticalDesign` | PVIs with a parabolic curve given by length or by K; grades, curve ends, high and low points, levels |
| `fit_to_ground` | a starting-point profile proposed from a TIN along the centreline (not a design) |
| `AlignmentDesign` | both together; `build()` gives the `Alignment` with profile; `to_landxml()` |
| `run_checks`, `DesignBasis` | checks against limits the user supplies |
| `OptionSet`, `compare_options`, `score` | several options compared on figures computed from the model |
| `add_design`, `store_design`, `add_option_set`, `DesignModule`, `OptionSetModule` | the document elements |
| `physics` | the physics relations, derived in the doc comments |

## Formulae

Stated and derived in the module documentation (`src/horizontal.rs`, `src/vertical.rs`,
`src/physics.rs`): plain curve `T = R tan(D/2)`, `Lc = R D`, `E = R (sec(D/2) - 1)`,
`M = R (1 - cos(D/2))`; the clothoid by integrating the heading, its series, the spiral shift
`p = y - R (1 - cos ts)` and extension `k = x - R sin ts`, the tangent lengths with equal and
unequal spirals; the parabola, K, turning point `x = -g1 L / A`; `e + f = V^2 / (127 R)` from
`v^2 / (g R)`; stopping distance; crest and sag K from sight geometry.

Clothoid accuracy: the alignment crate evaluates spirals by Gauss-Legendre quadrature and also
holds the Fresnel power series. `tests/clothoid_reference.rs` compares both with an arbitrary
precision Fresnel evaluation (Python mpmath, 50 digits, script in `tests/data/gen_clothoid.py`)
for eight spirals (30 to 250 m long): the largest differences are **8.5e-14 m (quadrature)** and
**2.8e-14 m (series)**; the test asserts 1e-10 m.

## API examples

### Horizontal

```rust
use afrobim_design::*;

let mut h = HorizontalDesign::new("Option A", 0.0, vec![
    Pi::point(0.0, 0.0),
    Pi::spiral_curve(600.0, 0.0, 300.0, 80.0, 80.0), // radius 300, spirals 80 m in and out
    Pi::point(1000.0, 500.0),
]);
let solved = h.solve()?;                       // HorizontalSolution
let c = solved.curve_at_pi(1).unwrap();        // CurveData
println!("T {:.3} L {:.3} E {:.3} TS {}", c.tangent_in_m, c.total_length_m, c.external_m,
         afrobim_alignment::format_station(c.ts_station_m, 3));
let alignment = solved.alignment;              // afrobim_alignment::Alignment
// solved.tangents[i].straight_m and .reverse report the straight between curves.
```

### Vertical

```rust
let mut v = VerticalDesign::new("Option A", vec![
    VPvi::point(0.0, 100.0),
    VPvi::with_k(400.0, 108.0, 50.0),          // or VPvi::with_length(400.0, 108.0, 200.0)
    VPvi::point(800.0, 100.0),
]);
let s = v.solve()?;
let z = s.level_at(350.0)?;                    // 106.75
let high = s.high_points();                    // [(400.0, 107.0)]
```

### Edit operations for the sketching UI

Each takes `&mut` design, applies the change to a copy, and returns the recomputed solution, or
a `DesignError::Impossible` whose `problems()` hold plain-words messages with the PI or PVI
index. **A refused edit leaves the design unchanged**, so the UI can call it on every drag step
and show the message while the cursor is somewhere impossible.

| call | for |
|---|---|
| `h.insert_pi(index, Pi)` | click to add a PI |
| `h.move_pi(index, x, y)` | drag a PI |
| `h.delete_pi(index)` | delete a PI |
| `h.set_radius(index, r)` | change a radius (0 removes the curve) |
| `h.set_spirals(index, ls_in, ls_out)` | change spiral lengths |
| `h.feasible_radius_range(index)` | `(min, max)` radius that fits now, for drag limits |
| `v.insert_pvi(index, VPvi)`, `v.insert_pvi_at_station(s)` | add a PVI (the latter on the profile as it is) |
| `v.move_pvi(index, station, elevation)` | drag a PVI |
| `v.delete_pvi(index)` | delete a PVI |
| `v.set_curve_length(index, l)`, `v.set_curve_k(index, k)`, `v.set_curve(index, CurveSpec)` | change or remove a curve |

`plan_labels(&design)` gives text labels with plan positions (PI number and station, curve data,
TS/SC/CS/ST points) for a front end that can draw text: the draw crate has glyph outlines only
and the module's drawing therefore has no text.

### Fit to ground (starting point only)

```rust
let fit = fit_to_ground(&alignment, &tin, &FitInputs {
    sample_spacing_m: 10.0, tolerance_m: 0.3, min_segment_length_m: 100.0,
    max_grade: 0.06, min_curve_length_m: 60.0, k_crest: None, k_sag: None, // all entered by the user
})?;
println!("{}", fit.label);        // "STARTING POINT ONLY: ..."
let vertical = fit.design;        // a VerticalDesign; edit it like any other
// fit.notes lists every grade moved and every curve held short; fit.max_above_ground_m etc.
```

The method (least squares segments, greedy) is stated in `src/fit.rs`. It knows nothing about
drainage, cover, sight distance, balance or standards.

### Build, store, export

```rust
let design = AlignmentDesign::new(h, Some(v));
let built = design.build()?;                         // DesignedAlignment { alignment, horizontal, vertical, warnings }
let xml = design.to_landxml(&LandXmlOptions::default())?;

let (design_id, alignment_id) = add_design(&mut doc, &design)?;   // two elements, see below
let mut d = load_design(&doc, design_id)?;
d.horizontal.move_pi(1, 610.0, 5.0)?;
store_design(&mut doc, design_id, &d)?;              // rebuilds the alignment element in place
```

`add_design` adds an `alignment` element (layer `Alignments`, hidden, site placement, exactly
as the app's alignment glue adds an imported one; name made unique) and an `alignment_design`
element holding the PIs and PVIs and the alignment element's id. `store_design` replaces the
alignment element's data keeping its id and name, so corridors and drainage elements stay
linked (refresh them afterwards, as after any re-import). An impossible design is refused and
nothing changes. Register the modules with `afrobim_design::register(&mut registry)`; the
`alignment_design` module draws the PI polyline (grey), the centreline (black), PIs (red
crosses), curve centres (blue crosses), lists curve data in `derived`, and reports impossible
geometry as Error notes.

### LandXML export

`write_landxml` / `AlignmentDesign::to_landxml`. Element and attribute names were checked
against the LandXML 1.2 schema retrieved from
`http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd`, and an exported file validates:

```
xmllint --noout --schema LandXML-1.2.xsd sample.xml    # sample.xml validates
```

(`AFROBIM_DESIGN_SAMPLE_OUT=sample.xml cargo test -p afrobim-design --test landxml write_sample`
writes the sample.) Written: `Units/Metric`, `Application`, `Alignments/Alignment`
(`name`, `length`, `staStart`), `CoordGeom` with `Line`, `Curve` (`Start`, `Center`, `End`,
`PI`), `Spiral` (`Start`, `PI`, `End`: the schema requires all three, the PI being the
intersection of the end tangents), `StaEquation`, `Profile/ProfAlign` with `PVI`, `ParaCurve`,
`UnsymParaCurve`, `CircCurve`. Not written: superelevation, existing-ground profiles,
cross-sections, `dir`/`dirStart`/`dirEnd` attributes. Round trip through the
`afrobim-alignment` importer (`tests/landxml.rs`): element starts, headings, lengths and
curvatures agree to **2.8e-11** on a test alignment with spirals; the test asserts 1e-6 (m,
rad, 1/m); PVIs, curve lengths and levels agree to 1e-9.

### Checks against a standard

```rust
let basis = DesignBasis::from_csv_str(&std::fs::read_to_string("limits.csv")?)?;
let row = basis.row("80 km/h").unwrap();
let report = run_checks(&basis, row, Some(&built.horizontal), built.vertical.as_ref(), &CheckOptions::default());
for f in report.failures() { println!("{}: {}", f.location, f.message); }
std::fs::write("checks.csv", report.to_csv())?;
```

A limits file is CSV with optional `#name,...` and `#source,...` lines (the source is
required: document, edition, table, page, or `user input`), then a header and one row per
design speed. Columns (all but `label` optional, empty means no limit): `label`,
`design_speed_kmh`, `min_radius_m`, `max_superelevation`, `max_side_friction`,
`min_curve_length_m`, `min_spiral_length_m`, `min_tangent_same_direction_m`,
`min_tangent_reverse_m`, `max_grade`, `min_grade` (ratios), `min_k_crest`, `min_k_sag`,
`min_vertical_curve_length_m`, `stopping_sight_distance_m`, `reaction_time_s`,
`longitudinal_friction`, `eye_height_m`, `object_height_m`, `headlight_height_m`,
`headlight_beam_deg`. JSON works too (`DesignBasis::from_json_str`).

A missing limit is reported `NotChecked`, never passed. A minimum radius, a stopping sight
distance and the K values can be left out and derived from the physics inputs; the report says
which limit was derived and from what. Checks: radius, curve length, spiral length, tangent
between curves (same direction and reverse), grade (max and min), K (crest and sag), vertical
curve length, and stopping sight distance provided over each vertical curve by a numerical
sight-line test (crest: eye and object heights; sag: headlight height and beam angle).

### Options

```rust
let set = OptionSet { name: "Alternatives".into(), surface: Some(surface_id), estimate: Some(estimate_id),
    options: vec![AlignmentOption { name: "A".into(), design: design_a, corridor: Some(def), note: String::new() }, /* ... */],
    criteria: vec![Criterion { key: "fill_m3".into(), weight: 2.0, direction: Direction::LowerIsBetter }] };
let id = add_option_set(&mut doc, &set);
let cmp = compare_in_document(&doc, id, crossing_features_from_alignments(&doc, &["A", "B"]))?;
let csv = options_comparison_csv(&cmp);               // unweighted figures, how computed, reasons for gaps
let scores = score(&cmp, &set.criteria);              // the user's weights
let csv2 = scoring_csv(&scores);
```

Figures and how each is computed are listed in `src/options.rs`: length, number of curves,
minimum radius, maximum grade, rise and fall, mean absolute grade, turning per km, share of
length in curves, cut, fill, balance, length with no volume, land take, crossings, cost. Cut
and fill come from `afrobim_corridor::earthworks` (average end area) on the option's corridor
definition against the ground. Land take is the corridor footprint, daylight to daylight,
summed over section pairs. Cost is the user's `Estimate` computed on quantity lines made from
the option (`earthworks.cut`, `earthworks.fill`, `alignment.length`, `land.take_area`, plus any
extra lines supplied); **no rate is built in**. Crossings are counted against features the user
supplies (rivers, roads, other alignments of the document). A figure that cannot be computed
says why. Scoring scales each criterion 0 to 1 between the worst and best option and weights
them with the user's weights; the unweighted figures are always printed beside.

## Standards

### Uganda MoWT Road Design Manual Volume 1: Geometric Design (2010): NOT OBTAINED

I tried to fetch it and could not. What was tried: two web searches (no copy of the Volume 1
PDF among the results; the results were other documents, among them a JICA study that cites
the manual's values second-hand, a regional manual of a different title, and district road
works manuals); the Ministry of Works and Transport site `works.go.ug`: its "Manuals and
specifications for road works" page lists Volumes VI (Urban Roads), VI Part 2 (Standard
Drawings) and VII (Non Motorized Transport) only, and guessed Volume 1 addresses returned 404;
`unra.go.ug` refused the connection. No table of the manual was read, so **no `mowt_2010` basis
is shipped and no value is taken from it**; there is no worked example to reproduce.

The owner should supply, from Volume 1 (2010), as a limits CSV (one row per design speed and,
if the manual distinguishes them, terrain or road class), with the table number and page in the
`#source` line:

1. design speeds by road class and terrain;
2. minimum radius, or the maximum superelevation and the side-friction factors by design speed
   (the radius is then derived by `e + f = V^2 / (127 R)`);
3. minimum length of horizontal curve, and minimum or maximum spiral (transition) lengths;
4. minimum tangent between curves, same direction and reverse;
5. maximum and minimum grades by design speed and terrain;
6. minimum K for crest and sag curves, or the stopping sight distance table plus the eye height,
   object height, headlight height and beam angle it assumes;
7. minimum length of vertical curve;
8. reaction time and longitudinal friction behind the stopping sight distances, if the K values
   are to be derived.

### AASHTO A Policy on Geometric Design (Green Book)

No citable, retrievable copy of the tables was found in this work, so none of its values is
included. The same limits file takes AASHTO values the user enters, with the edition, table and
page in `#source`.

## Limits and what is not done

- The design checks are the geometric ones listed above. Not checked: lateral sight distance on
  horizontal curves, passing and decision sight distance, superelevation rates and runoff
  lengths (superelevation is not designed here), combinations of horizontal and vertical
  alignment, cross-section elements.
- The crest and sag K derivations hold for sight distance not longer than the curve; real
  curves are checked by the numerical test, which has no such restriction. The sag test
  assumes the beam angle is measured from the vehicle's grade.
- Vertical curves are parabolas (by length or K); asymmetric and circular vertical curves are
  not offered by the design (they are read and written by the alignment crate and LandXML).
- A PI with spirals needs the spirals to turn less than the deflection; compound curves
  (curves of different radius in sequence) are not made from PIs. `AlignmentBuilder` in
  `afrobim-alignment` builds those element by element.
- Stations: the first PI is at `start_station_m`; the profile must cover the alignment's
  stations for the corridor to use it (a warning says when it does not).
- Fit to ground is a greedy fit, not an optimisation, and marked as a starting point.
- Land take is a plan footprint, not a cadastral take; widths on curves are not corrected for
  curvature. Earthworks are average end area, no prismoidal correction, as in the corridor crate.
- Crossings are plan intersections with the supplied line features; no levels, no skew, no
  structure sizing.
- The plan drawing has no text (draw crate limitation); use `plan_labels`.
- The app has no interactive sketching UI yet; the edit operations above are what it should call.
- LandXML export is checked against the schema and the AfroBIM importer, not against a file
  read by Civil 3D or another third-party program.
