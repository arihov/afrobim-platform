# afrobim-visibility

Sign visibility along an alignment (work package A2, analysis stage). For a sign with a site
placement, a driver's eye is swept along the approach and sight lines are traced to the sign face.
The result is the continuous clear visibility distance, what obstructs the view and at which
stations, the obliquity of the line of sight, and a list of check lines against the provisions of
the UK Traffic Signs Manual chapters and the Uganda Traffic Signs Manual held in `Manuals/`.

It is an analysis aid for an engineer. It states no compliance: each line is `OK`, `FAILS`,
`DIFFERS`, `INPUT NEEDED` or `INFORMATION` with its citation, and the engineer decides.

Nothing here is from memory. Every value of a standard has a `Citation` (document, edition,
provision, page) in `src/sources.rs`; what could not be found is listed below as "not found in the
documents held" and stays an input. Tables in miles per hour stay in miles per hour: the band a
km/h limit belongs to (`SpeedBandMph`) is the engineer's decision and is never derived.

## Method

Site metres, X east, Y north, Z up. Offsets are positive to the right looking up-station.

1. **The face.** The sign's site placement gives the lower-left corner of the face (lifted by the
   mounting height: the entered one, else `Mounting height (mm)` on the sign's first post), and its
   rotation. The width runs along `(cos r, sin r)` and the face looks along `(sin r, -cos r)`,
   exactly as `afrobim-app` draws it (`view3d::frame_in`). The size is the element's `width` and
   `height`. Traffic served by the face travels on heading `r + 90` degrees, which fixes the
   direction of travel unless one is entered.
2. **The eye.** On the approach at a signed offset from the centreline (entered as a signed offset
   or as the distance to the centre of the lane, `EyeOffset`, with the side taken from the traffic
   side and direction), `eye_height_m` above the profile level at its station. The sweep goes from
   `step_m` to `start_distance_m` before the sign. The **distance** of an eye position is the
   distance along the alignment to the station of the face centre, not the length of the sight line.
3. **Sight lines** go to a `grid` x `grid` lattice of the face (default 3 x 3: corners, edge
   midpoints and centre). A line only touching an obstruction, or ending on it, is not blocked.
4. **What blocks a line**
   - every triangle of the `ObstacleScene`: ground TIN, corridor surfaces and side slopes, other
     signs (a 10 mm slab, a modelling choice), posts, foundations, gantries, lighting columns,
     barriers and any other solid with a site placement. The sign's own supports are skipped;
   - the road's own vertical profile (a crest): the line is blocked where it runs below the
     centreline level of the profile at the station under it, sampled every `sample_m`;
   - user-entered lateral obstructions (`LateralObstruction`): everything beyond an offset on one
     side, optionally of limited height and station range. This is the inside of a horizontal curve
     when nothing is modelled there;
   - the back of the face: an eye not in front of the face plane does not see it.
5. **Result.** Per eye position: the points seen (`Full`, `Partial`, `Blocked`), the blockers with
   their stations, the obliquity (3D and in plan) between the line of sight to the face centre and
   the face normal, and the observation angle (travel direction to line of sight). The
   **continuous clear visibility distance** is the farthest distance from which the sign stays
   visible at every closer position, for the whole face (`full`) and for part of it (`partial`),
   refined by bisection to `tolerance_m`. Intervals beyond the limit from which it is visible again
   are listed, and blockers are summarised over the whole sweep.

## Sources: what was found

Citations are by document, edition, provision and printed page (PDF page in brackets).

UK Traffic Signs Manual (the editions held differ by chapter; they are stated per value):

