# afrobim-gantry

Sign gantries (portal and cantilever) as AfroBIM model elements that carry signs over the
carriageway: model, setting out on an alignment, hosted signs, a sheet elevation, 3D solids,
quantities, and requirement checks against the UK DMRB.

**This is not a structural design: passing the requirement checks does not show that the gantry,
its connections or its foundations are structurally adequate.** A gantry is a highway structure;
it needs Eurocode analysis, fatigue assessment and technical approval. Legs, boom and foundations
here are *schematic envelopes of entered dimensions*.

## What it does

- **Model** (`GantryDefinition`, the `data` of a `gantry` element): type (portal: a boom between
  two legs; cantilever: a boom from one leg), alignment and station, skew, leg offsets from the
  centreline, leg and boom envelopes (box or circular) and levels, optional foundation blocks,
  optional mass per metre of each member, the carriageways under the gantry (edge offset, lanes
  with width and direction, hard strips), the carried signs, and the inputs for the checks.
  Serde round-trips; `notes()` validates.
- **Setting out** (`setting_out`, `site`): plan position, direction of the gantry line and
  reference level (the profile's centreline level at the station) from an
  `afrobim_alignment::Alignment`.
- **Signs on the gantry** (`add_sign_to_gantry`, `remove_sign_from_gantry`, `sync_gantry`): a
  sign is positioned by offset or lane number and by the height of its lower edge above the boom
  underside. Hosted signs get `host`, their `SitePlacement` and a property from the gantry and
  follow it when it moves. A sign with post supports is refused.
- **Geometry**: `drawing()` (sheet elevation) and `solids()` (`Mesh3` legs, boom, foundations).
- **Quantities** (`GantryQuantities`, `gantries_to_csv`). No rates.
- **Requirement checks** (`design::GantryChecks`) with a line-by-line report, each line cited.
- **`GantryModule`** (category `gantry`) and `add_gantry` for a `Document`.

Only the single portal and the single cantilever are modelled. CD 365 Revision 1 (Terms and
definitions, "Gantry") also names multiple portals, double cantilevers and combinations; they are
not in this cut.

## Conventions

| | |
|---|---|
| Offsets | metres, square to the alignment at the gantry's station, positive to the right looking up-station |
| Gantry line | the plan line the legs and boom stand on. Skew `s` (degrees) turns it counter-clockwise seen from above: a point at square offset `o` is `o·tan s` up-station of the station and `o / cos s` along the line |
| Reference level | the alignment profile's centreline level at the station; 0 when there is no profile. All gantry levels are entered as heights above it |
| Local frame (solids, `site`) | origin on the gantry line at the leftmost extent, at the reference level; +X along the gantry line towards increasing offset, +Y square to it, +Z up; mm. `site.rotation_deg` = heading − 90° + skew |
| Elevation | looking along local +Y, developed along the gantry line; lowest point on y = 0 |
| Carried sign | face parallel to the boom, on the boom face towards the oncoming traffic it serves. Up-station traffic: rotation = heading − 90° + skew; down-station: + 180°. The direction is the lane's (enter lanes keep-left for the UK and Uganda) or stated on the mount |
| Carried sign `site` | `x`, `y`: lower-left corner of the face; **`z`: level of the sign's lower edge** (for post-mounted signs the supports crate reads `z` as ground level; a gantry sign has no ground point) |

## Governing sources

Downloaded with `curl` from `https://www.standardsforhighways.co.uk/tses/attachments/<id>` on
14 November 2026, read as text (`pdftotext -layout`) and the pages relied on rendered and read as
images. PDFs are not in the repository.

| Document | Revision obtained | Status of the copy | Attachment id | Used for |
|---|---|---|---|---|
| CD 127 Cross-sections and headrooms | Version 1.0.1, published 29 July 2021 | served without a superseded mark | `10442706-b592-42c8-85f8-2a0c779a8e37` | headroom values, sag compensation, structure free zone |
| CD 365 Portal and cantilever signs/signals gantries | Revision 1, published March 2020 | **watermarked SUPERSEDED** | `6df58b73-a71f-48dc-8b76-b073220c8702` | gantry clauses listed below |
| CD 377 Requirements for road restraint systems | Revision 4, published January 2021 | **watermarked SUPERSEDED** | `1fe48581-82ba-4b6f-95a1-ee93309bd1b5` | read only; **no value taken from it** |

**The current CD 365 was not obtained.** Third-party indexes list a Version 1.1.0 (September
2025) as superseding Revision 1; the site's document search could not be used from the build
environment and no link to that version was found. Every CD 365 clause number and requirement in
this crate is therefore from Revision 1 and **must be confirmed against the current version
before it is relied on**. The calculation report says this on its first line. The same applies
to CD 377 (successor Version 5.0.x not obtained), which is why nothing numerical is taken from
it. The currency of CD 127 Version 1.0.1 was judged only from the absence of a superseded mark
and from the DMRB index of 15 October 2023; confirm it too.

### Citations: every number from a standard

| Value in code | Source |
|---|---|
| `CD127_T41_GANTRY_NEW_CONSTRUCTION_MM` = 5700 | CD 127 v1.0.1 Table 4.1, row "Footbridges, sign and signal gantries, and other structures vulnerable to vehicular impact", new construction headroom 5.70 + S |
| `CD127_T41_GANTRY_MAINTAINED_MM` = 5410 | same row, maintained headroom 5.41 + S |
| `CD127_T41_HIGH_LOAD_NEW_CONSTRUCTION_MM` = 6450 | CD 127 v1.0.1 Table 4.1, row "All permanent structures over high load routes (not applicable in NI)", 6.45 + S |
| `CD127_T41_HIGH_LOAD_MAINTAINED_MM` = 6180 | same row, 6.18 + S |
| `CD127_T43_SAG_COMPENSATION` = 1000→80, 1200→70, 1500→55, 2000→45, 3000→25, 6000→15 mm; ≤ 1000→80; > 6000→0 | CD 127 v1.0.1 Table 4.3 |
| `CD365_3_23_OVERBRIDGE_HEIGHT_FACTOR` = 2 | CD 365 r1 cl. 3.23 |
| 60 / 30 years, + 10 years (text of a listed requirement only) | CD 365 r1 cl. 3.2, 3.3 |

The `PLACEHOLDER_*` constants are round starting dimensions from no standard, flagged by a note
on every gantry built from them.

### What the checks cover

| Check | Clauses | Outcome |
|---|---|---|
| Headroom: lowest point of boom, carried signs and entered equipment, less the highest surface under the gantry and the entered SLS deflection, made square to the surface, against Table 4.1 + S | CD 127 v1.0.1 cl. 4.1, 4.2, 4.3, 4.9, 4.11, Tables 4.1, 4.3, definition of "Headroom"; CD 365 r1 cl. 3.19 | OK / FAILS; INCOMPLETE when the deflection has not been entered |
| Distance from the edge of the paved width to each leg's near and far face | CD 127 v1.0.1 cl. 4.5, 4.6 | reported, no pass or fail |
| Dual carriageway, non-passively-safe portal: no leg between the carriageways unless agreed with the TAA | CD 365 r1 cl. 3.21 | OK / FAILS |
| Portal: beam ends at the same level | CD 365 r1 cl. 3.22 | OK by construction (one boom level) |
| Clear distance to an over-bridge ≥ 2 × maximum height, unless wind interaction is in the design | CD 365 r1 cl. 3.23 | OK / FAILS; a requirement with the distance when none is entered |
| Listed for the engineer, not checked: technical approval; Eurocode design and fatigue; type selection; working life; vehicle restraint to CD 377; clearances to restraint systems; set-back of verge supports; maintenance; permanent access; foundation reserves; deformation limits and pre-camber | CD 365 r1 cl. 3.1; 6.1, 6.10; 2.1, 2.2; 3.2, 3.3; 4.1; 3.20; 7.28.1; 3.16, 3.18; 9.1; 6.23; 6.6, 6.7, Table 6.7 | REQUIREMENT |

Headroom arithmetic:

```text
required  = Table 4.1 value + S (Table 4.3)
vertical  = lowest point − surface rise − SLS deflection        (all above the reference level)
headroom  = vertical / √(1 + grade² + cross-fall²)
OK when headroom ≥ required
```

`grade` is the profile's at the station (0 without an alignment or profile). The surface rise,
cross-fall, sag radius and deflection are entered; none is computed.

**Not checked because the documents held give no implementable value:** a limit on span or
outreach (CD 365 r1 states none; NOTE 3 to its Section 10 only remarks that dynamic effects become
significant around 20 m spans for conventional steel gantries); a minimum set-back of legs from
the carriageway (CD 365 r1 cl. 7.28.1 says "should be set back" with no distance; the numbers are
in CD 377, of which only a superseded revision was obtained); the need for and containment of
vehicle restraint (CD 377, by risk assessment).

## Ambiguities met, and how each was read

1. **Table 4.3 between tabulated radii.** CD 127 does not say whether to interpolate. The code
   does not: a radius between rows takes the S of the smaller tabulated radius (the larger S).
2. **"Greater than" or "not less than".** CD 127 cl. 4.2 says the headroom after deflection
   "shall be greater than the minimum standard headroom"; cl. 4.9 says "equal to or greater
   than". The code passes a headroom exactly equal to the requirement.
3. **Headroom is "measured at right angles to the surface of the carriageway"** (CD 127
   definitions). The model's levels are vertical; the code divides by √(1 + grade² + cross-fall²),
   which is exact for a plane surface. On a 6 % grade this takes 10 mm off 5.70 m.
4. **Where the headroom applies.** Cl. 4.1 applies it "above the paved width, verges and central
   reserve". The code takes the single lowest point of boom, signs and equipment against the
   single highest entered surface level, wherever each is. This is safe but can be severe for a
   sign that hangs low over a verge.
5. **Allowance for deflection.** CD 365 r1 cl. 3.19 says "allowing for clearances and
   deformations defined in Section 6". Table 6.7 gives *limits* on deformation, not an allowance,
   and its vertical limit ("1/300 of span plus height of support") can be read as (span + height)
   / 300 or span / 300 + height. Nothing is derived from it: the engineer enters the SLS
   deflection from the structural analysis.
6. **"Maximum height" in CD 365 r1 cl. 3.23** is not defined. It is taken as the highest point of
   boom and signs above the lowest leg base.
7. **"Outermost face" in CD 127 cl. 4.6(2)** is read as the face of the leg furthest from the
   paved width; both faces are reported.
8. **"Dual carriageway" for cl. 3.21** is taken as two or more carriageways entered, and "central
   reserve" as anywhere between the first and last paved widths.
9. **Support height.** CD 365 r1 cl. 6.9 measures to the underside of the main member "or
   signs/signal"; the reported support height is to the boom underside only.
10. **Signs on a skewed gantry** are placed parallel to the boom. The standards held do not say
    whether a sign on a skewed gantry is to be turned square to the traffic.
11. **Kernel convention.** `SitePlacement.z` means ground level for a post-mounted sign and the
    lower edge for a gantry-hosted one (see Conventions). The existing 3D view adds a mounting
    height read from the sign's posts, which is 0 for a sign without posts, so it draws a
    gantry-hosted sign at the right level as it stands.
12. **Posts added to a gantry-hosted sign.** `afrobim-supports` does not know about gantries and
    will add posts if asked. This crate cannot refuse that; `support_conflicts` finds such signs.

## How quantities are computed

| Quantity | Formula |
|---|---|
| Span (portal) | distance between the legs' inner faces along the gantry line: (o₂ − o₁) / cos(skew) − (w₁ + w₂) / 2 |
| Reach (cantilever) | from the traffic face of the leg to the tip along the gantry line: \|o_tip − o_leg\| / cos(skew) − w / 2 (CD 365 r1 definition of "Outreach of cantilever") |
| Leg length | base to top of boom: boom underside + boom depth − base level |
| Support height | base to boom underside (CD 365 r1 cl. 6.9) |
| Boom length | equal to the span or reach |
| Signs | number, and Σ width × height of the sign elements' bounding rectangles |
| Foundation concrete | Σ length × width × depth of the entered blocks |
| Steel mass | Σ leg length × its kg/m + boom length × its kg/m, **only** when every member has a mass per metre entered. Never density × envelope |

The schematic leg runs past the end of the boom to the boom's top, so lengths do not double-count
the corner. `gantries_to_csv` writes one row per gantry and a `TOTAL` row; several legs share a
cell separated by `;`.

## Limits

- Straight gantry line through one station; levels from the centreline profile only. Cross-fall
  and verge levels are not modelled: the road line is level, and the engineer enters the surface
  rise and each leg's base level.
- One boom level; no pre-camber, no inclined or stepped booms.
- Two legs or one. No twin portals, double cantilevers, or legs in pairs.
- The elevation carries no text or dimensions figures (the drawing primitives have no text);
  values are in the module's derived values and the report.
- Sign faces are bounding rectangles.

## Not done in this cut

Structural analysis and member design; wind, collision and other actions; deflections; fatigue;
foundation design; passive safety assessment; variable message signs and signal equipment and
their loads; access walkways, ladders and handrails; lighting of signs; electrical works, cabling
and earthing; vehicle restraint design; costs and rates.

## Using it from the app

```rust
use afrobim_gantry::{add_gantry, add_sign_to_gantry, sync_gantry, GantryDefinition, GantryModule, GantryQuantities, SignPosition};
use afrobim_gantry::design::GantryChecks;

registry.register(Box::new(GantryModule));                          // once

let def = GantryDefinition::placeholder_portal(&alignment.name, station_m);   // then edit
let gantry = add_gantry(&mut doc, "G1", &def, Some(&alignment))?;
add_sign_to_gantry(&mut doc, gantry, sign_id, SignPosition::Lane { index: 2 }, -150.0, None, Some(&alignment))?;

// After the gantry, the alignment, its profile or a carried sign changes:
sync_gantry(&mut doc, gantry, Some(&alignment))?;

let def = afrobim_gantry::definition_of(&doc, gantry)?;
let report = GantryChecks::run("G1", &def, Some(&alignment))?.calculation();   // Vec<String>
let (rows, skipped) = GantryQuantities::measure_all(doc.elements());
let csv = afrobim_gantry::gantries_to_csv(&rows);
```

3D site view: draw `Generated::solids` of a gantry element in the frame given by its `site`
(origin `x, y, z`, rotation `rotation_deg` about +Z), with no lift.
