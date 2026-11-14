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
| Uganda MoWT *Road Design Manual Volume 3: Pavement Design*, Part I: Flexible Pavements (January 2010) | `design::mowt` | Traffic, classes, catalogue: gives a structure |
| UK DMRB CD 224 *Traffic assessment* and CD 226 *Design for new pavement construction* | `design::dmrb` | Design traffic in full; of the thickness design only what is an equation. **No complete structure**, and from superseded revisions (see below) |
| *AASHTO Guide for Design of Pavement Structures* (1993), flexible | `design::aashto93` | Design equation solved for SN; layered SN check of an entered structure |
| South Africa (SAPEM / TRH4) | none | Not built. `DesignMethod` and `DesignInput` are `#[non_exhaustive]`; a new method is one sub-module, one variant of each and an implementation of `DesignResult` |

Every method gives the same kind of output through the `DesignResult` trait: `calculation()`
(line by line, with references), `warnings()` and `layers()` (a stack for a
`PavementDefinition`, when the method arrives at one). `design::design(&DesignInput)` runs
whichever method the input belongs to; `DesignMethod::all()` lists them for a menu.

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

## Design: `design::dmrb`

Sources obtained, both from `standardsforhighways.co.uk`:

| Document | Revision read | Status |
|---|---|---|
| CD 224 *Traffic assessment* | Revision 0, March 2020 (`/tses/attachments/257e5888-2bfd-492d-92d4-ecf7d40428b0`) | Superseded by Version 0.1.0 (2025) |
| CD 226 *Design for new pavement construction* | Version 0.1.0, 29 November 2021 (`/tses/attachments/9654b4de-efa7-4843-8598-295019387077`) | Superseded by Version 0.2.0 (2025) |

**The current revisions could not be obtained.** The site lists its documents through a search
application; only the attachment addresses of these earlier revisions could be found, and both
PDFs are stamped SUPERSEDED. Everything below is from the revisions in the table, every
calculation carries a note saying so, and the factors must be compared with the current
documents before this method is relied on.

| Item | In code | Source |
|---|---|---|
| Growth factors G, 5 to 40 years: OGV1 + PSV 1.02 ... 1.24; OGV2 1.04 ... 1.54; all commercial vehicles 1.45 at 40 years | `growth_factor` | CD 224 Table 2.12, clause 2.13 |
| G = 1.0 for past traffic | `TrafficInput::past_traffic` | CD 224 clause 2.17 |
| Flow after i years `Fi = F(1 + R)^i`, R = 1.07 %, 2.10 %, 1.54 % | `flow_after_years` | CD 224 Equation 2.14, Table 2.12 Note 2 |
| Wear factors WM and WN for seven classes, two categories and all commercial vehicles | `wear_factor` | CD 224 Table 2.18 |
| Percentage in the heaviest loaded lane P, by lanes and flow | `heaviest_lane_percent` | CD 224 Table 2.21 |
| `Tc = 365·F·G·W·10⁻⁶`, `T = ΣTc·Y·P` | `design_traffic` | CD 224 Equations 2.23a, 2.23b |
| OGV2 not less than 70 % on new schemes (warning) | `design_traffic` | CD 224 clause 2.7 |
| Foundation class 1 to 20 msa, class 2 to 80 msa; EME2 on class 3 or 4; 40 year life; minimum 1 msa; crushed rock or slag above 80 msa | `design` | CD 226 clauses 2.4, 2.5, 2.7, 2.17, 2.19, 2.22 |
| Total asphalt over an HBGM base `H = −16.05(log T)² + 101 log T + 45.8`, 100 to 180 mm, 180 mm from 80 msa, up to 400 msa, rounded up to 5 mm | `asphalt_over_hbgm_mm` | CD 226 Equation 2.24 and Notes 1 to 3 |

**Verified against** the three worked examples of CD 224 (Tables 2.23.1N1, N2, N3: 51, 100 and
157 msa).

