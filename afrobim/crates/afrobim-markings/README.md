# afrobim-markings

Road markings as AfroBIM model elements, to the UK Traffic Signs Manual Chapter 5 (Road
Markings). This first cut does the **longitudinal lines** and stops there: centre lines, lane
lines, warning lines, edge of carriageway lines and double white lines, laid out along an
alignment, drawn in plan, given thin solids for the 3D site view, checked against the manual's
tables and measured.

## Source edition

**Traffic Signs Manual, Chapter 5: Road Markings, 2018** (TSO, first published 2019,
ISBN 978 0 11 553208 5), the current edition, written for TSRGD 2016. It was downloaded from the
official source and read as text, and the pages holding every figure and table used were rendered
and checked by eye:

`https://assets.publishing.service.gov.uk/media/5c4ace6ded915d38a0611abc/traffic-signs-manual-chapter-05.pdf`
(linked from `https://www.gov.uk/government/publications/traffic-signs-manual`; 142 pages).

The owner later supplied the same edition as two PDFs (sections 1 to 7, 89 pages; section 8
onward, 53 pages). Their extracted text is byte-for-byte identical to the download's, so the
citations hold for either copy.

The 2003 edition in `Manuals/tsm-chapter-05.pdf` is superseded and was **not** used; no number
here was taken from it or compared with it. The PDF of the 2018 edition is not in this
repository. TSRGD 2016 itself was not read for this crate: diagram numbers and Schedule
references are quoted as Chapter 5 prints them.

No dimension is from memory. Every number in `catalogue.rs` carries its figure, table or
paragraph, and the tests assert each one against that citation.

## The catalogue

Dimensions in mm. "Speed" is the speed limit in mph, as the manual states it.

| Type (id) | Diagram (TSRGD) | Speed | Mark | Gap | Widths prescribed | Citation |
|---|---|---|---|---|---|---|
| Centre line (`1008`) | 1008 (S11-4-6) | 40 or less | 2000 | 4000 | 100, 150 | Figure 2-1, Table 2-1, 2.2.1 |
| Centre line (`1008.1`) | 1008.1 (S11-4-7) | over 40 | 3000 | 6000 | 100, 150 | Figure 2-1, Table 2-1, 2.2.1 |
| Lane line (`1005`) | 1005 (S11-4-4) | 40 or less | 1000 | 5000 | 100, 150 | Figure 2-3, Table 2-2 |
| Lane line (`1005.1`) | 1005.1 (S11-4-5) | over 40 | 2000 | 7000 | 100, 150 | Figure 2-3, Table 2-2 |
| Warning line (`1004`) | 1004 (S11-4-2) | 40 or less | 4000 | 2000 | 100, 150 | Figure 2-4, Table 2-3, 2.4.3, 2.7.1 |
| Warning line (`1004.1`) | 1004.1 (S11-4-3) | over 40 | 6000 | 3000 | 100, 150 | Figure 2-4, Table 2-3, 2.4.3, 2.7.1 |
| Edge line, short broken (`1009A`) | 1009A (S11-4-8) | any | 600 | 300 | 100, 150, 200 | Figure 2-6, Table 2-5, 2.5.3 |
| Edge line, broken (`1010`) | 1010 (S11-4-10) | any | 1000 | 1000 | 100, 150, 200, 250, 300 | Figure 2-6, Table 2-5, 2.5.4 |
| Edge line, continuous (`1012.1`) | 1012.1 (S11-4-11) | any | continuous | | 100, 150, 200 | Figure 2-6, Table 2-5, 2.5.6 |
| Double white lines, both continuous (`1013.1A`) | 1013.1 (S9-6-23) | any | continuous + continuous | | each line 100 minimum, 150 normal | Figure 3-1 version A, 3.2.1 |
| Double white lines, one broken (`1013.1D-L`, `1013.1D-R`) | 1013.1 (S9-6-23) | any | broken line 1000 | 5000 | each line 100 minimum, 150 normal | Figure 3-1 version D, 3.2.1 |

