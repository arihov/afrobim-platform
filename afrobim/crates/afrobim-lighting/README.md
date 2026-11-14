# afrobim-lighting

Street (road) lighting as AfroBIM model elements: installations along an alignment and single
columns, with layout, a schematic plan and solids, a column schedule, quantities and a
line-by-line report of requirement checks.

**The output is for an engineer to check. It is not a certified design. Photometry is computed
only as far as the section "Photometry from luminaire files" says, from first-principles
physics and the user's own grid and inputs; it is not validated against DIALux, Relux or AGi32
(none is available here) and it is not an EN 13201-3 calculation.**

## What it does

| Part | What you get |
|---|---|
| `Installation` | Columns between two stations of an alignment: arrangement (single-sided left or right, staggered, opposite, twin central), spacing, carriageway width, set-back, mounting height, bracket outreach and tilt, column type (free text), luminaire data as entered, lighting class as a label. |
| `SingleColumn` | One column at a station and offset, for junctions and conflict areas. Bracket towards the alignment or on an entered bearing. |
| `layout_installation`, `layout_single_column` | Column positions: mark, station (internal, and displayed through `Alignment::station_label`), side, offset, easting, northing, alignment heading, bracket direction(s) towards the carriageway, and the profile level where the alignment has a profile. |
| `LightingModule` (category `lighting`) | Plan `Drawing` (circle for the column, hairline for the bracket, filled rectangle for the luminaire) and `Mesh3` solids (column, bracket, luminaire block) at the entered mounting height and outreach. Validation notes and derived values. |
| `LightingQuantities`, `schedule_to_csv`, `summary_to_csv` | Columns by mounting height and arrangement, luminaires by product, connected load, column schedule CSV. |
| `design::check_lighting` | Requirement checks, each with its clause, like `SupportDesign::calculation` in `afrobim-supports`. |
| `photometry`, `calc` | IES LM-63 and EULUMDAT readers, flux and peak checks, illuminance on user-defined grids from the luminaires of an element, drawings, CSV, and the settings of a run stored on the element. See "Photometry from luminaire files". |

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
| **Stockmar, *Handbook about Interpretation of EN 13201*** (LiTG, INTERREG Central Europe CE 452 "Dynamic Light", deliverable D.T4.1.4, October 2017, 32 pages) | **Secondary source.** Interprets EN 13201-2:2015, EN 13201-5:2015 and CEN/TR 13201-1:2014 | the owner's copy (not in the repository) | Tables 4.1 to 4.3, 5.1 to 5.3 (pages 4 to 6, 7, 9, 10). Module `en13201` |
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
BS 5489-1:2003 has been read. **BS EN 13201-2 itself has not been supplied, and BS 5489-1:2003
does not reproduce it.** The class values come from a secondary source instead:

> **The owner should confirm every value in module `en13201` against BS EN 13201-2:2015 itself
> (and the selection weights against CEN/TR 13201-1:2014) before it is relied on.** They are
> read from Stockmar's *Handbook about Interpretation of EN 13201* (LiTG, 2017), not from the
> standards. The handbook gives its own table numbers and no EN table number, so every citation
> reads "EN 13201-2:2015 as reproduced in Stockmar, Handbook about Interpretation of EN 13201,
> Table 5.n, p. n", and every report line that uses them carries that warning.

> **BS 5489-1:2003 is still superseded** (see above); that notice stands. Its ME, CE and S class
> names are not classes of EN 13201-2:2015, so an entered "ME3a" is not compared with the
> handbook's tables.

So this crate contains:

- the M, C and P class values of EN 13201-2:2015 as the handbook reproduces them (below), and a
  comparison of results the engineer **enters** (or that `calc` computes, below) with them. The
  class check itself computes no photometry, and a report line about a missing value says so.
  Threshold increment, longitudinal luminance uniformity and wet uniformity are never computed.
  Compliance with a class is shown only as far as the entered or computed results show it;
- no HS, SC or EV class, and no energy performance indicator formula (the handbook gives none);
- no spacing, mounting height or spacing-to-height rule (BS 5489-1:2003 tabulates none; they
  come from the photometric calculation);
