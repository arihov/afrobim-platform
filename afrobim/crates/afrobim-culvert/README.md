# afrobim-culvert

A parametric culvert designer. The user's data becomes the road section at the culvert, the
barrel length, the end structures, six drawings, the payable quantities and a bar bending
schedule (BS 8666:2020, BS 4449 masses), so a culvert is a built-in rule and not a heavy CAD
file. Phase 1 is the engine only: no app window. Every design number is an input; the defaults
are those of the Km 39+136 reference drawing and `default_notes()` says which default came from
the drawing, which was back-calculated from it and which is an assumption.

Units: geometry and dimensions in millimetres, levels, chainages and coordinates in metres.

## Use

```rust
use afrobim_culvert::*;

let mut input = CulvertInput::reference();          // 1 x 1200 CPC, Km 39+136
input.barrel = Barrel::Pipe(PipeSpec::catalogue(900.0, 2, 300.0)?);
let design = design(&input)?;                       // CulvertDesign
design.layout;                                      // lengths, inverts, end structures, cover
design.drawings.plan;                               // afrobim_draw::Drawing
design.quantities.lines;                            // category culvert.*, units m, m3, kg, nr
design.bbs.to_csv();                                // the schedule as CSV
```

`CulvertInput` and everything in it is `Serialize + Deserialize`, so the app can store it in an
element's `data`.

## Public API

| Item | Purpose |
|---|---|
| `CulvertInput`, `CulvertInput::reference()`, `validate()` | all inputs; the reference is the drawing |
| `design(&input) -> CulvertDesign` | layout, BBS, quantities, drawings, setting-out, checks |
| `derive`, `schedule`, `measure`, `draw_all`, `setting_out` | the stages of `design`, usable alone |
| `RoadTemplate`, `RoadSection::from_points`, `RoadSection::from_corridor_section`, `RoadSource` | the three ways in for the road section |
| `Ogl`, `RoadSection::toe` | ground line and the embankment toe |
| `Barrel::{Pipe(PipeSpec), Box(BoxSpec)}`, `pipe_catalogue()` | barrels |
| `InvertSpec`, `LengthRule`, `Thresholds`, `Bedding`, `EndStructure`, `ConcreteClasses`, `QuantityOptions` | the rest of the inputs |
| `ReinforcementTable`, `BarRule`, `DimExpr`, `Count`, `Series`, `shape_defs()` | the editable reinforcement table |
| `raw_cutting_length_mm`, `min_bend_radius_mm`, `unit_mass_kg_per_m` | BS 8666 and BS 4449 helpers |
| `plan_solids`, `plan_overlaps` | convex plan polygons of the end structures and the overlap test |
| `select_pipe`, `select_box`, `HydraulicsInput` | auto-select (pipe through `afrobim-drainage`; box is a marked TODO) |
| `default_notes()` | every non-trivial default with its source |

Errors are `CulvertError::{Invalid, NotImplemented, Hydraulics}`; the text says what to change.
Impossible inputs are refused: OGL above FRL at the centreline, negative cover, wings shorter
than the apron, inlet invert not above the outlet, a headwall below the barrel top, a headwall
above the road edge, end structures that would overlap, a cut with no cut slope, and so on.
Failed thresholds (cover, gradient) are reported as `Check`s and do not stop the design.

## Frames

* Road offset: metres from the road centreline, negative left looking up-station.
* Culvert axis: `s` mm along the culvert from where it crosses the road centreline, positive to
  the right-hand end. A road offset is `s x sin(skew)`, so the length along the culvert is
  `road offset / sin(skew)`. Skew is the angle between the axis and the road centreline; 90 is square.
* At each end `o` is the distance outward from the outer face of the headwall and `t` is across
  the culvert.
* Axis bearing (left end to right end) = road bearing + skew.

## Formulas

**Road section.** A template gives, from the centreline outwards on each side, the carriageway
edge (`y = FRL - w x xf/100`), the shoulder edge, and an optional kerb: a vertical step of
`height_step` then the walkway falling at its crossfall. Points may also be typed or handed in
from a corridor section. Levels between points are linear. Outside the last point the surface is
the embankment slope `y = y_edge - d/n`.

**Toe.** Distance `d` out where `y_edge - d/n` equals the ground level (linear between ground
points, held flat beyond the first and last). Where the ground is above the edge the cut slope
`y_edge + d/m` is used and a missing cut slope is an error.

