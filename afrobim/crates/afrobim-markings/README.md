# afrobim-markings

Road markings as AfroBIM model elements, to the UK Traffic Signs Manual Chapter 5 (Road
Markings). This first cut does the **longitudinal lines** and stops there: centre lines, lane
lines, warning lines, edge of carriageway lines and double white lines, laid out along an
alignment, drawn in plan, given thin solids for the 3D site view, checked against the manual's
tables and measured.

Two parts. The first (this section down to "Limits") is the **longitudinal lines** of the UK
Chapter 5, category `marking`. The second, at the end of this file ("Second set (R2) and the
Uganda basis (R3)"), adds transverse lines, arrows, hatching and chevrons, yellow lines, road
studs and the Uganda Traffic Signs Manual's markings, category `marking_ext`, with a choice of
basis.

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
references are quoted as Chapter 5 prints them. (The second part reads TSRGD 2016 for the
diagrams Chapter 5 does not dimension; see its sources table.)

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
standard and enters no quantity. Where the alignment has a vertical profile the solids lie at
the profile level at each mark's stations (`plan_at_level`, clamped to the profile's ends);
with no profile the road surface is taken as level ground at z = 0. The quantities do not use
the profile.

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
- TSRGD 2016 was not read for the first part; the second part reads the diagrams it names.

## Not done

The first part's list stands for the longitudinal lines (raised profile edge lines 1012.2 and
1012.3, 1009B, double white lines versions B and C, 1013.5, splayed and inclined double lines).
Of the other items once listed here, the second part now covers stop and give way lines, lane and
deflection arrows (lane arrows as placeholder blocks), hatching and chevrons, yellow waiting
lines, road studs and the Uganda manual's markings; what it leaves out is listed there.

Still not done anywhere in this crate:

- Worded markings beyond placeholder blocks, lane destinations (diagram 1035), bifurcation
  arrows, tapers, tramway, cycle, bus markings, red routes, loading restriction lines.
- Yellow box junctions, yellow bar markings, traffic calming markings.
- Junction and roundabout layouts (sections 5 to 7).
- Rates and costs.


---

# Second set (R2) and the Uganda basis (R3)

Added without changing the first catalogue: the longitudinal lines above, their elements, their
quantities and their tests are as they were. Everything below lives in **new elements of category
`marking_ext`** (module `ExtMarkingModule`), so the code that reads `marking` elements
(`MarkingDefinition::from_value` in the IFC export, cost collector and sheets) never sees them.
The app wires them in by registering the second module and calling the new functions; see "Using
it from the app".

## Sources for this part

| Short name | Document | Used for |
|---|---|---|
| UK TSM Ch.5 2018 | Traffic Signs Manual Chapter 5 Road Markings, 2018 (as above; held as two PDFs, printed pages quoted) | Tables 2-1, 2-2, 2-3, 2-5, 2-6, 9-1, 11-2; Figures 2-8, 2-16, 2-17, 9-1, 9-5; paragraphs 2.7.1, 2.11, 3.3, 4.4, 4.5, 6.2.2, 11.2 |
| TSRGD 2016 | The Traffic Signs Regulations and General Directions 2016, SI 2016/362 (`Manuals/TSRGD-2016-uksi_20160362_en.pdf`, 547 pages, printed page numbers) | Diagrams the chapter refers to and does not dimension: 1017 and 1018.1 (Schedule 7 Part 4, p.116), 1002.1 and 1003A and 1023A (Schedule 9 Part 6, pp.147 and 148), 1003.1 (Schedule 11 Part 4, p.223), 1001 (Schedule 14 Part 2, p.468) |
| Uganda TSM 2004 | **Republic of Uganda, Ministry of Works, Housing and Communications, Road Development Program Phase 2, Traffic Signs Manual, Volume 1 and Volume 2 (Appendices), TRL Limited, July 2004, Contract RDP/GN/S008.** Edition confirmed from the title pages of both PDFs. | Vol.1 section E Road Markings (printed pages 34 to 40, Table E.1 on p.40); Vol.2 Appendix 3 section 7 sign pages M01 to M71 (the sign pages carry no page number; they are cited by sign number and by the page of the PDF, pp.161 to 187) |

Dimensions that appear only in a figure were read from the rendered page. Where a figure's
dimension could not be assigned to a feature with confidence, the entry says so in its `notes`
and the value is an **input** of the element, not a catalogue number.

## Basis

