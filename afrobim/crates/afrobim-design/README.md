# afrobim-design

Concept design of road alignments for AfroBIM: alignment design from scratch (work package C1)
and comparison of alignment options (C2). Metres and radians, plan X east and Y north, stations
are internal stations, as in `afrobim-alignment`.

The output of a design is an ordinary `afrobim_alignment::Alignment` with its `Profile`, stored
in the document the way an imported alignment is, so signs, markings, drainage, corridor and
sheets work on it unchanged.

**The only standard's values in this crate are those of the Uganda MoWT Road Design Manual Volume I:
Geometric Design (January 2010), each read from that document and carried with its table, section
and page** (module `mowt`, built-in basis `mowt_2010`; see "Uganda MoWT Road Design Manual Volume I"
below). Every other limit, friction factor, reaction time, height, rate and weight is an input. See
"Standards" below.

## Contents

| part | what it does |
|---|---|
| `HorizontalDesign` | PIs (`afrobim_alignment::Pi`: x, y, radius, spiral in, spiral out) to tangents, spirals and arcs, with all curve data and stations |
| `VerticalDesign` | PVIs with a parabolic curve given by length or by K; grades, curve ends, high and low points, levels |
| `fit_to_ground` | a starting-point profile proposed from a TIN along the centreline (not a design) |
| `AlignmentDesign` | both together; `build()` gives the `Alignment` with profile; `to_landxml()` |
| `run_checks`, `DesignBasis` | checks against limits the user supplies, or against the built-in `mowt_2010` basis (`DesignBasis::mowt_2010`) |
| `mowt` | the Uganda MoWT Road Design Manual Vol. I (January 2010): typed lookups of its tables with citations, and the `mowt_2010` basis |
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

The limits are either the user's own table (below) or the built-in Uganda MoWT basis
(`DesignBasis::mowt_2010`, see "Uganda MoWT Road Design Manual Volume I"); both go through the same
`run_checks`.

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

### Uganda MoWT Road Design Manual Volume I: Geometric Design (January 2010): built-in basis `mowt_2010`

#### Sources

| | |
|---|---|
| Document | Ministry of Works and Transport (Uganda), *Road Design Manual Volume I: Geometric Design* (running header "Geometric Design Manual"), 271 PDF pages |
| Edition, as the document states it | **January 2010**: the preamble (printed page i, PDF page 1) is signed "Ministry of Works and Transport, P.O. Box 10, Entebbe, January 2010" and says the Manual "is part of the revised Road Design Manual, November 1994"; section 1.1.1 (printed page 1) says it supersedes the Road Design Manual of November 1994. The copy has no title page and no foreword. The PDF metadata gives a creation date of 3 May 2011 (not an edition date). |
| Where fetched | `https://uace.or.ug/wp-content/uploads/2023/03/Volume-1-Geometric-Design-Manual.pdf`, fetched 2026-11-15 |
| Status of the copy | **A third-party copy**, hosted by the Uganda Association of Consulting Engineers, not the Ministry's own site. It has not been compared with a Ministry original. The PDF is not committed to the repository. |
| Pages | Printed page + 9 = PDF page throughout the parts used. Every citation gives both: `p. 80 (PDF p. 89)`. |

How it was read: the text layer (`pdftotext -layout`) for text, and every page holding a table or figure
used was rendered (`pdftoppm -r 120`) and looked at. The tables were compared with the rendered pages;
the superelevation tables 6-4 to 6-9 were taken from the text layer, compared with the rendered pages
for Tables 6-4, 6-6 and 6-8, and then checked cell by cell against the manual's own formula (see "What
was checked"). **The text layer of printed page 45 (PDF page 54) holds a second, hidden set of figures
for Table 4-4 that does not show on the rendered page** (Class Ib, flat: 110 km/h, SSD 180 m, passing
730 m, radius 530 m, rolling crest K 62); what the rendered page shows was used and the conflicting
cells are withheld (slip S01). Tables 4-3 to 4-9 were also compared with the text layer, which matches
the rendered values except for that hidden set. No value comes from memory, from another standard, or
from a derivation.

#### What was taken from where

