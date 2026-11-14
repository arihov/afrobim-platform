# afrobim-restraint

Road restraint systems as AfroBIM model elements: safety barrier runs along an alignment, with a
plan drawing, schematic solids, quantities and requirement checks.

**The output is for an engineer to check. It is not a certified design, and it is not the risk
assessment the standard requires.**

## Two parts

**Modelling** (`BarrierRun`, `resolve`, `plan_drawing`, `solids`, `RestraintQuantities`,
`RestraintModule`). A run is a length of safety barrier between two stations of an alignment at
a signed offset. It carries its declared classes (containment level, working width class,
impact severity level and, for higher containment, vehicle intrusion class), a free-text system
or product description, its end treatments, and post spacing and beam height **as entered
product data**. This side draws and measures; it holds no requirement and makes no claim of
compliance.

**Requirements** (`design` module). The checks the UK standards state in a form that can be
applied exactly, each with its clause, set out line by line (`RestraintDesign::calculation`)
like `SupportDesign::calculation` in `afrobim-supports`.

## Governing sources

UK standards govern. Every number in the code that comes from a standard carries its document,
edition and clause or table in a comment, and is asserted in a test against that table.

| Document | Edition obtained | From | Used for |
|---|---|---|---|
| DMRB **CD 377** *Requirements for road restraint systems* | Version 5.0.1, published 27 February 2026 (core document; the four National Application Annexes are attached to it) | standardsforhighways.co.uk, downloaded 14 November 2026 | All requirement checks below |
| DMRB **CD 127** *Cross-sections and headrooms* | Version 1.0.1, published 29 July 2021 | standardsforhighways.co.uk, same day | Set-back: CD 377 clause 2.37 makes CD 127 the source |
| **BS EN 1317-2:2010** *Road restraint systems, Part 2* | EN 1317-2:2010 (E), the owner's copy (a scan, read from rendered pages) | supplied by the owner | Tables 1 to 5: impact tests, containment levels and their notes, impact severity levels, working width classes, vehicle intrusion classes |
| **DD ENV 1317-4:2002** *Part 4: terminals and transitions* | Draft for Development, the English version of ENV 1317-4:2001; its foreword says it "is not to be regarded as a British Standard" | supplied by the owner | Table 1 and clause 5.1: terminal classes P1 to P4 and their order |
| MoWT *Road Design Manual Volume 1: Geometric Design* | January 2010 | uace.or.ug | Local reference only (Section 12.3); see "Differences" |
| TII **DN-REQ-03034** *Safety Barriers* (Ireland) | November 2015 incl. Amendment No. 1 | tiipublications.ie | Cross-check only: its Tables 5/1 to 5/4 reproduce the EN 1317-2 tables and agree with the owner's copy |

Held but **not coded from** in this cut: BS EN 1317-1:2010, BS EN 1317-3:2010 (crash cushions),
BS EN 1317-5:2007+A1:2008 (product requirements), and AASHTO *MASH 2016*, which is kept for a
later United States option and governs nothing here. **PD CEN/TS 1317-7** (terminal classes
T80/3, T110, T110/1) and **PD 6688-1-7** are not held. No PDF is in the repository.

The owner should confirm that BS EN 1317-2:2010 is still the current edition, and replace
DD ENV 1317-4:2002 with whatever has superseded it when a copy is available: CD 377 5.0.1 still
cites both.

### Citations

| Value in the code | Source |
|---|---|
| Impact tests TB 11 to TB 81: speed, angle, mass, vehicle | BS EN 1317-2:2010 Table 1 |
| Containment levels T1 to T3, N1, N2, H1 to H4b, L1 to L4b, their groups and acceptance tests | BS EN 1317-2:2010 Table 2 |
| Which level covers which (`ContainmentLevel::covers`) | BS EN 1317-2:2010 Table 2 NOTES 2, 3 and 4 |
| Impact severity A, B, C: ASI ≤ 1.0, 1.4, 1.9; THIV ≤ 33 km/h | BS EN 1317-2:2010 Table 3 |
| Working width W1 to W8: ≤ 0.6, 0.8, 1.0, 1.3, 1.7, 2.1, 2.5, 3.5 m | BS EN 1317-2:2010 Table 4 |
| Vehicle intrusion VI1 to VI8: same levels; VI9 > 3.5 m | BS EN 1317-2:2010 Table 5 |
| Terminal classes P1 < P2 < P3 < P4 | DD ENV 1317-4:2002 Table 1, clause 5.1 |
| Minimum containment N2 at 50 mph or more, N1 below | CD 377 3.6, 3.7 |
| N2 in front of a structural support; H4a (CS 453 Group 1a/1b), H1 (Group 2) | CD 377 3.47, Table 3.48 |
| N2 at each end of a vehicle parapet | CD 377 3.64 |
| ISL A or B for permanent safety barriers; A preferred | CD 377 3.12, 3.12.1 |
| Taper not steeper than 1 in 16 (deformable), 1 in 20 (rigid), non-proprietary | CD 377 2.39 |
| Terminals: T110, T110/1 or P4 facing traffic at 50 mph or more; otherwise minimum T80/3 or P1; energy absorbing | CD 377 6.3, 6.4, 6.5, 6.7 |
| Transition ISL not above B (C into an impact-resistant structure) | CD 377 7.11, 7.12 |
| Gaps: 50 m and 100 m | CD 377 3.35, 3.36 |
| 50 mph and 5,000 AADT scope thresholds | CD 377 1.3, 2.2, 2.3 |
| Set-back 1200 / 600 / 1200 mm; relaxations to 600, 1000, 450 mm | CD 127 Table 2.24 and Notes 1 to 3 |
| MoWT: 30 m minimum guardrail, 600 mm set-back, 80 m gaps | MoWT 2010, 12.3.4, 12.3.6, 12.3.7 |

