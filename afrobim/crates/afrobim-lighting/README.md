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
| **BS 5489-1:2003** incorporating Corrigendum No. 1, *Code of practice for the design of road lighting, Part 1* | **2003: superseded** | the owner's copy (not in the repository; copyright BSI) | Table 2, clauses 5.2.1.2, 5.2.3.3, 6.4.4, Tables B.1 to B.5, Table D.1. Module `bs5489_2003` |
| MoWT *Road Design Manual Volume 1: Geometric Design* | January 2010 | uace.or.ug | Section 12.8 "Lighting" (pages 259 and 260). Secondary reference only |
| TII DN-LHT-03038 *Design of Road Lighting for the Strategic Motorway and All Purpose Trunk Road Network* | February 2012 (the NRA addendum to UK TD 34/07 of August 2007, which TD 501 has since replaced) | the owner's copy | Paragraphs 2.9, 2.15. Irish; secondary reference only |

Only the core DMRB documents were read, not their National Application Annexes.

> **BS 5489-1:2003 is not the current edition.** It was replaced in 2013 and again in 2020.
> It is used because it is the edition the owner holds. The later editions select lighting
> classes differently and use the M, C and P class names of BS EN 13201-2:2015 instead of the
> ME, CE and S names of 2003; their clearances and maintenance factor guidance may differ too,
> and Table D.1 predates LED luminaires. **Confirm every value taken from it against the
> current edition.** Every report line that uses it says "superseded edition", the report opens
> with a warning, and all of its data is in the one module `bs5489_2003` so that a later
> edition can replace it.

TD 501 hands the lighting class (clause 3.6), the performance values (2.1), the maintenance
factor (3.8), conflict areas (3.10) and, with CD 377, the column off-set (3.4) to BS 5489-1,
BS EN 13201-2, ILP PLG02, ILP TR30 and BS EN 12767, and reproduces none of them. Of those only
BS 5489-1:2003 has been read. **BS EN 13201-2 has not been supplied, and BS 5489-1:2003 does not
reproduce it**, so this crate contains:

- no luminance, uniformity, threshold increment, surround ratio or illuminance value for any
  lighting class: compliance with a class cannot be shown here;
- no spacing, mounting height or spacing-to-height rule (BS 5489-1:2003 tabulates none; they
  come from the photometric calculation);
- no M, C or P class selection (current editions);
- no passive safety class by speed (ILP TR30 and BS EN 12767 are not held).

The lighting class stays a label the engineer enters. Where a check needs a current figure the
engineer enters it with its source and the report says whose figure it is. A missing input gives
an `INPUT NEEDED` line, never an assumed value.

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
| `bs5489_2003::TABLE_2_CLEARANCE`, `min_clearance_m` | Edge of carriageway to face of column: 0.8 m at 50 km/h, 1.0 m at 80, 1.5 m at 100, 1.5 m at 120 | BS 5489-1:2003, Table 2 |
| `bs5489_2003::VERTICAL_CLEARANCE_OVER_CARRIAGEWAY_M`, `VERTICAL_CLEARANCE_PEDESTRIAN_M` | 5.7 m; 2.1 m | BS 5489-1:2003, clause 5.2.1.2 |
| `bs5489_2003::BRACKET_PROJECTION_MAX_FRACTION_OF_HEIGHT` | one quarter of the mounting height | BS 5489-1:2003, clause 5.2.3.3 |
| `bs5489_2003::TABLE_B1_COMPARABLE` | ME, CE and S classes of comparable level | BS 5489-1:2003, Table B.1 |
| `bs5489_2003::traffic_route_class` | ME (and S) class by road type and ADT, flow boundaries 40 000, 15 000 and 7 000 | BS 5489-1:2003, Table B.2 |
| `bs5489_2003::conflict_area_class` | ME1 to ME5 give CE0 to CE4 | BS 5489-1:2003, Table B.3 |
| `bs5489_2003::subsidiary_road_class` | S1 to S6 by crime rate, Ra, traffic flow and zone | BS 5489-1:2003, Table B.4 |
| `bs5489_2003::town_centre_class` | CE1 to CE3 by type of traffic, flow and zone | BS 5489-1:2003, Table B.5 |
| `bs5489_2003::luminaire_maintenance_factor` | 36 values, 0.42 to 0.93, by IP2X/5X/6X, pollution and cleaning interval 12, 18, 24, 36 months | BS 5489-1:2003, Table D.1 |
| `TII_BRACKET_PROJECTION`, `TII_MAX_BRACKET_INCLINATION_DEG`, `TII_LUMINOUS_INTENSITY_CLASS` | 1.5 m up to 8 m mounting height, else 2 m; 5°; G6 | TII DN-LHT-03038 (2012), paragraphs 2.15, 2.9 |
| `MOWT_MIN_MOUNTING_HEIGHT_M` | 9 m | MoWT GDM 2010, section 12.8, p. 259 |
| `MOWT_MEDIAN_MOUNTING_HEIGHT_M` | 12 to 15 m | MoWT GDM 2010, section 12.8, p. 260 |
| `MOWT_SETBACK_LOW_SPEED`, `MOWT_SETBACK_HIGH_SPEED`, `mowt_setback_behind_kerb_m` | 0.5 m behind the kerb at 50 km/h or less; 1.2 m or greater at 80 km/h or greater | MoWT GDM 2010, section 12.8, p. 260 |

## The checks