| Provision | Where | Used for |
|---|---|---|
| Minimum clear visibility distance by 85th percentile speed band (mph): 45, 60, 60, 75, 90, 105 (120) m | Chapter 3 Regulatory Signs, 2008, Table 1-1 with paras 1.15 to 1.18, p. 6 (PDF 9) | regulatory signs |
| Measured from the centre of the most disadvantaged driving lane; the full sight line to the whole sign face is preserved | Chapter 3, 2008, para 1.16, p. 6; Chapter 4, 2013, para 1.23, p. 6 (PDF 8) | eye lane, "whole face" criterion |
| Warning signs: minimum clear visibility distance (column 3, not to be reduced) and distance of sign from hazard (column 4: 45, 45, 45 to 110, 110 to 180, 180 to 245, 245 to 305 m), notes 1 to 5 (largest figures 120 m and 305 m for 70 mph dual three or four lane roads; braking and gradient; 50 % increase for a 10 % descent as an example) | Chapter 4 Warning Signs, fourth edition 2013, Appendix A, p. 73 (PDF 75) | warning signs |
| Siting within about 10 % of the recommended distance; not more than 10 % closer; distance plate | Chapter 4, 2013, paras 1.20 and 1.21, pp. 5 to 6; Chapter 1, 1982, paras 1.54 and 1.55, p. 15 | siting tolerance |
| Terminal speed limit signs, minimum clear visibility distance by approach limit and sign limit | Chapter 3, 2008, Table 14-3, p. 116 (PDF 119) | `UkSignRule::SpeedLimitTerminal` |
| Reading: observation angle 10 degrees, reading time, `C = S x cot 10`, `R = reading time x speed`, reading time `2 + N/3` s (4 s at N = 6; not more than six destinations), x-height 100 mm at 60 m proportional | Chapter 2, "Determination of x-height" (3-page extract, edition not stated), PDF p. 3; the same reading rule in Chapter 1, 1982, para 1.31, pp. 12 to 13 | legibility lines |
| 10 degree sight line to the lower part of an overhead sign | Chapter 7, 2013, para 9.7, p. 89 (PDF 91) | overhead line |
| Mounting height of the lower edge 900 to 1500 mm above the carriageway alongside; footway headroom 2300 mm (2100 absolute minimum) | Chapter 3, 2008, para 1.21, p. 7; Chapter 4, 2013, para 1.24, p. 6; Chapter 1, 1982, para 1.49 | mounting line |
| Set-back: at least 450 mm from the edge of the carriageway; 600 mm with severe camber or on a central reserve; 1200 mm on high-speed dual carriageways; 600 mm behind a hardened verge | Chapter 1, 1982, para 1.48, p. 14 | clearance line |
| Face orientation 95 degrees to the left edge of carriageway on a straight rural road (the sense is in a figure that was not read) | Chapter 1, 1982, paras 1.51 and 1.52, pp. 14 to 15 | orientation line (information only) |
| Driver's eye position 1.05 m (stated for the STOP sign visibility criteria) | Chapter 3, 2008, para 3.10, p. 12 (PDF 15) | default eye height |

Uganda Traffic Signs Manual, Volume 1, July 2004 (and Volume 2 where stated):

| Provision | Where |
|---|---|
| Visibility distance to the sign: 50 m up to 50 km/h, 150 m over 50 and up to 80, 200 m over 80 | D.4.2, Table D.2, p. 9 |
| Second sign at least 0.6 v (v in km/h) beyond the first, minimum 30 m | D.4.2, p. 9 |
| Siting of standard warning signs from the hazard: 50, 100, 150 m | D.5.2, Table D.3, p. 13 |
| Siting of advance direction signs from the intersection: 50, 100, 150 m | D.7.2, Table D.5, p. 22 |
| Letter sizes on guidance signs: x-height 100, 150, 200 mm | D.7.3, Table D.6, p. 23 |
| Mounting height: lower edge 1.8 m normally, never less than 1 m, 2.1 m over or alongside a footway, 5.2 m on gantries and mast arms with 5.0 m clearance | D.4.3.1, pp. 9 to 10 |
| Clearance: nearest edge 0.6 m clear of the shoulder and 1 m from the traffic lane edge; 0.3 m from the kerb face on kerbed roads | D.4.3.2, p. 10 |
| Eye height 1.05 m (stated for the no-overtaking sight distance of marking M02) | Volume 2, Appendix 3, sign page M02, PDF p. 161 |

## Not found in the documents held

These are inputs, and the check lines say so:

- A numeric minimum clear visibility distance for **directional signs** in the UK chapters held
  (Chapter 7 para 9.8 refers to one without giving it). The legibility lines (distance from which
  reading must start, `R + C`) stand in the check.
- A UK **siting distance for advance direction signs** in the chapters held; and the Uganda Table
  D.5 distance needs the station of the junction.
- The **hazard station** of any sign: no sign records one. It is an input
  (`hazard_station_m`), and the siting lines need it.
- Any **minimum spacing between signs** in the UK chapters held (Chapter 4 Appendix A note 4 and
  para 1.22 only say one sign must not obstruct another, which the geometry tests). The Uganda
  0.6 v, 30 m rule is applied between consecutive signs of the same side and direction as a
  `DIFFERS` line that says it applies to the two signs of one hazard sequence.
- A **lane width** or the position of the lane centre; the **85th percentile speed**; the **UK speed
  band** of a km/h limit; the **number of words** where the sign is not a counted legend.
- An **eye height stated for sign visibility** (1.05 m is stated for other criteria in two held
  sources and is the default, to be confirmed).
- **Stopping sight distance** (TD 9 is referred to by Chapter 4 para 10.7 but is not held).
- The dimensions of Uganda **Figure D.2** (letters A to H of a table whose figure is an image) were
  not used; only the text of D.4.3.1 and D.4.3.2.
- Volume 2 of the Uganda manual: its sign pages defer to Volume 1 for siting and size; no sign
  specific siting, mounting or clearance values were found by searching for siting, visibility,
  mounting height, clearance and lateral.