**Barrel length (from the embankment).** Each end is where the embankment slope meets the level of
the top of the headwall there: `top = invert_end - apron_top_below_invert + headwall_above_apron`;
road offset `x = edge_offset +/- n (edge_level - top)`; length `|x| / sin(skew)`. The left and
right lengths are reported separately. With the inverts given at the ends the lengths follow in
one pass; with a centre invert and a gradient they are iterated to 1e-6 mm. A fixed total (split
equally) and fixed left and right lengths are alternatives. A headwall top above the road edge
is refused.

**Inverts.** `invert(s) = invert_CL - flow_dir x g x s`. With the end inverts given,
`g = (inlet - outlet) / length`. Cover is the finished road level less the top of the barrel
(`invert + ID + wall` for a pipe, `invert + height + top slab` for a box); the minimum is taken
at every road section point inside the barrel. Cover at the centreline negative is an error;
cover and self-cleansing gradient are checked against `Thresholds`.

**Headwall width.** `cells x OD + (cells - 1) x spacing + 2 x surround + extra` (extra 50 mm in the
drawing: 1850 against 1800). A box uses `cells x span + (cells + 1) x wall` for the barrel width.

**Wings.** The water face leaves the headwall corner at the splay angle `phi` to the axis for the
length `Lw`: reach `Lw cos(phi)` along the axis, spread `Lw sin(phi)` across. The wing is a
rectangle `thickness` thick with a square end. Overall width of the end structure
`= headwall width + 2 (Lw sin(phi) + T cos(phi))` (6753 in the drawing). Apron width at `o` is
`HW + 2 o tan(phi)`. Apron plus toe is `apron length + toe width` (2310).

**Quantities.** Blinding and bedding: surround width x thickness x barrel length. Surround
(pipe): `(surround width x (OD + over crown) - cells x pi OD^2/4) x length`. Headwall: width x
thickness x height (the barrel opening is deducted only if `deduct_barrel_opening` is set).
Wings: `2 x T x Lw x (h_headwall + h_end)/2`. Apron: thickness x trapezoid area. Toe beam: width
x overall width x (apron thickness + toe depth). End blinding: footprint x blinding thickness.
Weep holes: count x headwall thickness. Excavation: trench (surround width + 2 x working space) x
depth from the ground line to the underside of the blinding, integrated at `sample_step_mm`,
plus the end-structure footprints x depth. Backfill: the trench not filled by blinding, bedding
and surround (or box). Steel: the schedule's mass.

**Quantity categories** (all prefixed `culvert.`): `pipe_length` m (one line per cell),
`box_concrete` m3, `surround` m3, `blinding_barrel` m3, `bedding` m3, `blinding_end` m3,
`headwall`, `wing`, `apron`, `toe` m3, `end_structures` m3 (total), `excavation` m3, `backfill`
m3, `weep_holes` m, `weep_holes_nr` nr, `steel` kg (one line per bar diameter). Lines carry the
concrete class as an attribute. They have the shape of `afrobim-cost` quantity lines; this crate
does not depend on that crate.

## Reinforcement

`ReinforcementTable` is an editable table of `BarRule`s: mark, element, bar size, shape code,
dimensions A to F each as `constant + sum(k x geometry quantity)` (`DimExpr`), the bars per
element (fixed, per cell or at a spacing along a length) and the number of elements. A `Series`
steps dimensions from mark to mark for lettered marks (`01a` to `01l`); `DimRule::PerMark` gives
each mark its own value where the drawing steps irregularly (`05A`, `06B`, `08`, `11`). Dimensions are rounded to
whole millimetres; cutting lengths are rounded to the nearest 10 mm (half up), as every length of
the drawing is a multiple of 10. Weight = number x cutting length x unit mass
(Y8 0.395, Y10 0.617, Y12 0.888, Y16 1.579, Y20 2.466, Y25 3.854 kg/m).