Double lines (3.2.1): the two lines are normally 175 apart, minimum 90; both lines and the gap
must not exceed 900 overall. `-L` / `-R` says which line is broken, looking up-station.

All of these are white. See "Ambiguities" for what the chapter does and does not say on colour.

### Width by speed limit and road layout

The rows of the manual's tables (`applications()`):

| Table | Diagram | Speed | Width | Road layout or use |
|---|---|---|---|---|
| 2-1 | 1008 | 40 or less | 100 | Two-lane roads, not less than 5.5 m in width |
| 2-1 | 1008 | 40 or less | 150 | Two-lane roads 10 m or more; three lanes marked 2+1; four lanes or more |
| 2-1 NOTE 1 | 1008 | over 40 | 100 | Two-lane roads where the 85th percentile speed is generally below 40 mph |
| 2-1 | 1008.1 | over 40 | 100 | Two-lane roads, not less than 5.5 m in width |
| 2-1 | 1008.1 | over 40 | 150 | Two-lane roads 10 m or more; three lanes marked 2+1; four lanes or more |
| 2-2 | 1005 | 40 or less | 100 (150) | Lane line |
| 2-2 | 1005.1 | over 40 | 100 (150) | Lane line |
| 2-3 | 1004 / 1004.1 | 40 or less / over 40 | 100 | Central warning line, two-lane roads less than 10 m wide. Minimum marks: 5 at 30, 7 at 40, 7 over 40 |
| 2-3 | 1004 / 1004.1 | 40 or less / over 40 | 150 | Central warning line, two-lane roads 10 m or more wide. Minimum marks: 5, 7, 7 |
| 2-3 | 1004 / 1004.1 | 40 or less / over 40 | 150 | Central warning line, four or six-lane single carriageway. Minimum marks: 7, 10, 10 |
| 2-3 | 1004 | 40 or less | 100 | Lane line on approach to roundabouts, signals, minor road at a priority junction. Minimum marks: 5 at 30, 7 at 40 |
| 2-3 | 1004.1 | 50 / 60 / 70 | 100 / 100 / 150 | The same. Minimum marks: 7 |
| 2-3 NOTES 1, 2 | 1004 | over 40 | 100 | Only under the notes' conditions (low traffic speeds; minor road approach) |
| 2-5 | 1009A | any | none given ("See 2.5.3") | Field entrance or exit from a private drive |
| 2-5 | 1010 | 40 or less / 50 or 60 / 70 | 100 / 150 / 200 | Lane diverge or merge, private drive, lay-by; and lane-drop |
| 2-5 | 1010 | all three | 200 | Boundary of an emergency refuge area |
| 2-5 | 1012.1 | 40 or less / 50 or 60 / 70 | 100 / 100 / 150 | All-purpose roads with no hard shoulder or hard strip |
| 2-5 | 1012.1 | 40 or less / 50 or 60 / 70 | 150 / 150 / 200 | All-purpose roads with hard shoulder or hard strip |
| 2-5 | 1012.1 | all three | 200 | Motorways |
| 3.2.1 | 1013.1 | any | 150 (100 minimum) | Double white lines |

Speed thresholds are kept in mph exactly as written and read literally: "50 or 60" means 50 and
60. `kmh()` and `speed_label()` (`"40 mph (64 km/h)"`) are display conversions only.

## The element

Category `marking`, `data` = a `MarkingDefinition`:

| Field | Meaning |
|---|---|
| `line_type` | Catalogue id, e.g. `"1008.1"` |
| `alignment`, `geometry` | Name of the alignment element followed, and a copy of its geometry |
| `start_station_m`, `end_station_m` | Stations on that alignment, m |
| `offset_m` | Offset from the alignment, m, positive right looking up-station. Centre of a single line; middle of the gap of a double line |
| `speed_limit_mph` | The speed limit the diagram and width are checked against |
| `width_mm` | Optional. Width of the line (each line of a double line). Unset takes the table width |
| `separation_mm` | Double lines only, optional: gap between the lines (175 if unset) |
| `colour` | `White` or `Yellow`; every type here is white |
| `material` | Free text, never interpreted |
| `anchor` | `Start` (default) or `End`: which end the pattern is laid from |