- Found but not implemented: Chapter 3 Tables 14-4 and 14-6 (repeater and other speed limit sign
  visibility), Chapter 4 Tables 10-1 and 10-2 (visibility criteria for animal crossing hazards),
  Uganda Volume 2 no-overtaking sight distances. Not searched: TSRGD 2016, Chapters 5 and 8 of the
  UK manual, the Chapter 2 primary route and TrafxPLAN documents.

Editions: the Chapter 1 held is the **1982** edition (a reprint, "Introduction"); its values may
differ in later editions. Chapter 3 is 2008, Chapter 4 the fourth edition of 2013, Chapter 7 2013.
The x-height method is held twice: an official 3-page extract (`tsm-chapter-02-x-height.pdf`,
`C = S x 5.7`, example 185 mm) and an unmarked variant (`Chapter2-x-height.pdf`,
`tsschapter2-x-height.pdf`, 5.6713 and an iterative example converging to 195.88 mm). The code
uses the exact `cot 10`.

### Worked examples reproduced, and slips recorded (tests in `src/checks.rs`)

- Official extract: `S = 2 + 1 + 1 = 4 m`, `C = 22.8 m` ("say 23"), `R = 4 x 22 = 88 m`, total 111 m,
  x-height `100/60 x 111 = 185 mm`. Reproduced. Slips: 50 mph is 22.35 m/s and 22 is used (R is 1.6 %
  short); 5.7 is a rounding of 5.6713; "22.8, say 23" rounds up while the speed was rounded down.
  With exact values the example gives 186.82 mm.
- Variant copy (iterative): fixed point `(R + 5.67 x 3) / (0.6 - 5.67 x 0.01) = 195.88 mm`, equal to
  the document's final value. Slips: iteration 1 prints 112.09 for 89.41 + 22.69 (= 112.10);
  iteration 4 prints `S = 4.96` but multiplies 4.95 and prints a total of 117.52 for 89.41 + 28.12
  (= 117.53), so its 195.88 belongs to 117.53; the final "sensibly rounded to 195 mm" rounds a
  requirement of 195.88 down.

## Hand-worked tests (`cargo test -p afrobim-visibility`)

Each has its working in the comment above it (`src/sweep.rs`, `src/checks.rs`, `src/analysis.rs`):

- straight level road with a wall: similar triangles give 78.0 m, visible again from 130 m;
- a crest curve: the parabola `S = (sqrt h1 + sqrt h2) / sqrt k` gives 344.92 m;
- a horizontal curve with a lateral obstruction offset: the mid-ordinate relation, chord
  `2 sqrt(r^2 - r0^2) = 106.66 m`, central angle, distance along the centreline;
- one sign masking another: 130.0 m, with the masking sign named;
- obliquity: 1.861 degrees, and 3.139 degrees with the face turned 5 degrees;
- legibility lines from the source relation, both worked examples above, Table 1-1, Table 14-3,
  Appendix A, the Uganda tables, the 10 percent siting tolerance;
- a ground ridge in a document: 49.19 m, with the sign's own post ignored;
- the road-wide run, CSV, spacing and persistence under `check.visibility`.

## Inputs and persistence

`VisibilityInputs` is stored as JSON text in the sign's properties under **`check.visibility`**
(`inputs::KEY`; `inputs::save`, `inputs::load`). The `check.` prefix is the convention of
`afrobim-app` (`check_inputs::PREFIX`), so the key is hidden from generic property editing and the
model-wide run reads it. Every field has a default, so a partly filled record loads. The inputs:

| Field | Meaning | Default |
|---|---|---|
| `eye_offset` | `Signed(m)` or `LaneCentre(m)` | none: the sweep asks for it |
| `sweep.eye_height_m` | driver's eye above the road | 1.05 (see sources) |
| `sweep.start_distance_m`, `step_m`, `sample_m`, `tolerance_m`, `grid` | analysis settings, not standards | 400, 5, 1, 0.05, 3 |
| `direction` | up or down station | from the face's rotation |
| `traffic_side` | side traffic keeps to | left |
| `approach_speed_kph` | 85th percentile speed | the speed class stored on the sign, if any |
| `uk_band` | UK mph band (engineer's decision) | none |
| `uk_large_case` | larger figures for 70 mph dual roads | false |
| `uk_rule` | `Auto`, `Regulatory`, `Warning`, `SpeedLimitTerminal{..}`, `Directional`, `NotApplicable` | `Auto` from the sign's class |
| `uk_clearance_case` | which UK set-back applies | normal 450 mm |
| `mounting_height_m` | override of the post's | the post's `Mounting height (mm)` |
| `over_footway`, `overhead` | which mounting provisions apply | false |
| `x_height_mm`, `words`, `reading_time_s` | legend; destinations counted from the sign if not given | module's derived x-height |
| `hazard_station_m` | hazard or junction station | none |
| `lateral` | lateral obstructions | none |
| `road_edges` | carriageway, lane and shoulder edge offsets, kerbed | none |
| `standards` | check UK, Uganda or both | both |

## API (for the app wiring)

```rust
use afrobim_visibility::*;

// 1. The obstruction scene of the document: ground TINs, corridors, sign slabs, posts, gantries...
//    `solids_of` returns an element's solids in its own frame (mm), e.g. from the module registry.
let solids_of = |e: &Element| modules.generate(e).map(|g| g.solids).unwrap_or_default();
let scene = scene_from_document(&doc, &solids_of, &SceneOptions::default());

// 2. One sign, or every placed sign on every alignment.
let x_height_of = |e: &Element| modules.generate(e).ok().and_then(|g| derived_x_height(&g.derived));
let ctx = SignContext { doc: &doc, scene: Some(&scene), x_height_of: &x_height_of };
let one: SignAnalysis = analyse_sign(&ctx, sign_id, None);
let run: RoadRun = run_all(&ctx, &RunOptions { default_inputs: Some(inputs), only: None });
std::fs::write("visibility.csv", run.summary_csv())?;   // one row per sign
std::fs::write("visibility-lines.csv", run.detail_csv())?; // one row per check line
for line in run.report() { println!("{line}"); }            // table, then per-sign lines

// 3. Inputs kept on the sign (hidden from generic editing).
let mut i = VisibilityInputs::default();
i.eye_offset = Some(EyeOffset::LaneCentre(1.75));
i.approach_speed_kph = Some(80.0);
i.uk_band = Some(SpeedBandMph::From41To50); // the engineer's choice, not derived
save(&mut doc, sign_id, &i);

// 4. Drawings, labels returned separately (both in mm at full size).
let plan = plan_envelope(&one, Some(&scene)).unwrap();
let profile = sight_line_profile(&one, Some(&scene), 5.0).unwrap();
for l in &plan.labels { /* l.at in drawing units, l.text, l.kind */ }
```

Without a document the geometry works on its own (`sweep::Sweeper`, `FaceGeometry`, `Approach`,
`ObstacleScene::add_mesh_m`, `add_tin`, `add_body`), as in the tests and in the crate documentation.
`ObstacleScene::add_body` takes the coordination crate's `Body` (world millimetres), so the scene
the app already builds for clash detection can be reused: add those bodies, skip the ground
surface and corridor bodies if `scene_from_document` is not used, and call `finish()`.

A record in the model-wide check (`CHECKALL`) can map `RoadRun` rows to its rows by `SignAnalysis`:
`overall()` (worst status), `lines` (`CheckLine` with `id`, `status`, `title`, `detail`,
`citation`), `clear_full()`.

## Limits (stated plainly)

- The road surface is the **centreline level of the profile**; cross-fall and super-elevation are
  not applied to the eye, the road test or the mounting height. A sign's mounting height is
  measured from that level, not from the highest point of the carriageway the manuals name.
- The face origin is the element's site origin, the lower-left corner as the app draws it. A sign
  whose recorded offset refers to its post or its near edge needs the offset corrected first.
- Sight lines go to a lattice of points: an obstruction narrower than the lattice spacing at that
  distance can pass between two lines. Raise `grid` for thin posts. The road profile and lateral
  tests sample every `sample_m` along the line.
- Obstruction is geometric only: no vegetation, no vehicles, no night or lighting effects, no
  glare or specular reflection, no head movement.
- The lateral obstruction is an offset line from the centreline, not a modelled bank. A sign that
  stands beyond the offset is blocked from every position.
- Distances are along the alignment to the face centre's station; the sweep stops where the
  alignment starts. It does not follow the driver through a junction onto another alignment.
- Mounting, clearance and orientation lines depend on inputs the sign does not hold
  (`road_edges`, `over_footway`); without them they say `INPUT NEEDED`.
- The UK band for a km/h speed, the hazard station, the lane, and which provision applies to a
  sign are the engineer's. The check lines compare and cite; they do not decide.
- Corridors are rebuilt from their definition when the scene is built, which is slow on a large
  model (`SceneOptions::include_corridors`).
- Wired into `afrobim-app` (`visibility_ui.rs`): the `VISIBILITY` dialog (inputs stored on the sign as `check.visibility`), `VISIBILITYRUN`, the results and drawings windows, the 3D overlay of the limiting sight line, the `CHECKALL` rows and the sheet detail feeds. Since round 4 the scene build and the run of the app's `VISIBILITY` and `VISIBILITYRUN` commands (one sign or the road) are done on a worker thread, with a progress window and Cancel, and are applied only if the document is unchanged (`VisInput::compute`); `CHECKALL` still builds the scene on the interface thread. Pointer interaction was not exercised, only the functions the windows call. Debug builds only were run (no release build, by rule).