**Not done, because CD 226 Version 0.1.0 gives it only as a chart:** the total asphalt
thickness of a flexible pavement with an asphalt base, and the thickness of an HBGM base, are
curves on the nomograph of Figure 2.20 with no equation or table. They are not read off the
figure. `DmrbDesign::layers()` is therefore always `None`: this method gives the design traffic
and the stated requirements, not a structure. Worked example A1.2 (320 mm of asphalt above
80 msa on a class 3 foundation) is a nomograph reading and is not reproduced.

| Where | What | What the code does |
|---|---|---|
| CD 224 Table 2.23.1N1 | The 6-axle articulated line prints Tc = 0.93 msa; its own factors give 0.95, which the printed total of 2.87 also needs | Computes 0.95 |
| CD 224 Table 2.12 | G only at 5, 10 ... 40 years | Other periods need G entered; nothing is interpolated |
| CD 224 Tables 2.4, 2.12, 2.18 | Growth rows are by category, wear rows by class; "3 and 4-axle articulated" is one wear row | A class takes the growth row of its category, as the worked example does |
| CD 224 Table 2.21 | Covers 2 or more lanes; the "4 or more" expressions give 62.2 and 62.4 % either side of 10 500 cv/day | One lane is taken as 100 % with a warning; each expression is used in its printed range |
| CD 226 Equation 2.24, Appendix A1.1 | The equation gives 174.6 mm at 60 msa (175 mm rounded up to 5 mm); the worked example reads 180 mm from the nomograph, whose scale is in 10 mm bands | Returns the equation's 175 mm and prints the difference |
| CD 226 Equation 2.24 | `log` without a base | Base 10, the reading that gives 180 mm at 80 msa |

## Design: `design::aashto93`

The 1993 AASHTO Guide is not free and **was not read**. The method is taken from two documents
that reproduce it:

| Short name | Document | Where from |
|---|---|---|
| FHWA | Federal Highway Administration, *Geotechnical Aspects of Pavements Reference Manual*, FHWA NHI-05-037 (2006), Appendix C "1993 AASHTO Design Method" | `https://www.fhwa.dot.gov/engineering/geotech/pubs/05037/ac.cfm` |
| Notes | T. H. Fadhil and T. M. Ahmed, *Flexible Pavement AASHTO 1993*, postgraduate lecture notes, Department of Civil Engineering, University of Anbar, 30 pages | Supplied by the owner; not in the repository |

The design equation, the structural number expression and the table of standard normal
deviates were compared between the two: **they are identical**, term for term and value for
value.

| Item | In code | Source |
|---|---|---|
| `log10 W18 = ZR·S0 + 9.36·log10(SN+1) − 0.20 + log10(ΔPSI/(4.2−1.5)) / (0.40 + 1094/(SN+1)^5.19) + 2.32·log10(MR) − 8.07`, MR in psi | `log10_w18` | FHWA Eq. C.2; Notes Eq. 4.1 |
| Solved for SN by bisection between 0 and 30 | `required_structural_number` | FHWA C.2 and C.4 |
| Reliability to ZR, 18 listed values, no interpolation | `standard_normal_deviate` | FHWA Table C-3 (reproduced in the Notes) |
| `SN = a1·D1 + a2·D2·m2 + a3·D3·m3`, D in inches | `structural_number` | FHWA Eq. C.3; Notes Eq. 4.15 |
| SN required above each unbound layer from that layer's modulus | `design` | FHWA Appendix C, procedure step 8 |
| `W18 = 365·Σ(AADT·LEF)·G·DD·DL` | `design_lane_esals` | Notes Eq. 4.5 |
| Effective roadbed modulus from relative damage `uf = 1.18×10⁸·MR^−2.32` | `effective_roadbed_modulus_psi` | Notes, "Equation 7.8" |
| `R_stage = R_overall^(1/n)` | `stage_reliability_percent` | Notes, section 4.1.7 |