- a class suggestion from CEN/TR 13201-1:2014 (weighting parameters), labelled as a suggestion
  from a technical report and never a requirement;
- no passive safety class by speed (ILP TR30 and BS EN 12767 are not held).

The lighting class stays a label the engineer enters; "M3" (any case) resolves to its table row,
while "ME3a" and other labels do not and give an information line. A missing input gives an
`INPUT NEEDED` line, never an assumed value.

### EN 13201-2:2015 as reproduced in the handbook (module `en13201`)

Which edition the handbook interprets: EN 13201-2:2015 (performance requirements), EN
13201-5:2015 (energy performance indicators), and CEN/TR 13201-1:2014 (selection of lighting
classes), with EN 13201-3:2015 and -4:2015 only named. It mentions the 2004 edition of the
technical report only as the superseded first edition.

| Constant | Taken | Handbook |
|---|---|---|
| `TABLE_5_1_M` | M1 to M6: `L_av` 2.00, 1.50, 1.00, 0.75, 0.50, 0.30 cd/m2; `U_o` 0.40 (M1 to M4), 0.35 (M5, M6); `U_l` 0.70, 0.70, 0.60, 0.60, 0.40, 0.40; wet `U_o` 0.15 for all; `TI` 10, 10, 15, 15, 15, 20 %; `EIR` 0.35, 0.35, 0.30 (M3 to M6) | Table 5.1, p. 7 |
| `TABLE_5_2_C` | C0 to C5: `E_av` 50, 30, 20, 15, 10, 7.5 lx; `U_o(E)` 0.40; `TI` 10, 10, 10, 15, 15, 15 % at very high, high or moderate speed and 15, 15, 15, 20, 20, 20 % at low or very low speed (informative) | Table 5.2, p. 9 |
| `TABLE_5_3_P`, `P_E_AV_MAX_FACTOR` | P1 to P6: `E_h,av` 15, 10, 7.5, 5.0, 3.0, 2.0 lx; `E_h,min` 3.0, 2.0, 1.5, 1.0, 0.6, 0.4 lx; `E_v,min` 5.0, 3.0, 2.5, 1.5, 1.0, 0.6 lx and `E_sc,min` 3.0, 2.0, 1.5, 1.0, 0.6, 0.4 lx where facial recognition is necessary; `TI` 20, 25, 25, 30, 30, 35 % (the handbook's addition); P7 "performance not determined"; the average must not exceed 1.5 times the class value | Table 5.3 and its note, p. 10 |
| `selection::weight_*`, `suggest_m`, `suggest_c`, `suggest_p` | CEN/TR 13201-1:2014 weighting values per parameter option; class number is 6 minus the sum (M1 to M6, C0 to C5, P1 to P6; outside that, no class) | Tables 4.1, 4.2, 4.3, pp. 4 to 6, and text pp. 5 to 7 |

Readings to confirm: the handbook does not word the direction of each limit. Luminance,
illuminance, uniformity and edge illuminance ratio are read as minimum maintained values and
threshold increment as a maximum; results are compared as entered, without rounding. The P3
semi-cylindrical value is printed "1,5" and read as 1.5. The P-class threshold increment column
is the handbook's own addition to a table it calls "corrected and modified", so it is reported as
a recommendation (`DIFFERS`), as is the informative C-class threshold increment; the other
criteria can fail. The uniformity limit of 1.5 times is applied as the handbook words it.
Longitudinal uniformity, wet uniformity and edge illuminance ratio are conditional in the handbook
(long uninterrupted road; wet surface with reflectance data; adjacent footway or cycle path
without requirements), so they give an information line when not entered.

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
| Entered `L_av`, `U_o`, `U_l`, wet `U_o`, `TI`, `EIR` against the entered M class | EN 13201-2:2015 as reproduced in Stockmar, Table 5.1, p. 7 | each can fail except as noted; without entry `INPUT NEEDED` (`U_l`, wet `U_o` and `EIR` conditional: `INFORMATION`) |
| Entered `E_av`, `U_o(E)`, `TI` against the entered C class | as above, Table 5.2, p. 9 | `E_av`, `U_o(E)` can fail; `TI` is informative and gives `DIFFERS`, in the column for the entered speed |
| Entered `E_h,av` (and not above 1.5 times), `E_h,min`, `E_v,min`, `E_sc,min`, `TI` against the entered P class | as above, Table 5.3, p. 10 | illuminances can fail; `E_v,min` and `E_sc,min` only where facial recognition is entered as necessary; `TI` is a recommendation: `DIFFERS` |
| Class suggested by the CEN/TR 13201-1 weighting values | CEN/TR 13201-1:2014 as reproduced in Stockmar, Tables 4.1 to 4.3, pp. 4 to 6 | suggestion only: the entered class matching is `OK`, otherwise `DIFFERS`, never a failure; entered only when `class_selection` is set |
| Energy indicators, column structure | TD 501 4.2, 6.50, 3.19 | stated as not verified; no PDI or AECI formula is held |
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

// Entering calculated results (from a photometric calculation done elsewhere) compares them
// with the entered class, e.g. "M3": each line cites the handbook table.
let inputs = design::CheckInputs {
    photometry: design::PhotometricResults {
        source: "DIALux file LZ-01, 12 Nov 2026".into(),
        l_av_cd_m2: Some(1.04), u_o: Some(0.43), u_l: Some(0.61), ti_percent: Some(12.0),
        ..Default::default()
    },
    // optional: a CEN/TR 13201-1 suggestion beside the entered class
    class_selection: None,
    ..Default::default()
};
let report = design::check_lighting(&data, &inputs).calculation();
// en13201::lookup("M3") gives the table row; en13201::selection::suggest_m(&MSelection { .. }).text()
// gives a suggestion with its notice.
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

## Photometry from luminaire files

Added in work package A3. **Read the limits at the end of this section before using a number.**

### What is computed, and what stays an input

| Quantity | Status | Rests on |
|---|---|---|
| Luminous intensity distribution from `.ies` and `.ldt` | computed (read) | the format descriptions cited below, checked on real files |
| Flux by integration over the sphere, light output ratio, downward and upward fractions, peak intensity and its direction | computed | first-principles integration; compared with the file's stated flux and, for EULUMDAT, stated LORL and DFF. **No tolerance is applied**; the difference is reported |
| Horizontal illuminance on a grid: average, minimum, maximum, `U_o = min/avg`, `min/max` | computed | `E_h = I cos^3(gamma) / h^2` per luminaire, summed, times the maintenance factor |
| Vertical and semi-cylindrical illuminance at a user height and facing | computed | derived below from first principles |
| Edge illuminance ratio | computed | its definition in the handbook, section 4.1, p. 8: average horizontal illuminance on a strip just outside the carriageway edge over that on a strip inside the edge, of the same width and "equal to the width of one driving lane"; both sides, the minimum is the value. The strip width is entered |
| Road surface luminance `L_av`, `U_o` | computed **only if the user supplies an r-table file** and an observer | `L = I r / h^2`; no r-table value ships |
| Threshold increment `TI` | **left as entered** | no source for the formula was held or retrieved; the handbook gives only the observer view angles in words (p. 9) and a figure |
| Longitudinal uniformity `U_l`, wet `U_o` | left as entered | need lane-wise luminance and wet r-tables; the standard's rules are not held |
| The EN 13201-3 calculation grid (number and position of points), observer positions | **not implemented** | EN 13201-3 is not held; the handbook and BS 5489-1:2003 give no road grid rule. The grid is **user-defined**. The one grid rule in the held documents is for car parks (BS 5489-1:2003 clause 10.7.5, p. 26: first points within 0.5 m indoors or 1.0 m outdoors of the perimeter, spacing at most 1.0 m or 5.0 m, at least 36 points per grid) and is available as `car_park_grid_limits` for the engineer to apply by hand |
| Maintenance factor | **always the user's input**, in (0, 1] | BS 5489-1:2003 clause 6.4.4, p. 12: the product of the lamp and luminaire maintenance factors; typical luminaire factors in its Table D.1 (the crate's `bs5489_2003` module) |
| Hemispherical illuminance | not implemented | no source |