```rust
use afrobim_markings::{lookup_family, lookup_type, Basis, Family, Lookup};

// Ask a basis what it defines. No silent fall back.
match lookup_family(Basis::Uganda, Family::WarningLine) {
    Lookup::Found(ids) => println!("{ids:?}"),
    Lookup::NotDefined(n) => println!("{}", n.message()), // "...: not defined in the Uganda manual"
}
let ids = lookup_family(Basis::Uganda, Family::StopLine).found().unwrap(); // ["M31"]
let t = lookup_type(Basis::Uk, "1040"); // Found(TypeRef::Ext(spec))
```

* `Basis::Uk` (UK TSM Chapter 5 2018; speeds in mph) and `Basis::Uganda` (Uganda TSM 2004; speeds
  in **km/h**, the manual's own unit).
* Existing `MarkingDefinition` elements have `basis: None`, which is UK, unchanged. Setting
  `basis = Some(Basis::Uganda)` on one of them is an Error (`marking.basis.not_defined`, "...: not
  defined in the Uganda manual") because the first catalogue holds UK diagrams only.
* **UK value chosen on purpose.** `MarkingDefinition::uk_fallback` and
  `ExtDefinition::with_uk_fallback("reason")` make the UK value usable under the Uganda basis.
  The reason is stored in the element, written as an Info note (`marking.basis.uk_fallback`), shown
  in the derived values (`UK fallback (caller's choice)`) and written in the schedule column
  `uk_fallback`.
* A Uganda sign on a UK element is refused the same way.
* What this crate finds the Uganda manual does not define (`lookup_family` says so): warning
  lines, hatched markings (UK 1040) and double white lines (UK 1013.1). It has its own M05
  zig-zag, M23 no parking line, M53 island marking and yellow dividing lines, which the UK
  chapter does not.
* Speeds: the API takes the number to use in the basis's own unit and a note
  (`with_speed(60, "100 km/h taken as the 60 mph band by project rule PR-3")`). The note is
  written into the derived values (`Speed note (caller)`) and the schedule. This crate does
  not map km/h to mph bands.

## Catalogue added, with diagram numbers and citations

Dimensions in mm. "Placeholder" means a bounding block only (see "Not done and why"). Details,
interpretations and applications are in `ExtSpec::notes` and `ExtSpec::applies`.

### UK (`Basis::Uk`)

| Id | Marking | Dimensions read | Citation |
|---|---|---|---|
| `1017` (S7-4-2) | Single yellow waiting line | width 50, 75 or 100 (an input); 150 from the edge of carriageway to the line (read as clear distance) | TSRGD 2016 Sch.7 Pt.4 item 2, p.116 |
| `1018.1` (S7-4-1) | Double yellow waiting lines | each line 50, 75 or 100; gap 50, 75 or 100 (an input); 150 from the edge | TSRGD 2016 Sch.7 Pt.4 item 1, p.116 |
| `1001` (S14-2-46) | Stop line at light signals | continuous, 200 or 300 wide | TSRGD 2016 Sch.14 Pt.2 item 46, p.468 |
| `1002.1` (S9-6-1) | Stop line at a stop sign | continuous, 400 wide | TSRGD 2016 Sch.9 Pt.6 item 1, p.147 |
| `1003.1` (S11-4-1) | Give way line, roundabout | marks 1000, gaps 1000, width 200 or 300 (300 where the warning line is 1004.1 or a hatch boundary is 6 m and 3 m) | TSRGD 2016 Sch.11 Pt.4 item 1, p.223; UK TSM Ch.5 2018 6.2.2, p.62 |
| `1003A` (S9-6-3) | Give way line, double broken | two rows, each 200 wide, 300 apart; marks 600, gaps 300 | TSRGD 2016 Sch.9 Pt.6 item 3, p.148 |
| `1023A` (S9-6-4) | Give way triangle | base 1250, height 3750, border 150 on the sides, 600 on the base (reading noted) | TSRGD 2016 Sch.9 Pt.6 item 4, p.148 |
| `1022` (S9-6-2) | STOP (placeholder block) | tile widths S 548, T 436, O 624, P 520 = 2128 across; 1600 or 2800 along | UK TSM Ch.5 2018 Table 11-2 p.112, 11.7; TSRGD p.147 |
| `1024` (S11-4-15) | SLOW (placeholder block) | S 548, L 428, O 624, W 732 = 2332 across; 1600 at 40 mph or less, 2800 over 40 | UK TSM Ch.5 2018 11.2.1 p.106, Table 11-2 p.112 |
| `1038-ahead`, `-turn`, `-ahead-and-turn`, `-two-way` (S11-4-20) | Lane arrows (placeholder blocks) | length 4000, 6000, 9000 (4 m at 40 mph or less, 6 m at 50 or 60, 9 m at 70); widths 500 (shaft 300), 750, 850, 1200 by panel of the figure | UK TSM Ch.5 2018 Fig 9-1 and Table 9-1, p.97; TSRGD Sch.11 Pt.4 item 20, p.237 |
| `1014` (S11-4-14) | Deflection arrow (placeholder block) | length 4500, 6000, 9000; shaft 150, 200, 300; lateral extent 525, 700, 1050; head length 1500, 2000, 3000 | UK TSM Ch.5 2018 Fig 9-5, p.100 |
| `1040` (S11-4-23) | Hatched marking, parallel part | boundary 100 or 150; marks 4000 and gaps 2000 at 40 mph or less, 6000 and 3000 over; diagonals 45 degrees, 150 wide at 3000 spacing (short) or 200 wide at 2000 (long); 800 to 8000 between the boundaries | UK TSM Ch.5 2018 Fig 2-8 p.24, 2.7.1 p.23 |
| `1041` (S11-4-27) | Chevron marking, parallel part | as 1040 with 90 degree chevrons | UK TSM Ch.5 2018 Fig 2-16 p.30, 2.11 pp.30 to 31 |
| `1042` (S9-6-22) | Mandatory chevron marking, parallel-sided | continuous boundary 150 or 200; chevrons 90 degrees, arm 150 or 200, pitch 3400; 500 to 8000 between the boundaries | UK TSM Ch.5 2018 Fig 2-17 p.31, 2.11.2 |
| `STUD` | Road studs | spacing and colour by line type (below) | Tables 2-1, 2-2, 2-3, 2-5 pp.12 to 20; 3.3.1 p.36; 4.4, 4.5 pp.48 to 49 |

Lookup tables with their own tests: `lane_arrow_siting(mph)` (Table 9-1, p.97), `deflection_row(
speed85_mph)` (Table 2-6, p.24: taper 1 in 40/40/45/50/55, arrow 4.5/4.5/6/6/9 m, tip to taper
13.75, 43.75, 79.75 and so on), `stud_rows`, `uk_stud_spacing_on_bend` (4.5.3).

### Uganda (`Basis::Uganda`)

| Id | Marking | Dimensions read | Geometry |
|---|---|---|---|
| `M01` | Continuous dividing line, yellow | 100 | lines |
| `M02` | Double continuous dividing line, yellow | two 100, 130 apart | lines |
| `M03` | Broken dividing line, yellow | 100; urban 3000/6000, rural 4000/8000 | lines |
| `M04-L`, `M04-R` | Continuous and broken, yellow | M01 and M03, 130 apart (side of the broken line is not stated; `-L`/`-R` is the caller's choice) | lines |
| `M05` | Zig-zag dividing line, yellow | 100 wide, marks 2000, gaps 150, lateral extent 500 (300) | drawn |
| `M11`, `M12` | Lane lines, white | 100; M12 urban 3000/6000, rural 4000/8000 | lines |
| `M21` | Standard edge line, white | 100, 100 from the carriageway edge | lines |
| `M22` | Continuity line, white | 100 (200), 1000/1000 | lines |
| `M23` | No parking line, yellow | 100, about 250 out from the kerb (reading noted) | lines |
| `M31` | Stop line, white | continuous 400 | drawn |
| `M32` | Give way line, white | broken 400, marks 600, gaps 300 (read from proportions) | drawn |
| `M33` | Uncontrolled crossing | stripes 500 to 700, gaps 500 to 700, 3.0 to 5.0 m wide | drawn |
| `M34` | Signal crossing | two 100 broken lines, 500/500, 2.5 to 5.0 m apart | drawn |
| `M35` | Hump chequerboard | squares 500 (at least 300) | catalogue only (site inputs) |
| `M41` | Lane arrow, ahead | 4000 (6000 above 60 km/h), head 1575, head width 500, shaft 300 tapering to 120 | **outline drawn** |
| `M42`, `M43`, `M44`, `M45` | Turn, ahead-or-turn, diverge, deflecting arrows | lengths 4000/6000, 8000/16000, 5000/8000 | placeholder blocks |
| `M51`, `M52` | Chevron markings at island noses | 150 border, 600 tip, 8000 max | catalogue only |
| `M53` | Traffic island marking | yellow bars 200 at 45 degrees, 2000 apart, 150 continuous borders | parallel part drawn |
| `M61` | Give way marking (triangle) | 1250, 3750, 150, 600 | drawn |
| `M62` | STOP text marking | 2050 wide, 1600 or 2800 long | placeholder block |
| `M63` | Speed limit marking | dimensions not assignable | catalogue only |
| `M71` | Parking bay | min 2500 by 5000, 100 line | catalogue only |
| `STUD` | Road studs (Table E.1, Vol.1 p.40) | M01/M02/M04 white 6 m; M03/M12 white 18 m urban, 24 m rural; M21 red (amber next to medians) 18/24; M22 green 3 | point elements |

Citations are `Uganda TSM Vol.2 (2004) App.3 sign page Mnn (PDF p.nnn); Vol.1 section E.x (p.nn)`
in each entry. Application rules (where each is used, sight distances, siting of arrows) are
kept in `ExtSpec::applies`; two are also functions: `uganda_lane_arrow_siting(length_m)` (15 m
and a further 30 m, plus 50 per cent for 6 m arrows) and
`uganda_overtaking_sight_distance_m(km/h)` (50: 150 m, 60: 180, 80: 240, 100: 300).

## Using it from the app

```rust
use afrobim_markings::*;

// Register next to MarkingModule.
modules.register(Box::new(ExtMarkingModule));

// Linear item: station range and offset (positive right looking up-station).
// Yellow double line, kerb on the left at 4.0 m: lines lie inward of the kerb.
let def = ExtDefinition::linear("1018.1", Basis::Uk, 120.0, 180.0, -4.0)
    .with_width(100).with_separation(75).with_material("thermoplastic");
let id = add_ext_marking(&mut doc, alignment_id, "RX-001", &def)?; // Err carries the Error notes

// Hatching: strip width (clear, between the inner edges of the boundaries) is an input.
let hatch = ExtDefinition::linear("1040", Basis::Uk, 400.0, 460.0, 0.0)
    .with_strip_width(3000).with_width(100)
    .with_speed(40, "64 km/h limit: 40 mph band, project rule PR-3");

// Across the road: station, from-offset, to-offset (a lane, or the whole carriageway width).
let stop = ExtDefinition::across("1002.1", Basis::Uk, 500.0, -3.65, 0.0);
let give_way = ExtDefinition::across("M32", Basis::Uganda, 500.0, -3.65, 0.0);

// Point item: station, offset (centre of the item), rotation in degrees from up-station.
let arrow = ExtDefinition::point("M41", Basis::Uganda, 470.0, -1.8, 0.0).with_speed(80, "80 km/h");
let slow = ExtDefinition::point("1024", Basis::Uk, 470.0, -1.8, 0.0).with_speed(30, "30 mph");

// Studs: pick the row, then place them.
let rows = stud_rows(Basis::Uk, "1005.1", None, Some(60)); // spacing, colour, citation
let studs = ExtDefinition::studs(Basis::Uk, 0.0, 90.0, 0.0, 18_000, StudColour::White);

// A UK value under the Uganda basis is refused unless chosen:
let uk_on_ug = ExtDefinition::linear("1040", Basis::Uganda, 0.0, 24.0, 0.0)
    .with_strip_width(3000).with_width(100).with_speed(30, "")
    .with_uk_fallback("no Uganda equivalent; UK 1040 adopted for this junction");

// Edit, read back, follow an alignment that changed; quantities.
set_ext_marking(&mut doc, id, &edited)?;
let back = ext_marking_of(&doc, id);
sync_ext_markings(&mut doc);
let takeoff = ExtTakeoff::measure(doc.elements());
let schedule = ext_markings_to_csv(&takeoff);
```

Catalogue for a dialog: `ext_catalogue_for(Basis::Uganda)` (each `ExtSpec` has `id`, `name`,
`diagram`, `kind`, `geometry` status, `applies`, `citation`, `notes`), `stud_catalogue()`.

Placement rules (also in `ext_def.rs`):

* Linear items: `station_m` to `end_station_m` at `offset_m`; marks and gaps are laid as for the
  first catalogue (along the line itself, whole marks, a cut mark is a part mark).
* Across the road: from `offset_m` to `across_to_offset_m` at `station_m`, square to the
  alignment, turned about the `offset_m` end by `rotation_deg`. A broken line starts from the
  `offset_m` end.
* Point items: the centre of the item's bounding block is at the station and offset; rotation 0
  points up-station; rotation is anticlockwise in plan.
* An item is one rigid shape at its centre (a template), so on a bend it stays straight.
* Items on a kerb or carriageway edge (`1017`, `1018.1`, `M21`, `M23`): `offset_m` is the edge, the
  line lies inward of it (toward the alignment), the sign of the offset says which side.

Output per element: plan drawing, solids (display thickness 5 mm, display only, as before; stud
markers are 100 mm square for display only), and derived values (`Basis`, `Marking`, `TSRGD
reference`, dimensions, `Station`, `Offset`, `Colour`, `Number`, `Part marks`, `Painted length`,
`Painted area`, `Block area`, `Source`, `Reading notes`, `Speed note (caller)`, `UK fallback
(caller's choice)`, `plan_origin_x_m`, `plan_origin_y_m`).

Quantities (`ExtTakeoff`, `ext_markings_to_csv`): painted area, painted length, number (whole
marks, bars, stripes, arrows, studs; one for a point item) and part marks, and for a placeholder
the area of its block. **A placeholder has no painted area** (the cell is empty and the flag
`placeholder` is true); its block area overstates the paint and must not be priced as paint.

## What each quantity is

| Item | Number | Painted length | Painted area |
|---|---|---|---|
| Lines | whole marks | sum of mark lengths | length x width |
| Stop and give way lines | marks across (per row) | sum of mark lengths x rows | length x width |
| Hatching, chevrons, island marking | boundary marks + diagonals or chevrons | boundary marks + diagonal lengths | boundary area + sum of ideal diagonal areas |
| Zig-zag | marks | sum of slope lengths | sum of parallelogram areas |
| Crossings | stripes, or marks of the side lines | stripe length x number, or mark lengths | sum |
| Triangle, M41 | 1 | 0 or length | ring or outline area |
| Placeholder block | 1 | 0 | none (block area separate) |
| Studs | studs | 0 | none |

## Worked examples reproduced from the sources, and slips found in them

Reproduced in tests:

* Uganda M41: the figure's 2425 equals 4000 minus the head length 1575; the outline's area is
  0.50925 (shaft) plus 0.39375 (head) = 0.903 m2 by shoelace.
* Uganda lane arrow siting: 15 m and a further 30 m; "increased by 50%" for 6 m arrows is
  22.5 m and 45 m.
* UK Table 9-1 "up to 1.5 times": 37.5 m, 75 m.
* UK Table 11-2 tile widths sum for SLOW (2332) and STOP (2128).
* UK Table 2-6 rows, Table 2-1, 2-2, 2-3, 2-5 stud spacings, 3.3.1 range, 4.5.3 reduction.
* Uganda M01 sight distance table.

Slips and unclear points in the sources (nothing was corrected silently):

1. UK Ch.5 2018, 2.11.1 prints diagram "1049B (S9-67)"; every other place has S9-6-7.
2. UK Ch.5 2018, 11.2.2 refers to "the triangle marking to diagram 1023 (S9-6-4)"; the
   Regulations (Sch.9 Pt.6 item 4) and 6.3.6 call it 1023A.
3. UK Ch.5 2018 Figure 2-9 says three deflection arrows are "spaced at 2 and 3 seconds of travel
   time at 70 mph" and shows 64 m and 96 m. At 70 mph (31.29 m/s) 2 s is 62.6 m and 3 s is 93.9 m.
   The 2:3 ratio holds; the metres do not. The differences between arrows in Table 2-6
   (54 m and 99 m in the 61 to 70 mph row) do not reproduce from 2 s and 3 s at any single speed
   either. The Table 2-6 values are used as printed.
4. UK Ch.5 2018 Figure 2-8: the area between the boundary lines is dimensioned "800 min / 8000
   max" and a further "400 min" appears without a clear object; the chapter's text does not
   say between which edges 800 and 8000 are measured. The first is an input read as the clear width between the
   boundary lines' inner edges; the second is not used. Figure 2-17's "1000" is not used either.
5. Uganda Vol.1 E.4 and Vol.2 index: "M44 Diverge arrow – ahead of left turn lane" (probably "or").
6. Uganda Vol.1 E.6.2 is headed "Purpose and Use of Road Markings" but covers road studs.
7. Uganda M04 does not say on which side of the pair the broken line is; M43's figure carries
   seven dimensions without saying which is the overall width; M32's 300 and 600 are not labelled
   mark and gap (read from proportions); M53's taper table prints a symbol (extracted as `#`)
   where a less-than-or-equal sign is probably meant; the 6 m versions of the arrows are "extended
   proportionally" without saying which dimensions scale.
8. Uganda M21: "marked 100 mm from the edge of the carriageway" and M23 "about 250 mm out from the
   kerb" do not say which edge of the line the distance is to; the near edge is taken.

## Not done and why (this part)

* **Letter shapes** (STOP, SLOW, M62): only tile widths and character heights are given; blocks.
* **Lane arrows 1038**: Chapter 5 and the Regulations give one length and one width per panel
  and say "full details are shown on the working drawings", which were not available. Blocks;
  no painted area. The semantics of the four panels (turn, ahead, ahead and turn, branches both
  sides) are read from thumbnails.
* **Deflection arrow 1014**: the bend of the shaft and the head's barbs are not dimensioned
  beyond Figure 9-5's lengths and widths. Block.
* **Bifurcation arrow 1039, guidance arrows 1038.1, lane destinations 1035**: not read.
* **Hatching** 1040.3, 1040.4, 1040.5, tapers (Tables 2-6 and 2-7 are catalogued as lookups, tapered
  geometry is not laid), the refuge approach, and the tapered versions of 1041 and 1042.
* **Raised rib edge lines** 1012.2 and 1012.3 (Figure 2-7), 1009B (cycle track junction, in
  Chapter 3), double white lines version B and 1013.5, as in the first part.
* **Yellow lines**: 1017 and 1018.1 only. The loading restriction lines 1019 and 1020.1, the
  red route lines 1017.1 and 1018.2, clearways 1025.1, entrance markings 1027.1 and the end marks
  of 1017 and 1018.1 are in the Regulations and were not read or modelled. Shades of yellow are in
  12.5.3 of the chapter (a note, not geometry).
* **Mini-roundabout** 1003.3 and 1003.4: not read.
* **Uganda**: curved arrows (M42 to M45 bends and branches), M35, M51, M52, M63, M71 and the
  tapers of M53 are catalogued or blocks, not drawn.
* The Uganda manual leaves the tolerances, laying (start of the pattern) and the width of M53
  unstated; the same rules as the first part apply.
* Uganda `M21` with a paved shoulder, and `M23` on the kerb face, are not modelled.
* Not read: UK Chapter 6 crossings and zig-zags, Chapter 3 regulatory markings.

## Limits (this part)

* Plan geometry for quantities; the display solids follow the profile (see below); item areas are the ideal shapes (a hatch on a tight curve is slightly
  distorted by its vertex mapping and its area is not corrected).
* Hatching and chevrons start their first diagonal at the start station (the sources do not
  say); only whole diagonals are drawn; the lean of a diagonal is not dimensioned (`mirror`).
* Studs: one at the start station and one at each whole spacing along the offset line; the stud
  spacings are those of the tables for the line type and are inputs of the element (use
  `stud_rows`). A bend or fog reduction is the caller's (`uk_stud_spacing_on_bend` for 4.5.3).
* Speed-dependent choices use the number the caller gives in the basis's own unit; where the
  table has no column (for example 45 mph for Table 9-1) the element is refused, never rounded.
* The 6 m Uganda arrow scales head and shaft lengths by 6000/4000 and keeps all widths.
* TSRGD diagrams are read from the 2016 PDF as published; amendments after 2016 are not applied.

### Level of the solids, and the IFC mapping (round 4)

* **Level.** The solids of a second-set marking lie at the profile level of the alignment. A
  strip (a line, the boundary lines of hatching or chevrons) takes the level of each of its
  stations. A polygon (a stop or give way line, the triangle, an arrow, a worded block, the
  diagonals of hatching and chevrons, the zig-zag marks, a stud, the crossing stripes) takes, at
  each vertex, the level of the station of that plan point (`Alignment::station_offset`), with
  the mean of the other vertices where a vertex has none. Until round 4 polygons were extruded
  from z = 0 and strips followed the profile, so on a road at 100 m the diagonals of a hatch hung
  100 m below its boundary lines and the 3D view (and the IFC) showed them away from the
  pavement. `ext_plan` takes the station lookup as its last argument. The test
  `every_solid_of_every_kind_of_marking_sits_at_the_road_level_of_its_station` fails on the old
  code (a vertex at 0 mm where the road is at 50 998 mm) and passes now. The level is for display
  only, as the 5 mm thickness is, and enters no quantity.
* **IFC.** `afrobim-ifc` writes `marking_ext` as `IfcSurfaceFeature` adhering to a pavement that
  carries it, with the predefined type of its catalogue kind; see "Second marking set" in
  `crates/afrobim-ifc/README.md`.
