# afrobim-lighting

Street (road) lighting as AfroBIM model elements: installations along an alignment and single
columns, with layout, a schematic plan and solids, a column schedule, quantities and a
line-by-line report of requirement checks.

**The output is for an engineer to check. It is not a certified design, and it is not a
lighting design at all in the photometric sense: nothing here calculates light.**

## What it does

| Part | What you get |
|---|---|
| `Installation` | Columns between two stations of an alignment: arrangement (single-sided left or right, staggered, opposite, twin central), spacing, carriageway width, set-back, mounting height, bracket outreach and tilt, column type (free text), luminaire data as entered, lighting class as a label. |
| `SingleColumn` | One column at a station and offset, for junctions and conflict areas. Bracket towards the alignment or on an entered bearing. |
| `layout_installation`, `layout_single_column` | Column positions: mark, station (internal, and displayed through `Alignment::station_label`), side, offset, easting, northing, alignment heading, bracket direction(s) towards the carriageway, and the profile level where the alignment has a profile. |
| `LightingModule` (category `lighting`) | Plan `Drawing` (circle for the column, hairline for the bracket, filled rectangle for the luminaire) and `Mesh3` solids (column, bracket, luminaire block) at the entered mounting height and outreach. Validation notes and derived values. |
| `LightingQuantities`, `schedule_to_csv`, `summary_to_csv` | Columns by mounting height and arrangement, luminaires by product, connected load, column schedule CSV. |
| `design::check_lighting` | Requirement checks, each with its clause, like `SupportDesign::calculation` in `afrobim-supports`. |

The geometry is schematic. The column diameter (`DISPLAY_COLUMN_DIAMETER_MM`), the luminaire
block (`DISPLAY_LUMINAIRE_MM`), the bracket section and the plan symbol radius
(`DISPLAY_PLAN_SYMBOL_RADIUS_MM`) are display sizes from no product or standard. Only the
positions, mounting height, outreach and tilt are entered data.

## Governing sources

UK standards are the core. The documents below were downloaded and read on 14 November 2026;
every number in the code that comes from one of them carries the document, version and clause
or table in a comment.

| Document | Version read | Where from | Used for |
|---|---|---|---|
| DMRB TD 501 *Road lighting design* | **Version 0.1.0, September 2025** (supersedes Revision 0, March 2020, which the site now watermarks SUPERSEDED) | standardsforhighways.co.uk, search "TD 501" | Governing. Clauses 1.1, 1.4, 2.5, 3.4 to 3.6, 3.8, 3.11, 3.14, 3.15, 3.19, 5.7, 5.8, 6.47 to 6.54 |
| DMRB TA 501 *Road lighting appraisal* | Revision 0, March 2020 | standardsforhighways.co.uk | Clause 1.1: the need for lighting is established by appraisal |
| DMRB CD 109 *Highway link design* | Version 1.1.0, 30 October 2026 | standardsforhighways.co.uk | Table 2.10, stopping sight distance, called on by TD 501 clause 3.11 |
| DMRB CD 377 *Requirements for road restraint systems* | Version 5.0.1, 27 February 2026 | standardsforhighways.co.uk | Clauses 2.2, 2.3 and 2.6.1 item 16, called on by TD 501 clause 3.4 |
| MoWT *Road Design Manual Volume 1: Geometric Design* | January 2010 | uace.or.ug | Section 12.8 "Lighting" (pages 259 and 260). Local reference only |

Only the core DMRB documents were read, not their National Application Annexes.

**BS 5489-1 and BS EN 13201 are paywalled and TD 501 reproduces no table from them.** TD 501
hands the lighting class (clause 3.6), the performance values (2.1), the maintenance factor
(3.8), conflict areas (3.10) and, with CD 377, the column off-set (3.4) to documents that are
not freely readable (also ILP PLG02, ILP TR30, BS EN 12767). This crate therefore contains:

- no M, C or P lighting class table and no luminance, illuminance, uniformity or glare value;
- no maintenance factor;
- no set-back or clearance figure for columns from a UK document;
- no spacing-to-height rule;
- no passive safety class by speed.

Those are inputs: the lighting class is a label, and where a check needs a figure from a
paywalled standard the engineer enters it with its source and the report says whose figure it
is. A missing input gives an `INPUT NEEDED` line, never an assumed value.

### Citations for values in code