### File formats: what each field rests on

No standard text of either format is held. The descriptions used were fetched on 14 November
2026; the doc comment of `photometry` (`src/photometry/mod.rs`) lists every field against its
source. In short:

- **IES LM-63** (`parse_ies`): the line layout from the AGi32 Photometric Toolbox "IESNA LM-63
  Format" page and the luxpy reader; the meaning of each field (lamps, lumens per lamp with -1 for
  absolute photometry, candela multiplier, units 1 feet / 2 metres, photometric type 1 = C, angle
  ranges, the horizontal angle symmetry rule) from the Autodesk 3ds Max help "IES Standard File
  Format" (LM-63-1991) and the Lambda Research TracePro knowledge base (LM-63-95 items 3.4 to
  3.13); the 90 to 270 bilateral case from AGi32 "Photometric File Type C - Concepts"; versions,
  `TILT=` record layout, separators and line endings from R. Fritz, "Overview of the IES file
  format" (Radiance forum, 5 Nov 2021). Verified variants: no version line (1986), `IESNA91`,
  `IESNA:LM-63-1995`, `IESNA:LM-63-2002`; other tags (for example the 2019 tag) are read with a
  warning. `TILT=NONE` and `TILT=INCLUDE` (skipped with a warning); `TILT=<file>` is refused.
  **Type A and type B photometry are refused** with a message. Intensity is the candela value times
  the candela multiplier; the ballast factor is reported and applied only if
  `IesOptions::apply_ballast_factor` is set, because no retrievable source says LM-63 wants it
  applied to the candela values.
