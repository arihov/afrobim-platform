# afrobim-pavement

Road pavement as AfroBIM model elements: a layer stack along an alignment, its typical
cross-section, 3D slabs, layer quantities, and a pavement structure design check.

**The design output is a calculation for an engineer to check, not a certified design.**

## Two parts

**Modelling** (definition, cross-section, slabs, quantities). Every dimension is entered by the
engineer or written from a design result. The crate draws those dimensions and measures them by
plain geometry. It holds no rates, densities, bulking factors or costs.

**Design** (`design` module). One sub-module per published method. Which method governs a
project is the engineer's choice; the crate does not choose and the methods are not
interchangeable.

| Method | Module | Status |
|---|---|---|
| Uganda MoWT *Road Design Manual Volume 3: Pavement Design*, Part I: Flexible Pavements (January 2010) | `design::mowt` | Implemented: traffic, classes, catalogue |
| South Africa (SAPEM / TRH4) | none | Not built. `DesignMethod` and `DesignInput` are `#[non_exhaustive]`; a new method is one sub-module, one variant of each and an implementation of `DesignResult` |

### Why the MoWT manual first: an assumption

The owner is a highway design engineer at Uganda's Ministry of Works and Transport, so the
Ministry's own manual was taken as the governing basis for the first method. **That choice is
an assumption made when this crate was written, not an instruction from a project.** A project
may require another basis.

## The model

A `PavementDefinition` holds a copy of the alignment (so the element can be drawn and measured
on its own; `sync_alignments` refreshes it), a start and end station, the carriageway width, an
optional offset of the carriageway centre from the alignment, the cross-fall as an entered
value with its shape (crowned, or one plane to the left or right), the ordered layer stack, and
an optional shoulder each side (width, its own cross-fall, and either the carriageway's layers
or a stack of its own).