Numbers that come from no standard, all named as such in the code: `SCHEMATIC_POST_SIZE_MM`
(100), `SCHEMATIC_BEAM_DEPTH_MM` (300), `SCHEMATIC_BEAM_THICKNESS_MM` (50) and
`PLAN_POST_TICK_M` (0.3) are display sizes; `POST_TOLERANCE_M` (1 mm) and `MAX_POSTS` are
tolerances; `KM_PER_MILE` (1.609344) is the definition of the mile.

## What the requirement checks cover

| Check | Clause | Inputs |
|---|---|---|
| Minimum containment by speed limit | CD 377 3.6, 3.7 | speed limit |
| A higher level from the site specific risk assessment | CD 377 3.9 | that level, entered |
| In front of a structural support | CD 377 3.47, Table 3.48 | CS 453 group, entered |
| At the end of a vehicle parapet | CD 377 3.64 | a flag |
| Impact severity level | CD 377 3.12, 3.12.1 | |
| No hazard within the working width class; the widest class the clearance allows | CD 377 3.13 to 3.15 | clearance from the traffic face |
| No hazard within the vehicle intrusion class (higher and very high containment) | CD 377 3.13, 3.17 | the same clearance; the run's VI class |
| Crest or toe of a slope over 200 mm high outside the working width class | CD 377 3.39, 3.41 | distance to the slope |
| Set-back: desirable minimum, relaxations, the double sided central reserve rule | CD 127 Table 2.24, 2.25, 2.26 | location, the measured set-back, which relaxations the engineer claims |
| Rate of change of set-back | CD 377 2.38, 2.39 | change, length, and either the manufacturer's rate or "non-proprietary" |
| Length of need and minimum total length | CD 377 3.24 to 3.31, Figure 3.24 | **all the lengths** (see below) |
| Terminals: class and energy absorption | CD 377 6.1, 6.3 to 6.5, 6.7 | whether each end faces oncoming traffic |
| Transitions: presence, containment, ISL | CD 377 3.59, 7.1, 7.2, 7.10 to 7.12 | transition data on the run |
| Gap to the next installation | CD 377 3.35, 3.36 | the gap |

Each line ends in one of: `OK`, `OK BY RELAXATION` (permitted, to be justified), `FAILS`,
`REVIEW` (the clause has exceptions or needs judgement), `NOT CHECKED` (an input is missing or
the source is not held; nothing is assumed), `NOTE`.

The length-of-need arithmetic reproduces both worked examples of CD 377 Appendix C
(C1: 20.0 + 5.5 + 0 = 25.5 m, total 50.5 m; C2: 26.1 m, adopted 45.0 m, total 79.8 m).

### The RRRAP is outside this tool

CD 377 2.3 requires the Road Restraint Risk Assessment Process (RRRAP) on motorways and
all-purpose trunk roads of 50 mph or more and 5,000 AADT or more, and a local risk assessment
otherwise (2.2). Whether a barrier is warranted, how long it must be in advance of and beyond a
hazard (3.26, 3.27) and any containment level above the minimum (3.9) are outputs of that
process. **Version 5 of CD 377 gives no fixed lengths at all**: there is no longer a table of
minimum lengths to code. This crate does not perform, imitate or approximate the RRRAP. Those
values are inputs, and every report says so in its second line.

## Differences found in the MoWT manual