| Constant or function (`design` module) | Value | Source |
|---|---|---|
| `CD109_TABLE_2_10_SSD`, `cd109_ssd_one_step_below_m` | Design speed 120, 100, 85, 70, 60, 50 km/h: desirable minimum SSD 295, 215, 160, 120, 90, 70 m; one step below 215, 160, 120, 90, 70, 50 m | CD 109 v1.1.0, Table 2.10 |
| `TD501_UNLIT_GAP_SSD_MULTIPLE` | 4 | TD 501 v0.1.0, clause 3.11 |
| `TD501_SPARE_SUPPLY_CAPACITY` | 20 % | TD 501 v0.1.0, clause 5.8 |
| `TD501_LUMINOUS_INTENSITY_CLASSES` | G4, G5, G6 | TD 501 v0.1.0, clauses 2.5 and 6.54 item 2 |
| `TD501_RISK_GROUPS` | RG0, RG1 | TD 501 v0.1.0, clauses 1.4 and 3.15 |
| tilt check | zero degrees | TD 501 v0.1.0, clause 2.5 |
| `CD377_RRRAP_SPEED_LIMIT_MPH`, `CD377_RRRAP_AADT` | 50 mph, 5,000 AADT | CD 377 v5.0.1, clauses 2.2 and 2.3 |
| `MOWT_MIN_MOUNTING_HEIGHT_M` | 9 m | MoWT GDM 2010, section 12.8, p. 259 |
| `MOWT_MEDIAN_MOUNTING_HEIGHT_M` | 12 to 15 m | MoWT GDM 2010, section 12.8, p. 260 |
| `MOWT_SETBACK_LOW_SPEED`, `MOWT_SETBACK_HIGH_SPEED`, `mowt_setback_behind_kerb_m` | 0.5 m behind the kerb at 50 km/h or less; 1.2 m or greater at 80 km/h or greater | MoWT GDM 2010, section 12.8, p. 260 |

## The checks

`design::check_lighting(&data, &inputs)` returns `LightingChecks`; `calculation()` gives the
report lines, `passes()` is true when no line fails, `notes()` turns failures and missing inputs
into kernel notes. Each line is `OK`, `FAILS`, `INPUT NEEDED`, `INFORMATION` or `DIFFERS (local
reference; UK governs)`.

| Check | Clause | Kind |
|---|---|---|
| Need for lighting established by appraisal | TD 501 1.1; TA 501 1.1 | entered yes or no |
| Lighting class entered, one per zone | TD 501 3.6, 6.48, 6.50 | label present; selection is the engineer's |
| Unique references for the zone and its columns | TD 501 6.49, 6.52 | exact, within the element |
| Luminaire design tilt is zero | TD 501 2.5 | exact |
| Luminous intensity class G4, G5 or G6 | TD 501 2.5, 6.54(2) | exact on the entered class |
| LED light source | TD 501 3.14 | entered yes or no |
| Risk group RG0 or RG1 | TD 501 1.4, 3.15 | exact on the entered group |
| Column set-back | TD 501 3.4 | compared with the minimum the engineer enters from CD 377 and BS 5489-1 |
| Passive safety | TD 501 3.5; CD 377 2.6.1(16) | entered; a column not certified to BS EN 12767 is reported as a hazard for the RRRAP |
| RRRAP or local risk assessment | TD 501 3.4; CD 377 2.2, 2.3 | exact on entered speed limit and AADT |
| Maintenance factor | TD 501 3.8 | recorded as entered; not verified, not used |
| Unlit gap at least 4 × SSD | TD 501 3.11; CD 109 Table 2.10 | exact; SSD from the table at the design speed or as entered |
| Supply capacity = load + 20 % | TD 501 5.7, 5.8 | arithmetic on the entered powers, luminaires only |
| Design outputs entered | TD 501 6.53, 6.54 | lists what is missing |
| Performance, energy indicators, column structure | TD 501 2.1, 4.2, 6.50, 3.19 | stated as not verified |
| MoWT mounting height, median height, distance behind kerb, frangible columns | MoWT GDM 12.8 | local reference; differences never fail |

## Differences in the MoWT manual

The MoWT manual's section 12.8 is one page of general guidance. It has no lighting classes, no
performance values and no reference to a lighting standard. Where it speaks and TD 501 does too:

| Subject | MoWT GDM 2010, 12.8 | UK (governs) |
|---|---|---|
| Where lighting is provided | All main roads in urban areas; rural roads seldom, except junctions, level crossings, narrow or long bridges, tunnels, sharp curves, roadside activity | By appraisal to TA 501 (TD 501 1.1) |
| Mounting height | At least 9 m; 10 to 15 m frequently used; 12 to 15 m favoured on medians; high mast 30 m | No figure in TD 501: follows from the BS 5489-1 design; heights chosen to be least obtrusive by day (4.9.1) |
| Column set-back | Behind vertical kerbs: 0.5 m at 50 km/h or less, 1.2 m or greater at 80 km/h or greater; nothing between | No figure in TD 501: CD 377 and BS 5489-1 (3.4) |
| Columns as hazards | Frangible columns within the clear zone, but not in densely populated areas with footways | Passively safe structures to BS EN 12767 and ILP TR30 (3.5); non-certified columns are RRRAP hazards (CD 377 2.6.1) |
| Median columns | Dual mast arms, protected by a safety barrier, preferably integral with it on narrow medians | Not addressed in TD 501 |
| Luminaire tilt, light source, glare class | Not addressed | Zero tilt, LED, G4 to G6 (2.5, 3.14) |
| Unlit gaps | Not addressed | At least 4 × SSD (3.11) |
| Ducts | Provide ducts during road construction where lighting is intended later | Electrical design, section 5 (not in this crate) |

## Layout rules

TD 501 says nothing about where the first column stands or how a short last bay is treated (its
only words on geometry are clause 4.4: adopt the geometry of adjacent installations for
consistent maintenance access). The rules are therefore this crate's own, and the choice is
recorded in the element:

- The first column stands at the start station.
- Columns follow at the spacing **measured along the alignment (by station)**. On a curve the
  columns on the inside are closer together than the spacing and those on the outside further
  apart, by the factor (1 ∓ offset / radius).
- `EndTreatment::KeepSpacing` (default): columns at whole spacings only; the remainder before
  the end station has no column and is reported (`remainder_m`, Warning note).
- `EndTreatment::ColumnAtEnd`: one more column at the end station; the short last bay is
  reported (`last_bay_m`, Info note).
- `EndTreatment::EqualiseSpacing`: the smallest number of equal bays no longer than the entered
  spacing, columns at both ends; the actual spacing is reported.
- **Staggered**: the spacing is the distance along the alignment from one column to the next
  one on the other side, so same-side columns are two spacings apart. Confirm that this is the
  spacing your photometric design uses.
- Offsets: the carriageway is taken as symmetrical about the reference line (the alignment, or
  the alignment shifted by `reference_offset_m`). A side column stands at half the carriageway
  width plus the set-back. A twin central column stands on the reference line; for that
  arrangement the carriageway width and set-back are recorded but position nothing.
- Brackets point square to the alignment towards the carriageway. `ColumnPosition::bearing_deg`
  is that direction as a whole-circle bearing clockwise from north.
- Level: the alignment's profile level at the station. It is the centreline level: no
  cross-fall, verge slope or column base level is applied. `None` without a profile.
- Marks: prefix, hyphen, three-digit number, counted along the installation (left before right
  at an opposite pair).

## How quantities are computed

- **Columns**: one per stored position, grouped by entered mounting height and arrangement. A
  separate nominal column height is not modelled.
- **Luminaires**: one per bracket (two on a twin central column), grouped by product name and
  entered power.
- **Connected load (W)**: the sum of the entered luminaire powers. Luminaires only: no cable
  losses, diversity, signs or other equipment, and no spare capacity (the 20 % of TD 501 clause
  5.8 appears only in the check report).
- **Schedule CSV** (`SCHEDULE_CSV_HEADER`): mark, element, station, side, offset, easting,
  northing, level, mounting height, outreach, luminaire, number of luminaires, power; metres to
  3 decimals; a TOTAL line.
- **Summary CSV** (`SUMMARY_CSV_HEADER`): item, description, unit, quantity.
- No rates and no costs. Elements that cannot be read are listed in `skipped`, not counted.

## Using it from the app