`design::check_lighting(&data, &inputs)` returns `LightingChecks`; `calculation()` gives the
report lines, `passes()` is true when no line fails, `notes()` turns failures and missing inputs
into kernel notes. Each line is `OK`, `FAILS`, `INPUT NEEDED`, `INFORMATION` or `DIFFERS
(reference only, does not govern)`. The third line of every report is the warning that
BS 5489-1:2003 is superseded.

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
| Lighting class suggested for the road type and ADT | TD 501 3.6; BS 5489-1:2003 Table B.2 | superseded and informative: a different entered class is a difference, not a failure |
| Conflict area class from the approach class | TD 501 3.10; BS 5489-1:2003 Table B.3 | as above |
| Column clearance by design speed | TD 501 3.4; BS 5489-1:2003 Table 2 | exact at 50, 80, 100, 120 km/h; can fail; set-back is to the column axis |
| 5.7 m under a luminaire over the carriageway or within the clearance | BS 5489-1:2003 5.2.1.2 | compares the mounting height; can fail |
| Bracket projection up to a quarter of the mounting height | BS 5489-1:2003 5.2.3.3 | recommendation: a difference, not a failure |
| Typical luminaire maintenance factor and its product with the entered lamp factor | TD 501 3.8; BS 5489-1:2003 6.4.4, Table D.1 | information only |
| Bracket projection 1.5 m or 2 m, inclination up to 5°, G6 | TII DN-LHT-03038 2.15, 2.9 | secondary reference; differences never fail |
| MoWT mounting height, median height, distance behind kerb, frangible columns | MoWT GDM 12.8 | secondary reference; differences never fail |

`bs5489_2003` also offers `subsidiary_road_class` (Table B.4) and `town_centre_class`
(Table B.5) for the app to suggest a class; they are not wired into the report.

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

## Differences in the TII document (Ireland)

DN-LHT-03038 is the UK's TD 34/07 (2007) with an Irish addendum. TD 501 replaced TD 34/07 in the
UK, so most differences are between an older and a newer UK text.

| Subject | TII DN-LHT-03038 (2012) | TD 501 v0.1.0 (governs) |
|---|---|---|
| Luminaire inclination | Brackets inclined slightly upwards, not more than 5° (2.15); no direct light above the horizontal (2.2) | Design tilt shall be zero (2.5) |
| Luminous intensity class | Should normally be G6 (2.9) | G4 to G6 (2.5, 6.54) |
| Bracket projection | BS 5489-1 5.2.3.3, and not over 1.5 m up to 8 m mounting height, 2 m otherwise (2.15) | No figure |
| Column distance from carriageway | Not less than Table 2 of BS 5489-1 (3.4 as amended for Ireland; the UK original sent roads of 50 mph or more to TD 19) | CD 377 and BS 5489-1 (3.4) |
| Passive safety | UK paragraph 3.5 (TA 89) deleted for Ireland | BS EN 12767 and ILP TR30 (3.5) |
| Lighting class | Table B.2 of BS 5489-1 with opening-year ADT; ME classes on main carriageways, CE classes in conflict areas, MEW not used unless specified (2.7, 2.8, 3.6) | From BS 5489-1 (current edition) on road hierarchy and traffic data (3.6) |
| Light source | Colour rendering index Ra ≥ 20 (3.2); dimmable electronic control gear where possible (2.17) | LED (3.14); adaptive lighting and CMS (3.7, 3.18) |
| Maintenance factor | BS 5489-1 6.4.4, luminaire factors from Table D.1 (3.9) | In accordance with BS 5489-1 (3.8) |
| Unlit gap | "should not" be less than 4 × SSD (3.19): advice | "shall not" (3.11): a requirement, with SSD one step below desirable minimum |
| Extent of lighting at junctions | Continue 1.5 × desirable minimum SSD beyond tapers and from conflict points (3.16 to 3.18) | BS 5489-1 and ILP PLG02 (3.10); not coded here |
| Ingress protection | Preferably IP6X (2.16) | A design output (6.54) |

The 1.5 × SSD extents of TII paragraphs 3.16 to 3.18 are not coded.

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
10. BS 5489-1:2003 Table 2 gives the clearance to the **face** of the column at four design
    speeds. It is not interpolated, and the set-back here is to the column **axis** with no
    column width modelled, so the true face clearance is smaller by half the column width.
11. BS 5489-1:2003 Table B.2 gives strategic routes "<15 000" and ">15 000": exactly 15 000 ADT
    has no entry, and the report asks rather than choosing.
12. BS 5489-1:2003 Table B.3 lists ME1 to ME5 without the a, b, c suffixes; the suffix is
    ignored. Footnotes b and c of Table B.2 (slip roads, ME3b or ME3c in urban areas) and note 2
    of Table B.4 (traffic calming) are not applied.
13. The 5.7 m vertical clearance of BS 5489-1:2003 clause 5.2.1.2 is to the lowest point of the
    luminaire or bracket; the check compares the mounting height, which is slightly higher.
14. Class names: an entered "M3" will always show as differing from the 2003 table's "ME3a".
    That is the edition gap, not a design fault.

## Not done

- **Photometry**: no illuminance, luminance, uniformity, threshold increment or
  edge illuminance calculation, no IES LM-63 or EULUMDAT reader, no lumen method. The
  photometric file name is a reference only. Compliance with a lighting class cannot be shown
  with this crate.
- The performance requirements of the lighting classes (BS EN 13201-2): not supplied, not
  reproduced by BS 5489-1:2003 or TD 501, so not coded.
- The current BS 5489-1 (2020): class selection, maintenance factors and clearances of the
  current edition.
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