The MoWT *Geometric Design Manual* (2010), Section 12.3, is guidance of a different kind: it
says of itself that it "can only give a brief introduction" and to "always seek advice from
experts". UK governs; the MoWT figures appear only as optional `NOTE` lines
(`DesignInput::include_mowt_reference`).

| Subject | MoWT 2010 | UK (CD 377 5.0.1 / CD 127) |
|---|---|---|
| Performance | No EN 1317 classes. "Conventional" barrier is for cars at 65 km/h and 25° (12.3.2); types by deflection (Table 12-1: wire rope 2 m+, weak post 1 to 2 m, strong post 0.9 m, concrete none) | Containment level, working width class, ISL to BS EN 1317-2 |
| When to provide | Criteria list for roads above 50 km/h (drops over 2 m at sharp bends, water deeper than 1 m, objects in the clear zone, medians under 9 m with ADT over 10,000) and a fill height/slope chart, Figure 12-1 (12.3.3) | Risk assessment: RRRAP or local (2.2 to 2.6) |
| Length | "Generally you need at least 30 m" (12.3.4); run-out lengths by speed, Figure 12-2 (60 km/h: 50 to 60 m; 80: 80 to 90; 100: 100 to 110; over 110: 120), "a starting point" | From the RRRAP plus the manufacturer's lengths (3.24 to 3.31); no fixed figure |
| Set-back | At least 600 mm from the shoulder edge, or the carriageway edge without a shoulder (12.3.6) | 1200 mm or 600 mm by location, measured from a defined point of set-back (CD 127 Table 2.24, 2.25) |
| Clearance behind | At least 1000 mm from the back of the post to a rigid obstacle (500 mm if stiffened); 600 mm from post to break of slope (12.3.6) | No hazard within the working width class, measured from the traffic face (3.13); slope crest outside it (3.39) |
| Beam and posts | W-beam centre 550 mm ± 5 mm above the road; posts at 1900 or 3800 mm (12.3.5) | "In accordance with the manufacturers' specification" (3.5) |
| Ends | Flare by at least 1 m at 1 in 10 **and** an impact-absorbing terminal or a sharp ramp-down (12.3.7) | Tested terminals by class (Section 6); taper per manufacturer, or 1 in 16 / 1 in 20 (2.38, 2.39). A ramped end is not a CD 377 terminal |
| Gaps | Do not leave gaps under 80 m (12.3.7) | Close gaps up to 50 m; up to 100 m unless costly (3.35, 3.36) |
| Parapet approach | Semi-rigid guardrail "normally at least 30 m", stiffened into the parapet (12.3.8) | A transition (3.59, Section 7); length from the risk assessment |
| Plan symbol | None defined | None defined |

The MoWT post spacing, beam height and flare are **not** coded as defaults: post spacing and
beam height are entered product data.

## How quantities are computed

- **Length** is measured along the barrier (the offset line), not the centreline:
  `L = (end station − start station) + offset × angle turned`, where the angle turned is the
  integral of the alignment's curvature over the run (positive to the left) and the offset is
  positive to the right. This is exact on straights, circular arcs and clothoids; no chords are
  involved. A barrier on the inside of a curve is shorter than the centreline, on the outside
  longer. Terminals are not part of the length.
- **Posts**: one at the start, one every entered spacing along the barrier, one at the end.
  `bays = ceil((L − 1 mm) / spacing)`, `posts = bays + 1`; the last bay may be short. A run
  with no spacing entered has no posts.
- **Terminals** are counted by performance class and **transitions** by what they lead to and
  their containment level, as entered on the run ends.
- Totals by **containment level and working width class**.
- `runs_to_csv` gives one row per run and a `TOTAL` row; `summary_to_csv` gives
  `item,unit,quantity` rows. No rates, no costs.

Enter a transition between two runs on one of them only, or it is counted twice. Where two
runs meet, each has its own end post.

## Geometry

- **Plan**: neither CD 377 nor the MoWT manual defines a plan symbol for a safety barrier, so
  the run is a plain hairline with a short tick at each post on the side away from traffic and
  a bar across each end that has a terminal. Millimetre units, 1 m = 1000, like the alignment
  plan.
- **Solids**: a schematic, not a product model. A rectangular strip at the entered beam height
  with its traffic face on the run's line, and square posts behind it at the entered spacing.
  They follow the alignment in plan and take the alignment's **centreline** profile level at
  each station; with no profile covering the run, z = 0 and the element carries an Info note.
  No cross-fall or verge level is applied.

## For the application