```rust
use afrobim_lighting::{add_installation, design, Arrangement, ColumnSpec, EndTreatment, Installation, LightingModule,
    LightingQuantities, Luminaire, Side, schedule_to_csv};

modules.register(Box::new(LightingModule));          // once, with the other modules

let def = Installation {
    reference: "LZ-01".into(),
    alignment: alignment.name.clone(),               // must match the alignment passed below
    start_station_m: alignment.internal_station(1000.0, None)?,   // displayed -> internal
    end_station_m: alignment.internal_station(1600.0, None)?,
    arrangement: Arrangement::Staggered,
    spacing_m: 35.0,
    end_treatment: EndTreatment::KeepSpacing,
    first_side: Side::Left,
    carriageway_width_m: 7.3,
    reference_offset_m: 0.0,
    setback_m: 1.5,
    column: ColumnSpec {
        column_type: "10 m steel, passively safe".into(),
        mounting_height_m: 10.0,
        bracket_outreach_m: 1.0,
        tilt_deg: 0.0,
        luminaire: Luminaire { product: "…".into(), lumen_output_lm: 12000.0, power_w: 96.0, ..Default::default() },
    },
    lighting_class: "M3".into(),
    mark_prefix: "LC".into(),
    first_number: 1,
};
let id = add_installation(&mut doc, &alignment, def)?;           // or add_column(&mut doc, &alignment, SingleColumn { .. })
let data = afrobim_lighting::lighting_of(&doc, id)?;
let report = design::check_lighting(&data, &design::CheckInputs::default()).calculation();
let csv = schedule_to_csv(&LightingQuantities::of_document(&doc));
```

- The element lives in **site coordinates**, like an alignment element: `origin` is (0, 0),
  `site` holds the plan origin of its drawing in millimetres (also in the derived values
  `plan_origin_x_m`, `plan_origin_y_m`), rotation zero. The app hides the alignment layer on the
  design sheet; do the same for `afrobim_lighting::LAYER` ("Street lighting").
- Solids are in the same frame with z the level in millimetres, so in the 3D site view they are
  placed at `site.x`, `site.y`, `site.z` (zero) with no rotation, as supports are.
- A module sees only its own element, so the positions are stored in the element. After the
  alignment or the definition changes call `refresh_layout(&mut doc, id, &alignment, None)` (or
  pass `Some(new_definition)`).

## Limits

- Stations must lie on one alignment. An installation does not follow ramps or change alignment.
- The carriageway is symmetrical about one reference line with one width over the whole
  installation; widening, tapers and lay-bys are not followed. Split the installation.
- One column specification, one luminaire and one lighting class per installation.
- Columns are not checked against other elements (signs, barriers, structures, overhead lines,
  junction visibility).
- Marks are unique within an element; uniqueness across the document is not enforced.
- The CD 109 table is held for its six design speeds only and is not interpolated.

## Ambiguities in the sources (confirm these)

1. TD 501 clause 3.11 is worded "There shall not be an unlit gap of less than four times the
   stopping sight distance between lit sections." Applied as: an unlit gap must be at least
   4 × SSD.
2. Its note takes SSD from CD 109 "using the one step below desirable minimum distances";
   the row of that name in Table 2.10 is read at the road's own design speed.
3. TD 501 clause 3.15 prints "RG0 or RG01"; read with clause 1.4 as RG0 or RG1.
4. TD 501 defines mounting height above the road surface but clause 6.53 item 2 asks for it
   "above adjacent ground level". One mounting height is held, above the profile level.
5. TD 501 gives the power density indicator and the annual energy consumption indicator the
   same unit and abbreviates the latter "ANCI". Neither is computed.
6. The ILP's Guidance Note 23 says TA 501 was updated in 2025, but the Standards for Highways
   search on 14 November 2026 still returned TA 501 Revision 0 (March 2020) as the published
   document; that is the one read.
7. CD 377 clause 2.6.1 is advice ("should"), and CD 377 applies to motorways and all-purpose
   trunk roads; on other roads the highway authority's own practice applies.
8. The MoWT manual gives a distance behind the kerb for 50 km/h or less and 80 km/h or more and
   nothing between; and it measures from the kerb, whereas the set-back here is from the
   carriageway edge.
9. Staggered spacing: see the layout rules.

## Not done

- **Photometry**: no illuminance, luminance, uniformity, threshold increment or
  edge illuminance calculation, no IES LM-63 or EULUMDAT reader, no lumen method. The
  photometric file name is a reference only. Compliance with a lighting class cannot be shown
  with this crate.
- Energy performance indicators (BS EN 13201-5), energy and carbon assessment, whole-life cost.
- Column and foundation structural design (BS EN 40, CD 354, PD 6547). The wind design in
  `afrobim-supports` is for sign posts to the IHE Sign Structures Guide and must not be used for
  lighting columns.
- Electrical design: cables, ducts, feeder pillars, voltage drop, protection, earthing
  (TD 501 section 5). Only the clause 5.8 arithmetic on luminaire load is reported.
- Tunnels and underpasses, high mast lighting, conflict area extents (ILP PLG02), obtrusive
  light, ecology.
- Road restraint assessment (the RRRAP itself).
- Costs and rates.
- IFC or DXF export of lighting elements.