The kernel gives a module one element at a time, so the marking carries a copy of the
alignment's geometry. `sync_markings(doc)` refreshes the copies after an alignment is edited or
re-imported.

### Checks (notes on the element)

| Code | Severity | When |
|---|---|---|
| `marking.type.unknown` | Error | `line_type` is not in the catalogue |
| `marking.alignment.missing`, `marking.alignment.empty` | Error | No alignment geometry |
| `marking.stations.outside` | Error | A station is more than 1 mm outside the alignment |
| `marking.stations.order` | Error | End station not after start station |
| `marking.offset.inside_curve` | Error | The line would lie at or beyond the centre of a curve |
| `marking.speed.missing` | Error | Speed limit 0 |
| `marking.width.not_prescribed` | Error | A width that is not one of the diagram's prescribed alternatives |
| `marking.width.required` | Error | No width set and the table gives none (1009A, or no row for that speed) |
| `marking.double.width`, `.separation`, `.overall` | Error | A double line outside the limits of 3.2.1 |
| `marking.width.not_in_table` | Warning | A prescribed width that the table does not give for that speed |
| `marking.width.default` | Warning | No width set and the table's rows differ by road layout; the first row is drawn |
| `marking.speed.diagram` | Warning | A diagram the manual gives for other speed limits (e.g. 1008.1 at 30 mph) |
| `marking.speed.no_column` | Warning | An edge line at a speed limit Table 2-5 has no column for |
| `marking.colour` | Warning | Yellow on a line the manual shows white |
| `marking.speed.exception` | Info | Diagram 1008 or 1004 over 40 mph: allowed only under a table note, which is quoted |
| `marking.width.lane_70` | Info | 100 mm lane line at 70 mph (2.3.5 asks for 150 on dual carriageways and motorways) |
| `marking.double.width_not_normal` | Info | Double line not 150 mm wide |
| `marking.part_mark` | Info | The length ends in a cut mark; its length is given |
| `marking.marks.minimum` | Info | A warning line with fewer whole marks than Table 2-3's minimum |

An element with an Error note has no geometry and is left out of the quantities (and counted as
not measured).

## Layout

The marks follow the alignment at the offset, round curves and spirals, each a square-ended
strip of the line's width.

- **Marks and gaps are measured along the marking itself**, not along the alignment. Between two
  stations the line at offset `o` is `Δstation + o × Δheading` long (heading in radians,
  left-turning positive, offset positive right): exact for straights, arcs and clothoids. On an
  arc that is `Δstation × (R ± o) / R`. The station of each mark end is solved in closed form.
- **Where the pattern starts.** Chapter 5 gives the mark and gap of each diagram and says
  nothing on where the pattern starts or how a remainder is treated. This crate's rule: a full
  mark starts at the start station and modules repeat unadjusted. With `anchor = End` the
  pattern is laid backwards from the end station instead, so a line can finish on a full mark
  (a warning line up to a junction).
- **A part module at the end** is cut square at the station. A cut mark is kept as a *part
  mark*: drawn, included in the painted length, counted separately from whole marks, and
  reported in an Info note. A length that ends in a gap simply ends. Nothing is stretched to
  fit; the laying tolerances of Table 12-2 are not applied.
- A continuous line is one mark from end to end.
- A double line is two strands, each laid by the same rule along its own line.

Drawing: one filled outline per mark (several for a long curved mark), in millimetres, X east,
Y north, the lower-left corner of everything drawn on the origin. The derived values
`plan_origin_x_m`, `plan_origin_y_m` say where that origin is in plan, exactly as
`afrobim-alignment` does for its plan drawing. Curved edges are chords within 1 mm of the true
curve.

Solids: one mesh per line, every mark a closed prism in the same frame, from z = 0 to
`DISPLAY_THICKNESS_MM` (5 mm). **That thickness is for display only**: it comes from no
standard and enters no quantity. The road surface is taken as level ground at z = 0; the
vertical profile is not used.