```rust
use afrobim_restraint as rr;

modules.register(Box::new(rr::RestraintModule));            // category "restraint"

let run = rr::BarrierRun {
    name: "VRS-001".into(),
    alignment: String::new(),                                // set by add_barrier_run
    start_station_m: 1200.0,                                 // internal stations
    end_station_m: 1320.0,
    offset_m: 5.2,                                           // + right, of the traffic face
    traffic_face: rr::TrafficFace::Left,
    containment: rr::ContainmentLevel::N2,
    working_width: rr::WorkingWidthClass::W4,
    impact_severity: rr::ImpactSeverityLevel::A,
    vehicle_intrusion: None,
    system: "as specified".into(),
    post_spacing_m: Some(3.2),                               // entered product data
    beam_height_m: 0.61,                                     // entered product data
    start_end: rr::EndTreatment::terminal(rr::TerminalClass::T110),
    end_end: rr::EndTreatment::terminal(rr::TerminalClass::T80_3),
    set_back_m: Some(1.2),
};
let id = rr::add_barrier_run(&mut doc, &alignment, run)?;    // layer "Road restraint"

rr::sync_barrier_runs(&mut doc);                             // after an alignment changes
let q = rr::RestraintQuantities::measure(doc.elements());
let (schedule, summary) = (rr::runs_to_csv(&q.runs), rr::summary_to_csv(&q));

let b = rr::barrier_run_of(&doc, id)?;
let input = rr::design::DesignInput::new(rr::design::SpeedLimit::Kmh(100.0));
let report = rr::design::check_run(&b.run, &b.resolved, &input).calculation();
```

For the 3D view: `modules.generate(element)?.solids` are in millimetres in the element's frame;
the element's `site` gives that frame's origin (x, y, z in millimetres, rotation 0).

`parse_products_csv` reads a **user-supplied** product list
(`name,containment,working_width_class,impact_severity,post_spacing_m`) and `Product::apply_to`
fills a run from a line of it. No product data is bundled: lists such as Highways England's
*List of EN1317 Compliant Road Restraint Systems* (March 2016) are dated product lists, and the
free-text `system` field is where a product from such a list is named.

## Ambiguities, flagged and not resolved silently

1. **H levels against a minimum of N2.** BS EN 1317-2:2010 Table 2 NOTE 2 says H1 to H4b "do
   not include N2" (they are not tested with the 1500 kg car at 110 km/h, TB 32; the L levels
   are). CD 377 nevertheless sets N2 as the general minimum and H1 or H4a at structural
   supports, so it treats an H level as "greater than N2". The check reports OK and prints the
   standard's note on the line. Whether an L level should be asked for is the engineer's call.
2. **H4a against H4b** (and L4a against L4b): no hierarchy (Table 2 NOTE 3). Reported as REVIEW.
3. **Terminal classes T80/3, T110, T110/1.** PD CEN/TS 1317-7 is not held. Where CD 377 asks
   for "a minimum of T80/3", T110 and T110/1 are accepted as higher on the strength of CD 377
   6.3 alone; the line says it is an assumption.
4. **Metric speed limits.** CD 377 steps at 50 mph (80.47 km/h). A posted 80 km/h is 49.7 mph
   and falls in the lower band (N1, lower terminal classes, set-back relaxation). The report
   flags this for the road authority's ruling. "50 mph or more" (CD 377) and "50mph or less"
   (CD 127 Note 1) overlap at exactly 50 mph: both then apply.
5. **Scope.** CD 377 is written for the UK motorway and trunk road network and for its
   Overseeing Organisations. Applying it elsewhere is the adopting authority's decision; the
   National Application Annexes (England, Northern Ireland, Scotland, Wales) are not applied.
6. **Set-back is entered, not measured.** The alignment does not say where the back of the
   hard strip, kerb face or edge line is (CD 127 2.25), so the crate cannot derive set-back
   from the offset.
7. **Levels** are centreline profile levels, not the level of the verge at the barrier.
8. **MoWT text.** The manual prints "the beams are 4130 m long" (mm is meant, as its Figure
   12-3 shows), gives post spacing as 1900/3800 mm in the text and 1905 mm in the figures, and
   says "a flare rate of at least 1 in 10", which does not say whether steeper or flatter is
   meant. None of these is used in a calculation.
9. **CD 127 is dated 2021** and CD 377 5.0.1 (2026) refers to it without a revision; it was the
   only published version on the day of download.

## Not in this cut

Vehicle parapet design (a parapet is only a named thing a run can connect to), crash cushions,
pedestrian restraint, motorcyclist protection, temporary barriers, removable barrier sections,
central reserve provision and crossing points, the RRRAP and any other risk assessment, new
structural supports (PD 6688-1-7), terminal lateral displacement and exit box classes, checking
where a hazard sits within a run, tapered or flared offsets in the model (a run has one
offset), product catalogues, rates and costs.