- **EULUMDAT** (`parse_ldt`): the 30-line table from the AGi32 Photometric Toolbox "EULUMDAT File
  Format" page (citing Stockmar 1990 and 1998) and Paul Bourke's page (April 2021); the planes
  stored per symmetry indicator (Mc1 and Mc2) and the C270-first order of Isym 3 from the DIALux
  evo knowledge base article "Description of the EULUMDAT format" (16 Jul 2021). Intensity in
  candela = value (cd/klm) x conversion factor x total lamp flux / 1000. Lamp sets are read six
  lines per set as real files lay them out (the specification table does not say whether the
  fields of several sets are interleaved); the flux of several sets is added. A negative lamp
  number marks absolute photometry. Non-equidistant C-planes (Dc = 0) are supported: the angle
  list is used.
- **The C-gamma system**: gamma from the nadir, C-planes fixed in space, the tilt during
  measurement declared and not followed by the planes: EN 13032-1:2004+A1:2012 clause 4.2.3 (the
  public preview from iteh.ai; the full standard is not held). The installed tilt is applied
  relative to the tilt during measurement.
- **Symmetry** is expanded by angle (not by position in the list) to the full turn; tabulated C
  angles need not be evenly spaced. Between tabulated angles the intensity is interpolated
  **linearly in C and in gamma (bilinear), which is this crate's choice**: neither format
  prescribes one. Outside the tabulated gamma range the intensity is zero.

### Verification on real files

Real manufacturer and library files are **not committed** (their licences are not known to allow
redistribution). The committed tests use small synthetic files; `real_files_when_present` and
`intensities_and_illuminance_agree_with_luxpy_and_numpy_when_available` run on a folder given by
`AFROBIM_PHOTOMETRY_SAMPLES` (and `AFROBIM_PYTHONPATH` for the Python check) and skip otherwise.