## Quantities

`MarkingTakeoff::measure(doc.elements())` measures every marking; `markings_to_csv` and
`summary_to_csv` write it out.

| Quantity | How it is computed |
|---|---|
| Overall length | Length of the marking's reference line between its stations: `Δstation + offset × Δheading`. For a double line, the middle of the pair |
| Marks | Whole marks. A continuous line is 1. A double line counts both lines |
| Part marks | Marks cut by the end of the length |
| Painted length | Sum of the lengths of all marks, whole and part, of every line |
| Painted area | Painted length × width (exact for a strip of constant width, on a curve as on a straight) |

Per-marking CSV (`markings_to_csv`), then a `TOTAL` row:

```
mark,line_type,diagram,description,width_mm,colour,material,alignment,start_station_m,end_station_m,offset_m,speed_limit_mph,overall_length_m,marks,part_marks,painted_length_m,painted_area_m2
```

Totals by line type, width, colour and material (`summary_to_csv`), then a `TOTAL` row and a
`NOT MEASURED` row when any marking was skipped:

```
line_type,diagram,description,width_mm,colour,material,markings,overall_length_m,marks,part_marks,painted_length_m,painted_area_m2
```

Lengths and areas are written to 3 decimals; totals are sums of unrounded values rounded once.
Nothing is taken from the drawn chords or the display solids. There are no rates and no costs.

## Using it from the app

```rust
use afrobim_markings::{self as markings, LineGroup, LineType, MarkingDefinition, RoadContext};

// 1. Register the module (category "marking", display name "Road markings").
modules.register(Box::new(markings::MarkingModule));

// 2. Line types for a dialog.
for s in markings::catalogue() {
    // s.id, s.name, s.diagram, s.tsrgd, s.pattern.label(), s.widths_mm, s.speed.label(), s.citation
}
// The manual's rows for a type at a speed limit (widths with their road descriptions):
let rows = markings::applications_for(LineType::Centre1008_1, 60);
// Or the manual's choice of diagram and width for a family, speed and road layout:
let rows = markings::select(LineGroup::Centre, 60, RoadContext::TwoLane);

// 3. Add a marking on an alignment element.
let def = MarkingDefinition::new(LineType::Centre1008_1, 0.0, 850.0, 0.0, 60)
    .with_width(100)
    .with_material("thermoplastic");
let id = markings::add_marking(&mut doc, alignment_id, "RM-001", &def)?; // Err carries the Error notes

// Edit, read back, follow an alignment that changed.
markings::set_marking(&mut doc, id, &edited)?;
let def = markings::marking_of(&doc, id);
markings::sync_markings(&mut doc);

// 4. Quantities.
let takeoff = markings::MarkingTakeoff::measure(doc.elements());
let schedule = markings::markings_to_csv(&takeoff);
let summary = markings::summary_to_csv(&takeoff);
```

`add_marking` puts the element on layer `Road markings` at the sheet origin with a
`SitePlacement` at the plan position of its drawing origin (rotation 0, z 0), and hides that
layer on the design sheet when it creates it, as the app does for alignments. In the 3D site
view the solids go through the same frame transform as a support's solids.

## Ambiguities found, and how each is handled

Nothing below is resolved silently; each is either refused, flagged by a note, or left to the
user.

1. **Pattern start and part modules.** Not stated in Chapter 5. The rule above is this crate's.
2. **Width where a table row offers two.** Table 2-2 gives lane lines as "100 (150)"; 2.3.5
   asks for 150 on 70 mph dual carriageways and motorways. With no width set, 100 is drawn, and
   at 70 mph an Info note quotes 2.3.5.
3. **Width that depends on road layout.** Tables 2-1, 2-3 and 2-5 give different widths by road
   width, number of lanes, hard shoulder. The element has no road-layout field: the width is the
   user's choice (`select` / `applications_for` list the rows for a dialog). With no width set,
   the first row is drawn and a Warning lists the alternatives.
