# AfroBIM roadmap against the BIM workflow

Date: 2026-11-14. Owner: AfroBIM. Status: working plan, revised as work lands.

AfroBIM is to cover the whole BIM workflow from design to construction, for roads first. This
plan maps what exists to the eight stages, lists what is missing, and orders the work.

## Working method

- Planning, briefs, merging and review: the lead session.
- Building: subagents on Sonnet for engineering and application code, Haiku for documentation
  and mechanical work. Each works in its own git worktree and commits locally; the lead merges,
  tests and pushes.
- One agent at a time edits `afrobim-app`. New capability goes in its own crate first.
- Rules that every brief carries: no engineering value from memory; every value from a standard
  carries document, edition and clause; inputs stay inputs when the source is not held; worked
  examples from the source are reproduced in tests; limits are stated plainly.

## Stage by stage

| # | Stage | In AfroBIM today | Gap | Work package |
|---|---|---|---|---|
| 1 | Concept design | LandXML alignments, profiles, surfaces; corridor crate | No way to sketch an alignment or a massing option inside AfroBIM; no option comparison | C1 alignment sketching (PIs, curves, grades); C2 option comparison (length, earthworks, cost) |
| 2 | BIM modelling | Signs (UK, Uganda), supports, markings, drainage, gantries; crates for lighting, restraint, corridor, pavement; IFC 4.3 export | Four crates not yet in the app; IFC covers signs, supports and alignments only; no IFC import | M1 wire lighting, restraint, corridor, pavement; M2 IFC export of every module; M3 IFC import as reference |
| 3 | Coordination | 3D site view | No clash detection; no federated reference models | K1 clash detection between elements (hard and clearance); K2 issue list with BCF export |
| 4 | Analysis | Sign support wind design; drain capacity; pavement design (three methods); gantry and restraint and lighting requirement checks | No sight-line or sign visibility analysis; no photometry; checks are run one element at a time | A1 run every check on the model and report; A2 sign visibility along the alignment; A3 photometry from luminaire files |
| 5 | 4D (time) | Nothing | Programme, links from elements to activities, timeline playback | T1 schedule crate (activities, dependencies, critical path, calendar); T2 link elements, play the build in 3D; T3 import and export (MS Project XML, P6 XER) |
| 6 | 5D (cost) | Quantities per module as CSV | No bill of quantities, no rates, no estimate | Q1 cost crate: one bill from all module quantities, rate library entered by the user, estimate, cash flow from T1 |
| 7 | Documentation | Sign fabrication sheets and schedule (DXF, PDF); DXF export | No drawing sheets for the road modules; no title block; no report pack | D1 sheet composer (title block, views, schedules, PDF and DXF); D2 calculation report pack; D3 user manual |
| 8 | Construction | Nothing | Progress against the programme, site records, as-built | S1 progress crate (percent complete per element and activity, earned value against T1 and Q1); S2 site records (inspections, photos, non-conformances) linked to elements |

## Order of work

Wave 1 (now):

1. M1: wire lighting, restraint, corridor and pavement into the app through the shared module
   glue. Sonnet.
2. K1: clash detection crate. Sonnet.
3. T1: schedule crate. Sonnet.
4. Q1: cost crate. Sonnet.
5. D3: user manual from the command table and crate READMEs. Haiku.

Wave 2 (after wave 1 merges):

6. App wiring of K1, T1, Q1: Coordination, 4D and 5D ribbon tabs. Sonnet.
7. A1: model-wide check report. Sonnet.
8. D1: sheet composer and title block. Sonnet.
9. S1: progress and earned value. Sonnet.
10. M2: IFC export of every module. Sonnet.

Wave 3:

11. C1, C2: alignment sketching and option comparison.
12. T2, T3: 4D playback and programme exchange.
13. K2, M3: issues with BCF, IFC import as reference.
14. A2, A3, D2, S2.

## Carried over from earlier work

- South African bases for pavement (SAPEM, TRH4) and drainage (SANRAL): seams are in place.
- Current DMRB CD 224, CD 226 and CD 365 were not obtainable; the code uses superseded
  revisions and says so. Replace when the current PDFs are supplied.
- BS 5489-1:2003 is superseded; lighting class performance values (BS EN 13201-2) are inputs.
- Road markings: transverse lines, arrows, hatching, yellow lines, studs, Uganda manual.
- Structural design of gantries and lighting columns is out of scope by decision.
- Nothing has been tested against a Civil 3D LandXML export or opened in a BIM viewer.

## What "done" means for a work package

Crate with tests that pass on Windows, macOS and Linux; README with sources, formulae and
limits; wired into the ribbon with an icon and a command; shown in the 3D site view where it has
geometry; quantities under Export quantities; an entry in the workspace README status table.