| What | Table or figure | Section | Printed p. (PDF p.) | Where in the crate |
|---|---|---|---|---|
| Terrain classes and transverse slopes | text | 5.2 | 53 (62) | `mowt::classes::terrain_for_transverse_slope` |
| Perception-brake reaction time 2.5 s | text | 5.3, 6.7.2 | 55 (64), 128 (137) | `mowt::sight::inputs` |
| Design classes Ia, Ib, II, III, A, B, C; traffic ranges (capacity, pcu x 1,000/day); road way width; maximum design speed by terrain; functional classes | Table 4-2a | 4.5 | 44 (53) | `classes::class_definition`, `table_4_2a_max_design_speed` |
| Right of way, carriageway, lane, shoulder, median widths | Table 4-2b | 4.5 | 45 (54) | `classes::class_cross_section`, `cross_section::lane_width_m`, `shoulder_width_m` |
| Class parameter tables: design speed, minimum SSD and passing SD, minimum radius, gradients (desirable, absolute, minimum in cut), maximum superelevation, crest K (stopping, passing), sag K, normal and shoulder cross fall, right of way, for flat, rolling, mountainous and urban/peri-urban | Tables 4-3, 4-4 / 4-5, 4-6, 4-7 / 4-8, 4-9 | 4.5 | 45 (54) / 46 (55) / 47 (56) | `classes::class_parameter` |
| Minimum radius, limiting side friction f, e + f, calculated and rounded radius, for emax 4, 6, 7, 8 % and 30 to 120 km/h; the formula `R = V^2 / (127 (e + f))`; comfort f 0.22 | Table 6-1 | 6.2.3 | 80 (89); 79 (88) | `horizontal::min_radius`, `limiting_side_friction`, `manual_min_radius_m` |
| Maximum relative gradient ratio; runoff formula `L = i x Lw x ratio`, minimum the 2 s distance; multi lane factor | Tables 6-2, 6-3 | 6.2.3 vii | 92 (101), 93 (102) | `horizontal::max_relative_gradient_ratio`, `runoff_length`, `runoff_length_factor` |
| Superelevation rate and minimum runoff length by radius and speed; Rmin cells; emax 4, 7, 8 % with normal crown 2.5 and 4 % | Tables 6-4 to 6-9 | 6.2.3 vii | 98 to 103 (107 to 112) | `horizontal::superelevation`, `superelevation_neighbours`, `table_rmin` |
| Tangent runout, stationing of runoff and runout (as the worked examples compute them) | worked examples 1 and 2 | 6.2.3 vii | 94 to 96 (103 to 105) | `horizontal::tangent_runout_length`, `simple_curve_superelevation_stations`, `spiral_curve_superelevation_stations` |
| Curve widening, fill widening | Table 6-10 | 6.2.4 | 104 (113) | `horizontal::curve_widening`, `fill_widening` |
| Transition required if `R < V^3 / 432`; minimum spiral length `0.0702 V^3 / (R C)`; clothoid `A = sqrt(v^3 / k)`, k 0.3 to 0.6 (0.45); aesthetic limits | text | 6.2.3 iv, v | 88 to 90 (97 to 99) | `horizontal::transition_required`, `min_spiral_length_comfort`, `clothoid_parameter`, `spiral_length_for_k`, `clothoid_aesthetic_limits` |
| Maximum tangent 4.0 km; compound curve ratios 1.5 and 1.75; isolated curve radius + 50 %; switchback radius 20 m or less, 10 m minimum; S-curve A1/A2 1.5 | text | 6.2.2, 6.2.3 | 79, 84, 85, 87, 91 (88, 93, 94, 96, 100) | `horizontal::rules` |
| Sight distance on horizontal curves: line of sight, middle ordinate, required ordinate `R (1 - cos(28.65 S / R))` | Figure 6-21 | 6.2.5 | 105 (114) | `horizontal::line_of_sight_m`, `middle_ordinate_m`, `required_middle_ordinate_m` |
| Crest K, stopping sight distance control; eye 1.07 m, object 0.15 m | Table 6-11 | 6.3.1 | 108 (117) | `vertical::crest_k_stopping` |
| Crest K, passing sight distance control; object 1.3 m | Table 6-12 | 6.3.1 | 110 (119) | `vertical::crest_k_passing` |
| Sag K; headlight 0.6 m, 1 degree | Table 6-13 | 6.3.2 | 111 (120) | `vertical::sag_k` |
| Minimum length formulas of crest and sag curves, comfort length `G V^2 / 395`, K = 30 rule of thumb, drainage grade 0.30 % within 15 m | text | 6.3.1, 6.3.2 | 107, 109, 110, 111 (116, 118, 119, 120) | `vertical::crest_min_length_stopping_m`, `crest_min_length_passing_m`, `sag_min_length_m`, `sag_comfort_length_m` |
| Maximum gradients, desirable and absolute, by terrain and speed | Tables 6-14, 6-15 | 6.5.1 | 122 (131) | `vertical::max_gradient_desirable`, `max_gradient_absolute` |
| Minimum gradient in cuttings 0.3 to 0.5 % | text | 6.5.2 | 123 (132) | `vertical::min_gradient_in_cuttings_range_percent` |
| Critical length of gradient | Table 6-16 | 6.5.4 | 123 (132) | `vertical::critical_length_of_gradient_m` |
| Climbing lane threshold and tapers | Tables 6-17, 6-18 | 6.6 | 125 (134) | `vertical::climbing_lane_threshold_length_m`, `climbing_lane_tapers_m` |
| Stopping sight distance, wet pavement, with its assumptions; effect of grade | Tables 6.19, 6.20 | 6.7.2 | 129 (138) | `sight::stopping_sight_distance`, `stopping_sight_distance_on_grade` |
| Passing sight distance elements and minimum | Tables 6.21, 6.22 | 6.7.3 | 132 (141) | `sight::passing_elements`, `minimum_passing_sight_distance` |
| Meeting sight distance, two SSD + 10 m | text | 6.7.5 | 133 (142) | `sight::meeting_sight_distance_m` |
| Additions to headroom at crests | Table 7-1 | 7.2 | 134 (143) | `cross_section::headroom_addition_at_crest_m` |
| Edge strip and edge line width | Table 7-3 | 7.3 | 136 (145) | `cross_section::edge_strip` |
| Shoulder slopes; normal cross fall rules | text | 7.4, 7.5 | 137 (146) | `cross_section::shoulder_slope_range_percent`, `cross_fall_rules` |