A `Layer` is a name, a `Material` and a compacted thickness in millimetres. The manual has no
material codes; it names generic classes (Part I Table 5.1 and the chart keys), which are the
named `Material` variants. `Material::Other` is free text. A thickness of zero is a surface
treatment measured by area only (the catalogue's "SD").

Only the horizontal geometry of `afrobim-alignment` is used. There is no vertical profile and
superelevation is not read.

`PavementDefinition::notes` validates the definition (Error notes stop drawing and measuring)
and adds reminders that quote conditions Part I says its structures assume:

| Note | Source |
|---|---|
| Cross-fall below 2.5 % | Part I, section 1.2 (c) |
| No shoulder, or one narrower than 1 m | Part I, section 1.2 (b) |
| Hot mix asphalt surfacing thinner than 40 mm | Part I, Appendix A.4.1 |
| A layer thicker than 200 mm (Info: to be built in lifts) | Part I, section 5.9, construction note |

## Geometry

**Cross-section** (`cross_section`): millimetres, Y up, seen looking up-station. Every layer of
every strip is a filled outline following the cross-fall, to one scale both ways unless a
vertical exaggeration is asked for. Because a 50 mm layer in a 10 m section cannot carry a
label, each distinct stack is drawn again beside the section as a column of boxes whose heights
are the thicknesses times one stated factor, with thickness, name and material beside each. The
lettering is drawn as lines by a small stroke set inside this crate (the draw crate has no text
primitive); the same labels are returned as text for a front end that sets its own type. Fill
colours are a drawing convention only.

**Slabs** (`solids`): one closed `Mesh3` per layer of each strip, in millimetres, unrotated
(+X east, +Y north), the plan bounding box on the origin and the origin's plan coordinates
returned with them. **z = 0 is the carriageway surface at the carriageway centre at every
station.** Curves are followed by chords within 5 mm of the outer edge.

## How each quantity is computed

For a strip between offsets `o1 < o2` (positive to the right looking up-station) from station
`s1` to `s2`, on an alignment of curvature `k(s)` (positive turning left):

```text
area   = ∫∫ (1 + k·o) do ds = (o2 − o1) · (L + ō·Δθ)

L      = s2 − s1                 length along the alignment
ō      = (o1 + o2) / 2           offset of the strip's own centre
Δθ     = ∫ k ds                  change of heading, radians, left positive
volume = area × thickness        compacted, in place
```

A point at offset `o` travels `(1 + k·o) ds` for each `ds` of the alignment, which gives the
first line. Only the integral of the curvature enters, so the result is exact on straights,
arcs and spirals; curvature is linear along every alignment element, so `Δθ` needs no numerical
integration. `L + ō·Δθ` is the length of the strip's own centre line. A strip centred on the
alignment has area `width × L` whatever the curve; a shoulder, or an offset carriageway, does
not: on a 200 m radius a 1.5 m shoulder is 2.1 % larger outside the curve and 2.1 % smaller
inside. The tests check the formula against annulus sectors and against a strip traced point
by point through a spiral.

| Column | Meaning |
|---|---|
| `length_m` | Along the alignment between the stations |
| `strip_length_m` | `L + ō·Δθ` |
| `area_m2` | Plan area. Not developed for the cross-fall (0.03 % at 2.5 %) |
| `volume_m3` | Plan area × thickness; exact for constant vertical thickness |

`layers_to_csv` gives a row per layer of each strip with a `TOTAL` row; `summary_to_csv` gives
road length, then area and volume per layer (same name, material and thickness added over
strips and pavements) and the total volume. No rates and no densities: Part I states none for
pavement layers.

Limits of the geometry: all layers of a strip have the strip's full width with vertical edges
(no widening with depth, no side slopes); every layer follows the surface cross-fall (Part I
Appendix B suggests steeper falls on lower layers; not modelled); at an angle point in the
alignment the wedge is not measured (a Warning says so); an edge may not reach the centre of a
curve (an Error).

## Design: `design::mowt`

Source obtained: *The Republic of Uganda, Ministry of Works and Transport, Road Design Manual,
Volume 3: Pavement Design, Part I: Flexible Pavements*, January 2010, 83 pages, from the Uganda
Association of Consulting Engineers,
`https://www.uace.or.ug/wp-content/uploads/2023/03/Volume-3-Part-I-Flexible-Pavements-Manual.pdf`.
The PDF is not in the repository. Part I adopts the SATCC *Draft Code of Practice for the Design
of Road Pavements* (September 1998, reprinted July 2001) and limits itself to flexible
pavements up to 30 million equivalent standard axles on subgrades of CBR 2 % and more.

| Item | In code | Part I |
|---|---|---|
| Cumulative traffic per vehicle category, one direction: `DT = T·365·((1 + r/100)^p − 1)/(r/100)` | `cumulative_traffic` | Equation 1, section 2.2 (a)(iii) |
| Load equivalency `F = (P/8160)^n` (kg), `(P/80)^n` (kN) | `load_equivalency_kg`, `load_equivalency` | Equation 2 |
| Relative damage exponent n: granular/granular 4, granular/cemented 3, cemented/cemented 4.5, bituminous/granular 4, bituminous/cemented 4 | `PavementType::damage_exponent` | Table 2.2 |
| Load equivalency factors by axle load band, as printed | `table_2_3` | Table 2.3 |
| Design traffic loading from the two directions, by carriageway and paved width | `RoadType` | Table 2.4 |
| Traffic classes T1 to T8: limits 0.3, 0.7, 1.5, 3, 6, 10, 17, 30 million ESAs | `TrafficClass` | Table 2.5 |
| Subgrade classes S1 to S6: CBR 2, 3-4, 5-7, 8-14, 15-29, 30+ | `SubgradeClass` | Table 3.1 |
| Minimum subgrade depth meeting design strength: 250, 250, 350, 450, 550, 650 mm | `SubgradeClass::minimum_depth_mm` | Table 3.3 |
| Dry or wet charts from drainage, maintenance and traffic class | `Climate::chart_set` | Section 4.2, Table 4.1 |
| Catalogue structures, charts D1-D5 and W1-W5 (360 structures) | `catalogue_structure` | Appendix C, pages C-2 to C-11 |
| Nominal material strengths quoted on `Material` | `Material::nominal_strength` | Table 5.1 |
| Design life guidance; design moisture content `0.67·OMC + 0.8` | `design_life_guidance_years`, `design_moisture_content_percent` | Table 2.1; Equation 3 |
| Reminders printed with the result: design period 10 to 20 years; growth 2 to 15 %; under 0.1 million ESAs; lifts over 200 mm; chart notes | `MowtDesign::warnings` | Sections 2.1, 2.2, 2.3, 5.9; chart notes; Table 5.3 |

`design::mowt::design` returns the working per vehicle category, the class pair, the chart and
the structure; `calculation()` sets it out line by line with these references, and `layers()`
gives the structure as a stack for a `PavementDefinition`.

**Inputs, not defaults.** Traffic counts by category and direction; growth rate; design period;
the damage factor of each vehicle category (Part I declines to give any) or its axle loads; the
design subgrade CBR; dry or wet region and the expected drainage and maintenance; the base and
subbase combination.

**Verified against** Appendix D of Part I: 1.95 × 10⁶ ESAs, class T4, subgrade S3, chart W2,
50 / 150 / 150 / 150 mm. The tests also assert every table above and, for each chart, sample
cells, its extent and the sum of all its thicknesses.

**How the catalogue was read.** The charts are raster images (about 98 pixels per inch). Each
cell was read from a 300 dpi rendering and from pixel enlargements; "SD" against "50" was read
a second time for every cell. Nine charts are drawn to scale, and the drawn height of each of
their 324 columns agrees with the sum of the thicknesses read to better than the catalogue's
25 mm step, with one exception listed below. Chart W3 is a redrawn, sharper image that is not
to scale, so it has no such check. The height check confirms a column's total, not the split
between its layers. **A user who relies on a structure should look at the chart.**

### Ambiguities and slips found in Part I

| Where | What | What the code does |
|---|---|---|
| Equation 1 | Printed without brackets: `DT = T * 365 * [1 + r/100]p - 1 / r/100` | The bracketed form above, which reproduces Appendix D. At r = 0 the limit `T·365·p` |
| Equation 1, Appendix D | T is defined as the first-year average, "not the value at opening to traffic"; the example uses "the initial daily number" | T is whatever is entered |
| Table 2.5 | T2 printed "0.3-07"; ranges share their end values | 0.7; a boundary value takes the higher class; 30 is T8 |
| Table 3.1 | Whole numbers only (3-4, 5-7 ...) | A class runs up to the next class's first value: section 3.3 (b) treats CBR 4.5 as S2 |
| Table 2.4 | "Up to twice" for 4.5 m or less, double at 3.5 m or less; 4.5 m, 6.0 m and 2 000 vehicles are each in two rows or none | Factor is an input (default 2) between 3.5 and 4.5 m; boundary widths read with the narrower band; exactly 2 000 uses 90 % with a warning |
| Table 2.3 | Not Equation 2 at mid-band (8 000 kg, n = 4: equation 0.92, table 0.95); kg and kN halves differ | Both offered; a dash is `None` |
| Chart keys D2, W2, D5, W5 and Appendix D | Cemented subbase "7 day UCS 1.5 - 3 MPa"; Table 5.1 and keys D3, W3 give 0.75 - 1.5 MPa for a cemented subbase | Layer names quote the chart's key; a warning states the conflict |
| Chart D3, S2/T4 | Labels 175 / 200 / 225 mm; the column is drawn about 25 mm shorter (W3 has 200 / 175 / 225 in that cell) | Labels kept; a warning travels with the structure |
| Chart W3 key | "Surface dressing or hot mix asphalt as indicated", but every structure is "SD" | As drawn |
| Table 5.3 | Lists "D2 & W3" for granular base / cemented subbase; text above it calls it "Table 6.1"; "< 0.75 million" for class T2, whose limit is 0.7 | Charts D2 and W2 |
| Section 2.2 | Refers to "2.3(a) and (b)" for what are 2.2 (a) and (b) | None needed |
| Appendix D | Lime-treated subbase at class T4; Table 5.3 limits lime-treated subbases to T2 | The Table 5.3 limits are printed as a warning |
| Section 1.2 and Appendix B | Minimum cross-fall 2.5 %; Appendix B says "about 3 per cent" is suitable | The reminder uses the 2.5 % minimum |
| Table 5.2 | Layer coefficients and `DSN = a1·h1 + a2·h2 + a3·h3`, but no required structural number (refers to the 1989 SATCC guide) | Not implemented |

## Using it from the application

```rust
use afrobim_pavement::{add_pavement, layers_to_csv, summary_to_csv, PavementDefinition, PavementModule, PavementQuantities, Shoulder, ShoulderLayers};
use afrobim_pavement::design::{mowt, DesignResult};

// Once, with the other modules:
registry.register(Box::new(PavementModule::default()));

// Design (ribbon: Roads > Pavement design):
let result = mowt::design(&input)?;            // or design::design(&DesignInput::Mowt(input))?
let lines: Vec<String> = result.calculation(); // show or print
let layers = result.layers();                  // None when the manual gives no structure

// Model (ribbon: Roads > Pavement):
let mut def = PavementDefinition::new(alignment.clone(), 7.0, 2.5, layers.unwrap());
def.left_shoulder = Some(Shoulder { width_m: 1.5, crossfall_percent: 4.0, layers: ShoulderLayers::SameAsCarriageway });
let id = add_pavement(&mut doc, "PV-001", &def)?;   // sets width, height and the site placement

// 3D site view: Generated.solids, placed at element.site (x, y in mm, z = 0, no rotation).
// Sheet: Generated.drawing at element.origin. Notes and derived values as for any element.

// After an alignment is edited:  afrobim_pavement::sync_alignments(&mut doc);
// Schedules:
let q = PavementQuantities::measure(doc.elements());
let (schedule, summary) = (layers_to_csv(&q.rows), summary_to_csv(&q));
```

## Not done

- Rigid pavement thickness design, gravel wearing course design, overlay and rehabilitation
  design (Parts II, III and IV of Volume 3 exist on the same site and were not read).
- Mechanistic analysis; the structural number route of Part I Table 5.2.
- Surfacing design (Part I section 6), materials selection (Appendix A), drainage layer design
  (Appendix B).
- Costs, rates, densities, masses.
- Superelevation, vertical profile, widening on curves, tapers, side slopes, layers of
  different widths.