Minimum bend radius (BS 8666:2020 Table 2): former 4 d up to Y16, 7 d above; `r = 2 d` or `3.5 d`.
With `b = 0.5 r + d` (24 mm for Y12). There is no allowance or adjustment table: each cutting
length is the formula of its shape code (`shape_defs()`, `raw_cutting_length_mm`), then rounded
to the nearest 10 mm with halves rounded up (Excel `ROUND`, not banker's rounding). The formulas
were derived from the full reference schedule (`docs/reference/culvert/`), and all 85 of its
rows reproduce.

| Drawing code | BS 8666 shape | Cutting length |
|---|---|---|
| 100 | 00 | A |
| 111 | 11 | A + B - 0.5 r - d |
| 113 | cranked end | A + C |
| 117 | half round, developed length | A (= pi (OD/2 + 14.6) for the half-round bar) |
| 200 | 21 | A + B + C - r - 2 d |
| 202 | U with one angled leg | A + B + C - 0.5 r - d |
| 213 | three straights | A + B + E |
| 500 | 51 | 2 (A + B) + 2 C - 2.5 r - 5 d |
| 900 | four-bend crank | A + B + C + D + E - 4 (0.5 r + d) |

Row masses are rounded to three decimals, as the schedule prints them, so the grand total is
the sum of the printed rows (1498.176 kg for the reference). A dimension that the drawing shows
as a letter (`C`, `D`) is not a length and is left empty.

Bars whose dimensions the geometry explains are `Basis::Geometry`: the toe bar B (overall width
- 2 cover), toe links, headwall L-bars B and their spacing, apron bar 02 and the half-round bars.
Numbers copied from the drawing that nothing explains are `Basis::DrawingFixed` (wing bars,
headwall links and U-bars, the diagonal bar's 350 rise, the additional bars). The 01 series uses
`B = headwall width - 1350` and the 03 series ends at `overall width - 313`: relations assumed
so the bars grow with a wider headwall, not derived from the drawing. The box barrel rows
(`BX1` to `BX4`) are `Basis::Assumed` placeholders pending structural design; no design is done
here. With unequal ends the end-structure marks are scheduled once per end, the right end with
an `R` after the mark.

## The Km 39+136 golden test: what matches

Exact (to the figure printed): headwall 1850 x 2000 (1500 above the apron, 500 below), wings
3267 at 45 degrees (2310 in elevation), overall width 6753 (6753.1), apron plus toe 2310,
the whole bar bending schedule (all 85 rows of the reference CSV: mark, shape, dimensions, cutting length, total number and mass, grand total 1498.176 kg, see below), C20 blinding under the pipe 1.530 m3, C20 bedding 6.120 m3, weep holes 0.8 m,
the centre invert 1107.470 (from the end inverts and the 8347 / 8653 split), and the setting-out
points 1 to 6 within 5 mm from point 1 or from point 2 and the axis bearing (the test uses the
drawing's own split, and separately the computed length).

Within tolerance: barrel length 17 002.7 mm against 17 000 (limit 50); surround 31.976 m3 against
31.817 (+0.5 %, with 500 mm over the crown back-calculated); end-structure concrete 9.440 m3
against 9.646 (-2.1 %, no deduction of the pipe opening); pipe 17.003 m against 17 m.

Documented differences, not forced:

* **Left / right lengths.** We give 8276 left and 8727 right against 8347 and 8653. One side slope
  that gives the 17 000 mm total (1:1.77) cannot give the drawing's 306 mm of asymmetry, which
  alone would need 1:1.2. The drawing used levels or a rule we do not have; `LengthRule::LeftRight`
  reproduces its split.
* **Gradient.** The brief says 1.0 %. The drawing's inverts (1107.595 and 1107.340 over 17 m) give
  1.5 %, and the centre invert 1107.470 agrees with 1.5 %. We report what the levels give.
* **End-structure blinding 7.360 m3.** At 50 mm that is 147 m2; both end structures cover about
  23 m2 (our blinding is 1.14 m3). Unexplained.
* **Steel: drawing and schedule disagree.** The drawing's payable items table (page 3) lists
  1.329 t of steel; its own bar bending schedule totals 1,498.176 kg (169 kg more). The engine
  follows the schedule. Which figure is right is for the owner to confirm.
* **Section D-D.** The drawing uses a different datum (FRL 1092.720, cover 873), so its levels are
  not reproduced; the widths 200 | 100 | 1200 | 100 | 200 and the layers are. Our blinding is as
  wide as the surround (1800), where the drawing shows 1900.
* **Embankment slope** 1:1.77 and the OGL (1107.5 to 1107.9) are back-calculated and read by eye.

## The reference schedule and what the engine generates

`docs/reference/culvert/km39136-1x1200-cpc-bbs.csv` is the full 85-row schedule. For the 1 x 1200
CPC reference the engine generates all 85 rows, in the same order, with the same marks, shape
codes, bar sizes, dimensions, cutting lengths, total numbers and masses (`golden.rs`:
`reference_csv_cutting_lengths_and_masses_are_exact` checks the formulas on the CSV alone;
`the_engine_reproduces_the_reference_schedule` checks the engine against it). No row is missing.
Where the engine differs from the CSV, in presentation only:

* **Bars and elements.** The CSV's two count columns read the other way round from the engine's:
  `bars per element x elements` in the CSV is the engine's `elements x bars per element` (for
  example mark 02: CSV 4 x 18, engine 18 bars in each of 4 elements). The products agree on every
  row. `add4` (20 bars) is split 10 per end by the engine (10 x 2) because one element cannot be
  shared between two ends.
* **Spacing column.** The CSV repeats 200 (or 300, 1000) on most rows; the engine fills the
  spacing only where it drives the bar count (06, 07, 14) or where the drawing prints it.
* **Descriptions** are the engine's own wording.
* **Letters.** The CSV holds `B`, `C`, `D` in some dimension columns (labels of the drawing's
  sketch); the engine leaves those empty.

Which numbers follow the geometry and which are copied from the drawing (`Basis`) is unchanged
for the 01, 02, 07, 13, 14, 15 bars; the lettered series of the wings and the headwall
(04, 05, 06A, 06B, 08, 09c, 09d, 11, 16 and the 03 series for the middle marks) are the
drawing's own numbers, so a culvert of another size gets the same bars there, and a different
design needs those rows edited. The 03 series scales with the overall width.

## Checks and tests

`cargo test -p afrobim-culvert` runs the unit tests of the road and the golden and variant tests:
the reference, 1, 2 and 3 cells of 900 and 1200 pipes, 1, 2 and 3 cell boxes (2.0 x 1.5 m), a 60
degree skew, an asymmetric road with a walkway, unequal ends, every length rule, refusals, drawings'
text, quantity categories, auto-select. Every variant must have no plan overlaps (`plan_overlaps`,
separating-axis test; wings stand on the toe beam so that pair is exempt).

## Drawings

`Drawing`s in millimetres: plan (barrel, surround and excavation lines, headwalls, wings, aprons,
toe beams, rip-rap, road and culvert centre lines, chainage, flow arrow, points 1 to 6),
section A-A (road surface and pavement, embankment, OGL dashed, barrel, bedding, headwalls,
aprons, toe beams, rip-rap at the outfall slope, inverts, gradient, FRL, datum), section D-D,
front elevation, wing wall section C-C and toe beam section B-B. Dimensions are `Line`s plus a
`Glyphs` primitive per text; the text is drawn in this crate's single-stroke font and the
`Glyphs` primitive carries the string so sheet code can use its own lettering. Long sections are
at 1:1 vertical scale from a stated datum.

## Defaults (labelled)

See `default_notes()`. In short: minimum cover 600 mm and working space 300 mm and rip-rap 3000 mm
are assumptions; minimum gradient 1.0 % is the drawing's stated gradient; the pipe walls other than
1200 (100 mm) are indicative and must be confirmed against the supplier's class.

## What the app step needs next

* A window with the inputs above (road source picker, OGL from a surface or typed, barrel and end
  structures, reinforcement table editor, defaults with their source).
* Import of the road section from the corridor or a sample line at a chainage, and the OGL from a
  terrain surface (`afrobim-terrain` sampling along the culvert line), to call
  `RoadSection::from_corridor_section` and `Ogl::from_points`.
* Placement: the culvert point (E, N) and the road bearing from the alignment at the chainage.
* Drawing sheets (`afrobim-sheets`) placing the six drawings and the BBS table; DXF out through
  `afrobim-dxf`; the Glyphs text can be swapped for the sheet's lettering.
* Mapping `culvert.*` quantity lines into `afrobim-cost` rules.
* 3D (`Mesh3` solids of barrel, headwalls, wings, aprons) and IFC export.
* Box culvert hydraulics in `afrobim-drainage`, then `select_box`.