Not taken: Table 5-1 and the capacity, level of service and AADT tables of section 5 (Tables 5-2 to
5-10, a planning guide, not a definition of the design classes); Figure 6-14 and Figures 6-24 to 6-27
(graphs; the K lines they carry are in Tables 6-11 to 6-13); Figure 6-20 (shoulder superelevation, a
drawing; its rule is in the text only); Table 7-2 (lateral clearances by object and speed limit) and
Table 7-4 (side slopes); the phasing rules of section 6.4 (`Rh/Rv` ratio for S-curves); sections 8 to
12 (junctions, interchanges, speed management, furniture). Section 6.7.4 (decision sight distance)
gives no values in section 6.

#### What goes into the check framework, and what stays "not checked"

`DesignBasis::mowt_2010(class, terrain, design_speed_kmh, emax_percent)` builds a one-row basis. The
row carries, each with its source in `StandardRow::limit_sources` (printed in every check item as
"table value (...); source: ..."):

| `StandardRow` field | Value | Source |
|---|---|---|
| `min_radius_m`, `max_superelevation`, `max_side_friction` | rounded radius, emax, limiting f | Table 6-1 |
| `stopping_sight_distance_m` | at the class maximum speed the class table value; below it the low end of the Table 6.19 range | Tables 4-3 to 4-9 / Table 6.19 |
| `min_k_crest`, `min_k_sag` | at the class maximum speed the class table value; below it the low end of the rounded range of Tables 6-11 / 6-13 | Tables 4-3 to 4-9 / 6-11, 6-13 |
| `max_grade` | desirable maximum (default) or absolute maximum (`MowtOptions::grade`), by class and terrain, not by speed | Tables 4-3 to 4-9 |
| `reaction_time_s`, `eye_height_m`, `object_height_m`, `headlight_height_m`, `headlight_beam_deg` | 2.5 s, 1.07 m, 0.15 m, 0.6 m, 1 degree | pp. 55, 128, 110 |
| `min_grade` | only with `MowtOptions::minimum_grade_in_cut`: 0.5 % | Tables 4-3 to 4-9 |