**Inputs, never defaults:** W18, reliability or ZR, S0, ΔPSI, roadbed MR, and for each layer
its thickness, layer coefficient `a` and drainage coefficient `m`. The engineer enters a
structure; the module checks it and reports the thickness each layer would need. It does not
choose thicknesses and does not round.

**Cited suggestions** (`design::aashto93::suggestions`). Values the two sources print, for an
application to show beside an input, labelled with `FHWA_SOURCE` or `NOTES_SOURCE` (both end
"as reproduced from the AASHTO 1993 Guide"). They are never applied silently:

| Suggestion | Source |
|---|---|
| Reliability ranges by functional class, urban and rural | FHWA Table C-2 (Notes Table 4.4) |
| S0 for flexible pavements 0.35 to 0.50, 0.45 commonly used; po 4.2, pt 2.5 or higher for major highways | FHWA Appendix C, "Reliability" and "Serviceability" |
| AASHO Road Test average layer coefficients 0.44, 0.14, 0.11 | Notes, "Structural Number (SN)" |
| `a2 = 0.249·log10(EBS) − 0.977`; `a3 = 0.227·log10(ESB) − 0.839` | Notes Eqs. 4.17, 4.18 |
| Drainage coefficient ranges by drainage quality and moisture exposure | Notes Table 4.6 |

The layer coefficient charts for asphalt concrete, cement-treated and bituminous-treated bases
(Notes Figures 4.10 to 4.14) are nomographs: those coefficients are inputs only.

**Verified against** FHWA Appendix C (W18 = 11.6 million, R = 90 %, S0 = 0.45, ΔPSI = 1.7,
MR = 7 500 psi: SN = 5.07, SN2 = 2.43, D2 = 14.3 in); the Notes' nomograph example (SN = 5.0)
and Appendix H example (SN = 5.6 read from the chart, 5.55 by the equation); and the Notes'
Example 1 (W18 = 5.35 × 10⁶).

| Where | What | What the code does |
|---|---|---|
| FHWA and Notes | Equation, SN expression and ZR table | No difference found |
| FHWA Appendix C, step 8 | SN above the granular base (EBS = 40 000 psi) printed as 2.62; Equation C.2 with the printed inputs gives 2.74 (SN = 2.62 carries 8.8 million ESALs, not 11.6) | Follows the equation; the test records both |
| FHWA Eq. C.5 | Printed `ΔPSI = pt − po` | ΔPSI is entered as a positive loss |
| Notes, Example 1 | Table gives 2.24 ESALs for 6-axle trucks; the calculation uses 2.44, and the answer follows from 2.44 | Test records both (5.35 and 5.28 million) |
| Notes, Appendix H | Seasons add up to 13 months; effective MR printed as 5.7 × 10³ psi, but the Notes' own relative damage expression gives 5 470 psi for them | MR is an input; the helper returns 5 470 psi |
| Notes Table 4.6 | A scan with no decimal points ("1 40-1 35") | Decimal points restored in the suggestions |
| Units | The Guide works in psi and inches | Thickness entered in mm and divided by 25.4; modulus in psi |

Not done for AASHTO 1993: rigid pavements, overlays, serviceability loss from swelling and
frost heave, minimum layer thicknesses and rounding, CBR to MR correlations, load equivalency
factors by axle type.

## Using it from the application

```rust
use afrobim_pavement::{add_pavement, layers_to_csv, summary_to_csv, PavementDefinition, PavementModule, PavementQuantities, Shoulder, ShoulderLayers};
use afrobim_pavement::design::{mowt, DesignResult};

// Once, with the other modules:
registry.register(Box::new(PavementModule::default()));

// Design (ribbon: Roads > Pavement design). The engineer picks the method:
//   DesignMethod::all()  ->  [MowtRdmVol3, DmrbCd226, Aashto1993], each with name() and reference()
//   design::design(&DesignInput::Mowt(..) | Dmrb(..) | Aashto93(..)) -> Box<dyn DesignResult>
let result = mowt::design(&input)?;            // or the typed dmrb::design / aashto93::design
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