Twenty-one files were read: nine IES (Cooper and Eaton roadway luminaires, an SDL type II with
house side shield, a NAV type III and a GAT post-top; a James M Anderson LED street luminaire from
the ESP-r database; two files from a web viewer's sample set; three luxpy test files) and twelve
EULUMDAT (BHC street light with Isym 2 and Isym 3, a SON-T 250 W road luminaire template with
non-equidistant planes in three copies, Zumtobel, ERCO, a dummy file, a luxpy test file). Flux by
integration at a 1 degree step against the stated light output ratio:

| File | Format | Computed LOR | Stated LOR |
|---|---|---|---|
| BHC street light, luxpy test, Isym 3 | LDT | 100.0 % | 100.0 % |
| BHC street light, luxpy test, Isym 2 | LDT | 100.0 % | 100.0 % |
| SON-T road luminaire, Isym 3, 52 uneven C-planes | LDT | 84.1 % | 84.4 % |
| Zumtobel MIREL, Isym 4 | LDT | 100.1 % | 100.0 % |
| Zumtobel SCON-S, Isym 1 | LDT | 62.1 % | 62.1 % |
| Zumtobel VIVO L, Isym 1 | LDT | 100.2 % | 100.0 % |
| ERCO Trion, Isym 2, uplight | LDT | 40.9 % | 41.0 % |
| luxpy test file as LDT and as its IES twin | LDT / IES | 88.0 % / 88.0 % | 88.0 % / not stated |
| Taman street luminaire | IES | 98.2 % | not stated |

A wrong symmetry expansion or unit conversion would show here; the largest EULUMDAT difference is
0.34 %. Files that did **not** agree, reported as they are: the ESP-r Arran file (number of lamps
-1 and lumens per lamp -2491; it integrates to 3123 lm, 125 % of the 2491 lm taken from the file:
the meaning of a negative flux in an IES file is unclear, and a warning says so), the two luxpy
1986-style test files (106 % and 183 % of the lumens stated; they are synthetic test data), and the
dummy EULUMDAT file with two lamp sets (90.5 % against a stated 100 %; its own text says "can be
used for testing"). The absolute-photometry IES files (lumens per lamp -1) state no flux; the
integrated flux is reported.

**Cross-check against an independent open-source tool.** `luxpy` 1.12.5 (pip, with numpy 2.1.3 and
scipy 1.14.1) reads the same files. For each of nine files, 300 pseudo-random (C, gamma) directions
were compared with this crate's intensities, and the horizontal illuminance at 128 grid points from
one tilted luminaire was recomputed in numpy (an independent implementation of the geometry written
out in `calc/frame.rs` and `calc/engine.rs`, reading intensity through luxpy's map and scipy's linear
interpolation). The worst difference was 3e-15 of the peak intensity and 9e-16 of the maximum
illuminance. Two limits of luxpy were found and are reported: its symmetry completion assumes evenly
spaced planes, so for the SON-T file (non-equidistant C-planes) and for IES files such as the SDL
file, whose horizontal angles include 67.5 degrees but not 112.5, it places planes at the wrong
angles. For those files the comparison used a separate small numpy reader written from the same
cited tables. That part is therefore a check of the implementation against the same documents, not
a second reading of them.

### Coordinates, orientation and the formulae

World: x east, y north, z up, metres. A luminaire stands at its column plus the outreach along the
bracket, at the column's profile level (zero without a profile) plus the mounting height. Its frame
is (street = bracket direction, left, up). For a unit vector `d` from the luminaire to a point with
components `(a, b, c)` in that frame, tilt `t` (street end raised, as `ColumnSpec::tilt_deg`) and
tilt during measurement `m`, with `delta = t - m`: `a_p = a cos(delta) + c sin(delta)`,
`c_p = -a sin(delta) + c cos(delta)`, `gamma = acos(-c_p)`, `alpha = atan2(b, a_p)` and
`C = C_toward + s alpha` (`s` = +1 for C counter-clockwise from above). Hand-worked tests cover the
four directions, both senses of C, tilt and measured tilt, and tilted Lambertian beams.

`Orientation` (which C-plane faces the carriageway, and the sense of C) is a convention the file
does not state. Defaults per format (`Orientation::for_format`): **IES: C0 towards the
carriageway** (AGi32: "front 270 to 90 is the street side"; the real roadway files throw about C0
and the house-side-shielded one sends 83 % of its flux to the C0 side); **EULUMDAT: C270 towards
the carriageway**, C0/C180 parallel to the road (EN 13032-1 note to 4.2.3, preview p. 12; the real
road files put 67 to 74 % of their flux on the C270 side). The sense of C is assumed
counter-clockwise. These are **assumptions to check**: use `polar_diagram` and
`Photometry::flux_fraction_towards_c`, and override `CalcSettings::orientation`.

For a point source of intensity `I` at distance `d` from a point, `u` the unit vector from the
point to the source, `w` its horizontal part and `rho = |w|`:

- `E_h = I u_z / d^2` (equal to `I cos^3(gamma_w)/h^2` for a luminaire at height `h`);
- `E_v = I max(0, u . n) / d^2` for a vertical plane of horizontal normal `n`;
- semi-cylindrical illuminance, **as defined here**: the mean illuminance over the curved surface
  of an infinitesimal vertical half-cylinder facing `n0` (uniform in azimuth),
  `E_sc = I (rho + w . n0) / (pi d^3)`. Derived by averaging `max(0, cos(psi - beta))` over the
  half-circle (`(1 + cos(beta - psi0))/pi`) and tested against numerical integration of the
  half-cylinder. The facing directions are the user's (`Facing`); nothing retrieved says which
  direction a standard uses. The handbook (p. 10) calls 1.5 m "advisable" as the reference height
  for facial recognition; the height is entered, not defaulted;
- luminance `L = I r / h^2` with `r = 10 000 q cos^3(gamma)` (the CIE definition as summarised by
  Corell and Soerensen, "An instrument for the measurement of road surface reflection properties",
  DTU Fotonik and the Nordic road administrations, section 2, which cites CIE 144:2001; **not read
  in the standard itself**). `beta` is taken as the angle between the horizontal directions from
  the point to the luminaire and to the observer, 0 to 180 degrees.

Contributions add. Maintained = initial x maintenance factor.

### The user-defined grid

A `CalcArea` is a rectangle in the alignment's frame (stations along, offsets across, positive
right) divided into equal cells (`Division::Count(n)`, or `Division::Spacing(d)` giving
`ceil(length/d)` cells); **points are the cell centres**, so the average is the mean over the
area. `default_carriageway_area` proposes the carriageway between the first and last column; it is a
convenience, not a rule. The grid is laid out either on the alignment itself
(`GridFrame::Alignment`, following curves) or, without it, as a **local straight approximation**
from the stored layout (`GridFrame::from_layout`: the alignment point of the first column and its
heading). The straight frame is exact on a straight alignment and leaves the carriageway by about
`L^2/(2R)` over a length `L` on a curve of radius `R`; `from_layout` warns when the column headings
differ by more than 2 degrees (a threshold of this crate, not of a standard). The luminaires
themselves are always at their true laid-out positions. Only luminaires within
`contribution_distance_m` in plan of a point contribute to it (a user input; the result says how
many luminaires contribute to no point). The calculation plane is horizontal at `plane_level_m`
(default the mean column level); gradient and cross-fall are not applied.

### r-table file format (no values ship)

`calc::RTable::parse` reads a text file of `key: value` lines (`#` starts a comment): `unit`
(sr^-1 per table unit; CIE tables use 1e-4, from the Corell and Soerensen paper), optional
`q0_table` and `q0_surface` (both or neither; r is multiplied by their ratio, as in the note to
Table C.3 of BS 5489-1:2003, p. 45, "scaled for Q0 = 0.10"), `beta` (0 to 180 degrees,
increasing) and `row: tan_gamma v1 v2 ...` lines (`-` for no value, taken as zero and counted).
The only r-tables held are those of BS 5489-1:2003 Annex C (Tables C.2 and C.3, pp. 44 and 45): a
copyrighted superseded document whose values are not copied here; the engineer can type one into
this format. A contribution beyond the last row is taken as zero and counted in the warnings.

### Using it from the app

```rust
use afrobim_lighting::calc::*;
use afrobim_lighting::design::{check_lighting, CheckInputs, PhotometricResults};
use afrobim_lighting::{lighting_of, load_photometry_file};

// 1. The element, the file and what the engineer enters.
let data = lighting_of(&doc, id)?;
let loaded = load_photometry_file(std::path::Path::new("lantern.ies"))?;     // or .ldt
let settings = CalcSettings {
    maintenance_factor: 0.8,                 // user input, (0, 1]
    contribution_distance_m: 100.0,          // user input
    orientation: Orientation::for_format(&loaded.photometry.format), // check it
    areas: vec![default_carriageway_area(&data, Division::Spacing(3.0), Division::Spacing(1.0))?],
    plane_level_m: None,
    vertical: Some(VerticalSettings { height_m: 1.5, facing: Facing::AlongAlignment }),
    observers: vec![],                       // with an r-table: ObserverSpec { .. }
    eir_lane_width_m: None,                  // Some(lane width) adds the edge strips and the EIR
};

// 2. Keep what repeats the run on the element (path, hash, maintenance factor, grid).
set_photometry(&mut doc, id, "lantern.ies", &loaded, &settings, None)?;

// 3. Run: with the alignment the grid follows curves; None uses the straight local frame.
let run = run_photometry(&data, &loaded.photometry, &loaded.file_name, &loaded.sha256, &settings, None, Some(&alignment))?;
for w in &run.warnings { /* show them */ }
let flux = loaded.photometry.flux_check(1.0);   // flux, LOR, fractions, differences (no tolerance)

// 4. Fill the results and run the class check. Fields not computed keep their entered values.
let area = run.primary_area_for_class(data.definition.lighting_class()).unwrap().to_string();
let mut results = PhotometricResults::default();      // or the dialog's entries
run.apply_to(&mut results, &area)?;                   // sets results.source = "Computed by AfroBIM ..."
let checks = check_lighting(&data, &CheckInputs { photometry: results, ..Default::default() });

// 5. Outputs: drawings in mm plus labels (the front end draws the text), and CSV.
let lums = luminaire_instances(&data);
let contours = contour_drawing(&run.result, &area, &nice_levels(10.0, 40.0, 6), &lums);
let values = grid_values_drawing(&run.result, &area, &lums);
let polar = polar_diagram(&loaded.photometry, &[(0.0, 180.0), (90.0, 270.0)]);
let csv = grid_to_csv(&run.result);

// Later: repeat the stored run (file missing or changed: the embedded copy is used, with a warning).
let again = run_stored_photometry(&doc, id, Some(&alignment), Some(project_dir))?;
```

A complete, compiled example is the doc test in `src/calc/mod.rs`.

`PhotometryRun::apply_to` fills, from the primary area, `e_av_lx`, `u_o_e` (min/avg) and
`e_h_min_lx`; when vertical settings were used, `e_v_min_lx` and `e_sc_min_lx` as the **minimum over
all points and all facing directions** (a conservative rule chosen here, not a standard's); `eir`
when edge strips were calculated; `l_av_cd_m2` and `u_o` only with exactly one observer and an
r-table. It never touches `ti_percent`, `u_l`, `u_o_wet`, `facial_recognition_necessary` or
`low_or_very_low_speed`, so the luminance criteria of an M class stay `INPUT NEEDED` without an
r-table. The `source` string begins with `COMPUTED_SOURCE_PREFIX`; the class check then quotes it
and no longer says "calculated outside this crate".

### Keys stored on the element

One JSON object at `element.data.photometry` (`PHOTOMETRY_KEY`), kept by `refresh_layout`:
`file_path`, `file_sha256` (SHA-256 of the file's bytes), `settings.maintenance_factor`,
`settings.contribution_distance_m`, `settings.orientation`, `settings.areas[]` (name, kind,
stations, offsets, `along` and `across` divisions), `settings.plane_level_m`, `.vertical`,
`.observers`, `.eir_lane_width_m`, optional `rtable_path` and `rtable_sha256`, and `distribution`
(the parsed distribution) **only when it has at most 4000 intensities** (`MAX_EMBEDDED_VALUES`).
Larger distributions are referenced by file: `resolve_photometry` re-reads the file, and warns when
it is missing or its hash differs (then the embedded copy is used if there is one, else the file as
it is, or an error when it is missing). Writing `LightingData::to_value()` over `element.data`
directly drops this key; use `refresh_layout`.

### Limits of the photometry

- **Not validated against DIALux, Relux or AGi32**: none is available here. The checks are the
  analytic tests, flux against the files' stated light output ratios, and the luxpy and numpy
  comparison above.
- **Not an EN 13201-3 calculation**: the standard's grid, observer positions, TI and U_l are not
  held. The numbers answer "what does this grid of points give", not "does this meet the standard's
  method".
- Luminaires are points; a luminous opening of finite size, near-field effects (photometric
  distance) and obstruction are not modelled. Horizontal plane only; no road gradient or cross-fall.
- The orientation defaults are assumptions (above). The tilt during measurement uses the same sign
  as `ColumnSpec::tilt_deg`; not verified from the standard's figure.
- The `TILT=INCLUDE` multipliers of IES (lamp tilt) are skipped. Type A and B IES files are refused.
- Straight local grid frame on curves unless the alignment is passed.
- Intensity is zero outside the tabulated gamma range and interpolation is bilinear (choices
  stated above).
- `E_v,min` and `E_sc,min` take the minimum over the directions calculated; the facing directions
  are the user's.
- The luxpy comparison reads the same documents; it checks the implementation, not the documents.
- The app's text "AfroBIM computes no photometry" (`afrobim-app/src/lighting_inputs.rs`) and the
  matching sentence in `docs/user-manual.md` are out of date once this is wired; they were not
  touched here.

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

15. The handbook's table numbers (4.1 to 4.3 in its chapter 3, 5.1 to 5.4 in its chapter 4) do
    not follow its chapter numbers, and its introduction labels EN 13201-2:2015 "Part 5: Energy
    performance indicators". The tables are cited by the numbers printed on them, with the printed
    page (the PDF page is two higher).
16. The handbook words the C-class threshold increment as an informative criterion and
    recommends the lower-speed column for EN 13201-2:2015 (p. 9); the report asks which column
    applies (`low_or_very_low_speed`) rather than choosing.
17. The handbook's selection text says careful selection gives class numbers 1 to 6 (M, P) and 0
    to 5 (C). A sum outside that range gives no class here instead of clamping to M1, C0 or
    P1 or to the weakest class.

## Not done

- **Photometry**: see "Photometry from luminaire files". Not done: threshold increment, longitudinal
  and wet luminance uniformity, the EN 13201-3 grid and observer rules, hemispherical illuminance, the
  lumen method, any r-table values, a finite-size source model, IES type A and B.
- **Left out of the handbook** because it gives them only as a figure, as loose description or
  not at all: Table 5.4 (classes of comparable level, a grid read against rated luminances; the
  `L_av = q0 x E_av` and `L_av = rho / pi x E_av` relations are stated but not coded); the figures
  (flicker against spacing, stopping distance against speed, threshold increment against
  adaptation luminance); the speed ranges for the selection options (given as examples, section
  7.1, p. 14); the option descriptions (the handbook omits them) and the junction density rule
  (more than three intersections per kilometre, p. 16, reported not coded); the alternative
  selection method of CEN/TR 13201-1 Annex B (described on p. 19, no table); the adaptive
  lighting reductions; the HS, SC and EV classes (not in the handbook); and the energy
  performance indicators of EN 13201-5 (below).
- **Power density indicator and annual energy consumption indicator (EN 13201-5)**: not coded.
  Section 9 (pp. 20 to 21) names the indicators and describes related measures in words (power
  over area, power over illuminance, power over area and illuminance, installed power times
  annual operating time, and their densities) under names and units that are not stated as the
  standard's formulae, and has no worked example. Nothing is taken from memory.
- The current BS 5489-1 (2020): class selection, maintenance factors and clearances of the
  current edition.
- Energy performance indicators (BS EN 13201-5), energy and carbon assessment, whole-life cost.
  (The handbook gives no formula for the indicators: see above.)
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