The class tables carry the **low** end of the ranges of Tables 6.19, 6-11 and 6-13 (stopping sight
distance rounded up to a multiple of 5 m, K rounded), which the tests confirm cell by cell. The speed
must be one of 30, 40, ... 120 km/h and not above the maximum of Table 4-2a for the class and terrain.

Checks that stay **not checked** under `mowt_2010` (listed by `not_checked_under_mowt_2010()`, in the
row's `limit_sources` and in the basis notes):

| check | why |
|---|---|
| `H.curve_length` | the manual gives no minimum length of horizontal curve |
| `H.spiral_length` | the spiral length depends on the radius (`0.0702 V^3 / (R C)` or the runoff length of Tables 6-4 to 6-9); there is no single limit per row. Use `mowt::horizontal::min_spiral_length_comfort` |
| `H.tangent_same`, `H.tangent_reverse` | the manual says short tangents between curves "should be avoided" (6.2.3 i, 6.4) but states no length |
| `V.min_grade` | the minimum gradient is for cuttings; the framework applies it to every grade line. Off unless `minimum_grade_in_cut` is set |
| `V.curve_length` | the manual states no absolute minimum length of vertical curve in metres |

Limits the manual states that the framework has no check for (lookups only): superelevation rate and
runoff, maximum tangent 4.0 km, widening, sight distance on horizontal curves, passing sight distance,
critical length of gradient.

Where a class table cell is withheld (below), the cell is `None`; for the crest K `run_checks` then
derives K from the stopping sight distance and the manual's own eye and object heights by
`physics::crest_k_for_sight` and labels it "derived" (for Class Ia and Ib in rolling terrain the result
is 63.3, consistent with Table 6-11's 62).

**Manual constants against the first-principles derivation.** The manual prints `R = V^2 / (127 (e + f))`
and its tables use 127; `physics::min_radius_m` uses `12.96 g = 127.0966`, which gives radii about 0.07 %
smaller (V 80, e 7 %, f 0.14: manual 239.97 m, table 240 m, physics 239.79 m). The limits are the
manual's printed table values. Likewise the manual's crest constants 404 and 946 are 404.2 and 945.8 from the
heights (the printed forms are used in `mowt::vertical`, the general forms are there too).

#### Behaviour outside the tabulated range

Every lookup returns `Result<Cited<T>, DesignError>`; `Cited` carries the value, the unit as printed and
the `Source` (table or figure, section, printed page, PDF page; `source.citation()` gives one line).
Outside the table the lookup **refuses** with a plain message and returns nothing: a speed that is not a
tabulated speed, an emax other than 4, 6, 7, 8 % (Table 6-1) or 4, 7, 8 % with crown 2.5 or 4 %
(Tables 6-4 to 6-9), a radius that is not a row of the superelevation tables (the manual states no
interpolation rule, so none is applied; `superelevation_neighbours` returns the rows either side without
choosing), a radius below the table's Rmin, a widening radius below 20 m or in the gap between two bands,
a gradient that is not a row of Table 6-16, a dash in Tables 6-14, 6-15 and 7-3, escarpment terrain (the
manual defines it but gives no limits), Class Ib in flat terrain. Nothing is extrapolated and nothing is
interpolated anywhere in `mowt`.

#### Withheld cells (the manual contradicts itself)

A cell is withheld, refused with its reason, and left out of the basis when its printed value contradicts
another table of the same manual. The owner decides which figure governs. See the slips table:
S01 (Class Ib flat column), S02 (crest K 80 against 62 for Class Ia and Ib rolling), S03 (passing K 307
against 387, Classes II and A flat), S04 (Class II mountainous gradients), S05 (Class C flat and rolling K
values), S19 (the 5 % downgrade column of Table 6.20).

#### Slips found in the manual

Recorded by `mowt::known_slips()` and listed here. "Printed page (PDF page)".

| id | where (printed page, PDF page) | what | treatment |
|---|---|---|---|
| S01 | Table 4-4 (p. 45, PDF 54) against Table 4-2a (p. 44, PDF 53) | Class Ib flat column prints design speed 120 km/h with the figures of Class Ia; Table 4-2a gives 110 km/h. The PDF page also holds a hidden second set (110 km/h, SSD 180, passing 730, radius 530, rolling crest K 62) that is not visible when the page is rendered. | Class Ib in flat terrain is refused (flat column withheld). |
| S02 | Tables 4-3 and 4-4 (p. 45, PDF 54) | Rolling terrain, 100 km/h: crest K (stopping) printed 80; Table 6-11 and Figure 6-25 give 62 for 100 km/h and S^2/404 = 63.4 for S = 160 m. | Cell withheld; run_checks derives K from the SSD. |
| S03 | Tables 4-5 and 4-7 (p. 46, PDF 55) | Flat terrain, 90 km/h: passing K printed 307; Table 6-12 gives 387 (605^2/946 = 386.9). | Cell withheld. |
| S04 | Table 4-5 (p. 46, PDF 55) against Tables 6-14, 6-15 (p. 122, PDF 131) | Class II mountainous (60 km/h): gradients 6 % and 8 % printed; Tables 6-14 and 6-15 give 8 % and 10 % at 60 km/h in mountainous terrain. The 6 and 8 are those of Class Ia at 80 km/h. | Both gradient cells withheld (maximum gradient not checked). |
| S05 | Table 4-9 (p. 47, PDF 56) | Class C flat (60 km/h) and rolling (50 km/h): crest K stopping 22 and 14 and passing K 246 and 176 are the values of 70 and 60 km/h (Tables 6-11, 6-12 give 14 and 9, 176 and 126), while the sight distances in the same columns are those of 60 and 50 km/h. | Four cells withheld. |
| S06 | Table 4-5 (p. 46, PDF 55) | Urban/peri-urban stopping sight distance printed 58; every other class table prints 60 for 50 km/h (Table 6.19 gives 57.5). | Printed value used (consistent with 57.5). |
| S07 | Tables 6-6, 6-7 (7 % emax; p. 100 to 101, PDF 109 to 110) against Table 6-1 (p. 80) | Printed Rmin at 30 to 120 km/h are 30, 50, 90, 140, 200, 250, 300, 400, 500, 700 m; Table 6-1 (7 %) prints 30, 55, 85, 130, 185, 240, 320, 415, 530, 710 m. Tables 4-3 to 4-9 use Table 6-1. | Table 6-1 values used for the limit; the printed Rmin of the superelevation tables are available from table_rmin. |
| S08 | Tables 6-8, 6-9 (8 % emax; p. 102 to 103, PDF 111 to 112) against Table 6-1 | Printed Rmin at 100 and 120 km/h are 395 and 655 m; Table 6-1 (8 %) prints 400 and 670 m. | Table 6-1 values used for the limit. |
| S09 | Tables 6-8, 6-9 (p. 102 to 103, PDF 111 to 112) | At 100 km/h, R = 500 m (e 7.6 %) and R = 400 m (e 8.0 %): runoff L printed 59 and 62; the manual's formula with the Table 6-2 ratio 210:1 and a 3.5 m lane gives 55.9 and 58.8 (the printed lengths correspond to a ratio of about 222:1, which Table 6-2 does not print). Every other L cell of Tables 6-4 to 6-9 reproduces from the formula. | Printed values kept in the table. |
| S10 | Section 6.2.3 vii (p. 93, PDF 102) against Tables 6-4 to 6-9 | The text says the tables are for 3.65 m lanes; the notes under each table say lane width 3.5 m, and the printed L values reproduce with 3.5 m. | 3.5 m recorded as the lane width of the tables. |
| S11 | Worked example 2 (p. 95, PDF 104) | Takes the superelevation from 'table 6-7' with normal crown 2.5 %; Table 6-7 is for a normal crown of 4 %, Table 6-6 for 2.5 %. The cell used (V 80, R 250: 7.0 %, L 49) is the same in Tables 6-6 and 6-7. | Recorded. |
| S12 | Table 6-2 (p. 92, PDF 101) | The column heading prints the unit of the relative gradient as [%]; the cells are ratios (133:1, ...). | Ratios returned. |
| S13 | Section 6.2.3 v (p. 88, PDF 97) | The clothoid relation is printed A^2 = R - L; Figure 6-13 (p. 89) prints A^2 = R x L. | R x L used. |
| S14 | Worked example 1 on degree of curve (p. 83, PDF 92) | The deflection is given as 20 deg 18' 02" but the working uses 23 deg 18' 02" (T = 59.07, L = 116.50), and E is worked with half of 20 deg 18' 02" (10 deg 09' 01"): E printed 4.55 (4.555 for that angle); with 23 deg 18' 02" it is 6.03. | Recorded; the tests reproduce T, L and the stations with 23 deg 18' 02". |
| S15 | Worked example 2 (p. 95, PDF 104) | L = 20 (23.3 - 2 x 5.615) / 4.5386 = 53.188 uses D = 4.5386; D = 1145.916 / 250 = 4.5837, which gives 52.66 m. The manual's total tangent T = R tan(D/2) + Ls/2 = 76.045 is an approximation; the exact spiral tangent (R + p) tan(D/2) + k is larger, by 0.08 m here (76.12 m against 76.045 m). | Recorded; the crate's exact geometry is used elsewhere. |
| S16 | Worked example 1 on superelevation (p. 95, PDF 104) | 'End of equal superelevation = 10+8219.765 - 16.67 = 10+813.095' prints 8219.765 for 829.765. | Recorded. |
| S17 | Figure 6-21 and example (p. 105, PDF 114) | The text says the ordinate of [b] 'should not be less than that of [a]'; the example concludes '11.85 >> 3.94 design is OK' (available [a] above required [b]). The example works [b] with 28.55 where the relation has 28.65: 500 (1 - cos(28.65 x 126 / 500)) = 3.96, not 3.94. | The example's direction is used; 3.96 recorded. |
| S18 | Table 6-12 (p. 110, PDF 119) | Titled 'based on stopping sight distance'; it is the passing sight distance table. | Recorded. |
| S19 | Table 6.20 (p. 129, PDF 138) | The 5 % downgrade column equals the manual's own formula for a 4 % downgrade in all ten rows. | Column withheld. |
| S20 | Table 6.20 (p. 129, PDF 138) | Upgrade 3 %, 60 km/h (assumed 55): printed 71.6; the formula gives 71.3. | Printed value kept. |
| S21 | Tables 6-17, 6-18 (p. 125, PDF 134) | Titled 'Maximum Gradients (Desirable)' and 'Absolute Maximum Gradients' (copied from Tables 6-14, 6-15); they hold climbing lane thresholds and tapers. | Recorded. |
| S22 | Section 6.2.3, the second item numbered vii, Shoulder Superelevation (p. 96, PDF 105) | 'Figure 6-8 depicts shoulder superelevation rates'; the figure is 6-20 (Figure 6-8 is the elements of a circular curve). | Recorded. |
| S23 | Section 6.7.3 (p. 132, PDF 141) | The relation for the opposing vehicle's distance d4 is printed 'd3 = 2/3 d2'. | d4 = 2/3 d2 used. |
| S24 | Worked example on minimum radius (p. 79, PDF 88) | States V 80 km/h, emax 7 %, f 0.14 and asks for the minimum radius but prints no answer; the formula gives 239.97 m, the Table 6-1 row (240 m). | Recorded. |
| S25 | Tables 6-5 and 6-9 (p. 99 and 103, PDF 108 and 112) | For a normal crown of 4 % the tables print RC wherever the rate is not above 4 %, but the cells V 40 R 60 (Table 6-5) and V 90 R 1000 (Table 6-9) print 4.0 % (with L 22 and 50, equal to the RC lengths). | Printed cells kept. |

#### What was checked, and the worked examples

The manual's worked examples that the implemented parts can reproduce are reproduced in
`tests/mowt_horizontal.rs`. The manual has **no** worked examples for stopping or passing sight
distance, curve widening or vertical curves; those parts are checked by recomputing the printed
columns from the manual's formulas instead.

| Example | Where | Reproduced |
|---|---|---|
| Minimum radius, V 80, emax 7 %, f 0.14 (no answer printed, S24) | 6.2.3, p. 79 (PDF 88) | 239.97 m, the Table 6-1 row 240 m |
| Curve elements: D 4 deg, R 286.479, T 59.07, L 116.50, PC 4+994.8, PT 5+111.3, E | 6.2.3, p. 83 (PDF 92) | T, L, PC, PT yes, also through `HorizontalDesign`; E printed 4.55 follows the givens' 20 deg 18' 02" (S14) |
| Superelevation of a simple curve: T 147.81, Lc 274.674, FSE 4.2 %, Ls 28 m, Lra 16.67, nine stations | 6.2.3 vii, p. 94 to 95 (PDF 103 to 104) | all, to 0.01 m (printed intermediate values are rounded; one printing slip, S16); PC and PT also through `HorizontalDesign` |
| Superelevation with spirals: V^3/432 = 1185.185, Table 6-6 cell 7.0 % and 49 m, theta s 5.615, T 76.045, L 53.188, TS, SC, CS, ST and the runoff stations | 6.2.3 vii, p. 95 to 96 (PDF 104 to 105) | all; L 53.188 only with the printed D 4.5386 (S15); the crate's exact T is 76.12 m |
| Sight distance on a horizontal curve: LS 216.44, M 11.85, M[b] | 6.2.5, p. 105 (PDF 114) | LS and M yes; M[b] 3.96 against printed 3.94 (S17) |

Also recomputed from the manual's own formulas, with the mismatches listed as slips:

| Printed | Recomputed from | Result |
|---|---|---|
| Table 6-1, 40 cells of calculated radius | `V^2 / (127 (e + f))` | all within 0.06 m (rounding of the printed value) |
| Tables 6-4 to 6-9, 1264 cells of minimum runoff length | `max(e x 3.5 x ratio(Table 6-2), 2 s distance)` | all within 1 m except four (S09) |
| Tables 6-11, 6-12, 6-13, computed K | `S^2/404`, `S^2/946`, `S^2/(120 + 3.5 S)` | all within 0.011 |
| Table 6.19 | `0.278 prt V + V^2/(254 f)` | all within 0.2 m, at the assumed (low) and design (high) speed |
| Table 6.20 | the same with grade | all within 0.15 m except the 5 % downgrade column (S19) and one cell (S20) |
| Table 6.21 totals | d1 + d2 + d3 + d4 | all four columns |
| Tables 4-2b and 4-2a | lane x lanes = carriageway; carriageway + shoulders (+ median) = road way width | all seven classes |
| Tables 4-3 to 4-9 against Tables 6-1, 6.19, 6.22, 6-11, 6-12, 6-13, 6-14, 6-15 | the class cell against the speed table | all agree except the withheld cells |
| Tables 6-5, 6-7, 6-9 against 6-4, 6-6, 6-8 | same cells where the rate is above 4 % | all except two (S25) |

#### API examples (for wiring into the app)

```rust
use afrobim_design::mowt::{self, GradeLimit, MowtOptions, RoadClass, Terrain};
use afrobim_design::*;

// 1. What the "load limits" window can offer: built-in bases with classes, terrains, speeds, emax.
for b in builtin_bases() {
    println!("{}: {}\n{}", b.id, b.name, b.source);                       // "mowt_2010", edition, third-party copy
    for c in &b.classes {                                                  // c.name "II Paved", c.traffic_kpcu_per_day
        for t in &c.terrains {                                             // t.name, t.max_design_speed_kmh, t.speeds_kmh
            println!("{} {}: up to {} km/h {:?}", c.name, t.name, t.max_design_speed_kmh, t.speeds_kmh);
        }
        for (terrain, why) in &c.refused_terrains { println!("{terrain:?} not offered: {why}"); }
    }
    println!("emax: {:?}", b.emax_percent_options);                        // [4.0, 6.0, 7.0, 8.0]
    for n in &b.not_checked { println!("{}: {}", n.check_id, n.reason); } // what stays "not checked"
}

// 2. Build the basis for a selection and run the checks exactly as for a user table.
let sel = BuiltinSelection { class: RoadClass::II, terrain: Terrain::Rolling, design_speed_kmh: 70.0, emax_percent: 7.0,
                             options: MowtOptions { grade: GradeLimit::Desirable, minimum_grade_in_cut: false } };
let basis = DesignBasis::from_builtin("mowt_2010", &sel)?;                // or DesignBasis::mowt_2010(class, terrain, 70.0, 7.0)
let row = &basis.rows[0];                                                 // label "II Paved, Rolling, 70 km/h, emax 7 %"
let report = run_checks(&basis, row, Some(&built.horizontal), built.vertical.as_ref(), &CheckOptions::default());
for i in &report.items { println!("{} {} {:?}: {}", i.id, i.location, i.status, i.limit_basis); }
// limit_basis: "table value (minimum radius); source: MoWT Road Design Manual Vol. I Geometric Design (January 2010),
//               Table 6-1, section 6.2.3, p. 80 (PDF p. 89) (rounded radius; ...)"

// 3. A user table still wins when the user chooses it: load it as before (from_csv_str, from_json_str),
//    or let it override single limits of the built-in row.
let user = DesignBasis::from_csv_str(&std::fs::read_to_string("limits.csv")?)?;
let merged = row.overlay(&user.rows[0]);                                  // the user's values replace the manual's

// 4. Lookups, each with its citation; a refusal is an Err with a plain message.
let r = mowt::horizontal::min_radius(80.0, 7.0)?;                         // Cited<MinRadiusRow>
println!("{} m, f {} ({})", r.value.rounded_radius_m, r.value.limiting_f, r.source.citation());
let se = mowt::horizontal::superelevation(7.0, 2.5, 80.0, 250.0)?;        // 7.0 %, runoff 49 m, Table 6-6 p. 100 (PDF p. 109)
let k = mowt::vertical::crest_k_stopping(100.0)?;                         // K 62 to 105 (Table 6-11)
let w = mowt::horizontal::curve_widening(45.0, mowt::horizontal::WideningLanes::Two)?;   // 1.2 m (Table 6-10)
let ssd = mowt::sight::stopping_sight_distance(80.0)?;                    // 112.9 to 139.5 m (Table 6.19)
let lane = mowt::cross_section::lane_width_m(RoadClass::II);              // 3.0 m (Table 4-2b)
match mowt::horizontal::min_radius(75.0, 7.0) { Err(e) => println!("{e}"), Ok(_) => {} }
for s in mowt::known_slips() { println!("{} {}: {}", s.id, s.place, s.what); }
```

The app's `NO_LIMITS_TEXT` in `afrobim-app/src/design_glue.rs` ("The Uganda MoWT ... could not be obtained")
and the README text it mirrors are out of date once this basis is offered; this crate does not touch the app.

#### Limits of this work

* The manual copy is third party and was not compared with a Ministry original; the edition is the one the
  document gives itself (January 2010).
* Values were read from the PDF by text extraction and by eye from rendered pages. The superelevation
  tables (6 x 340 cells) were taken from the text layer; the rendered pages of Tables 6-4, 6-6 and 6-8 were
  compared with it and the other three tables were checked against these by the cross-table test and, for
  the runoff column, against the formula. A misread rate in a cell of Tables 6-5, 6-7, 6-9 that is also
  consistent with its sister table would not be caught.
* Figures 6-14 and 6-24 to 6-27 are graphs and are not tabulated.
* The `H.spiral_length`, tangent and curve length checks, and the minimum vertical curve length, are not
  checked because the manual gives no single number.
* The class tables are for design speed and terrain; they say nothing of urban roads beyond the
  "Urban/Peri-Urban" column (50 km/h in every class).
* This is an engineering aid, not a certification of any design. The manual says its contents are "partly
  guidelines and recommendations to be considered and partly standards which as a general rule should be
  adhered to" (preamble); judging which is which, and authorising a Departure from Standard, remain with the
  engineer.

### AASHTO A Policy on Geometric Design (Green Book)

No citable, retrievable copy of the tables was found in this work, so none of its values is
included. The same limits file takes AASHTO values the user enters, with the edition, table and
page in `#source`.

## Limits and what is not done

- The design checks are the geometric ones listed above. Not checked by `run_checks` (the `mowt`
  module has lookups for several of them): lateral sight distance on horizontal curves, passing and
  decision sight distance, superelevation rates and runoff lengths (superelevation is not designed
  here), combinations of horizontal and vertical alignment, cross-section elements.
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