4. **Diagram 1009A has no width by speed.** Table 2-5 says "See 2.5.3", which gives no number.
   A width must be chosen from the prescribed 100, 150, 200.
5. **Speed limits between the table's columns.** Table 2-5 has columns for 40 or less, 50 or 60,
   and 70. A limit of 45 or 65 mph has no column: a Warning, and a width must be set. This
   matters outside the UK: **80 km/h is 49.7 mph and 100 km/h is 62.1 mph, and neither falls in
   a column of Table 2-5.** How Uganda's km/h limits map onto the UK bands is a decision for the
   engineer; this crate does not make it.
6. **Diagrams 1008 and 1004 over 40 mph.** Allowed only under Table 2-1 NOTE 1 and Table 2-3
   NOTES 1 and 2. They are catalogued as exception rows and raise an Info note quoting the
   condition. The 100 mm width and (for NOTE 1) 7 marks of the 1004 exception rows are read from
   the table rows the notes are attached to; NOTE 2 states no number of marks.
7. **Three-lane warning lines.** Table 2-3 has no three-lane row; 2.4.7 says in words that the
   150 mm line is used. There is no catalogue row for it; choose 150 by hand.
8. **Minimum number of marks.** Table 2-3's minimum is "on each approach" and has columns only
   for 30, 40 and over 40 mph (none for 20). The element does not know whether it is an approach,
   so a shortfall is an Info note, not an error, and the lowest minimum among the rows of that
   width is used.
9. **Colour.** Chapter 5 says markings are prescribed in white, yellow and red (1.8.1, 12.5.1)
   and that yellow is the colour of waiting and loading restrictions (12.5.3). It calls the
   double line "white" in words; for diagrams 1004 to 1012.1 white is taken from Figures 2-1 to
   2-6, not from a sentence. Yellow on any type here raises a Warning.
10. **Double line width.** 3.2.1 gives a minimum (100) and a normal width (150), not a list, and
    an overall limit of 900. Any width from 100 up is accepted within that limit; 100 and 150 are
    what a dialog is offered.
11. **Double line position.** 3.2.2 allows the pair symmetrical about the centre or with the
    continuous line along the centre. `offset_m` is the middle of the pair; the second
    arrangement is obtained by setting the offset.
12. **Northern Ireland** (2.1.2, Appendix A) has a stricter rule for the two modules. Not
    modelled.

## Limits

- Horizontal geometry only. No profile, crossfall or superelevation: lengths are plan lengths.
- A marking has one line type, one width and one offset from end to end. Tapers, changes of
  width and lines that move across the carriageway are separate elements or not possible.
- At an angle point in an alignment (elements that do not meet tangentially) an offset line is
  not continuous; the strip simply bridges it. Smooth alignments are unaffected.
- The catalogue checks a marking against the speed limit and the tables. It does not decide
  whether a marking is warranted (visibility distances V and W of Table 2-4, the double white
  line design procedure of 3.9, junction layouts): that is the engineer's judgement.
- Table 12-2 laying tolerances are not applied or checked.
- TSRGD 2016 was not read directly.

## Not done

Listed so that nothing is assumed to be half-built:

- Raised profile edge lines, diagrams 1012.2 and 1012.3 (ribs: 2.6, Figure 2-7), and diagram
  1009B.
- Double white lines version B (hatched, up to 1200 mm) and the change-over version C; diagram
  1013.5; splayed and inclined double lines (3.5, 3.6); deflection arrows.
- Transverse markings: Stop and Give Way lines (Chapter 3).
- Arrows, lane destinations and worded markings (sections 9 and 11).
- Hatching and chevrons (diagrams 1040 to 1042), tapers (Tables 2-6, 2-7).
- Yellow lines and other waiting and loading restrictions; red routes.
- Road studs (section 4; the stud-spacing columns of Tables 2-1 to 2-5 are not catalogued).
- Yellow box junctions, yellow bar markings, traffic calming markings.
- Cycle, bus and tram markings.
- Junction and roundabout layouts (sections 5 to 7).
- The Uganda Traffic Signs Manual's markings.
- Rates and costs.
