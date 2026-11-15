# AfroBIM round 4 plan: remaining planned items

Date: 2026-11-15. Owner: AfroBIM. Status: agreed plan, not started.

Round 4 builds what waves 1 to 3 left open. Planning, briefs, merging and review are done by
the lead session. Building is done by subagents on Sonnet (engineering and application code)
and Haiku (documentation and mechanical work), each in its own git worktree.

## Decisions taken (2026-11-15)

| Question | Decision |
|---|---|
| Scope | All four groups: CAD basics, more road markings, South African bases, polish and gaps |
| Structural design of gantries and lighting columns | Stays excluded. AfroBIM checks geometry and requirements only |
| km/h limit against UK mph bands | Round up to the next band (80 km/h is treated as 50 mph). A project setting, changeable, stated in every calculation that uses it |
| CI | The owner clears the GitHub Actions billing or usage problem before building starts |
| South African sources | Fetch public copies of SAPEM, TRH4 and the SANRAL Drainage Manual. What cannot be fetched and read stays as user input and is reported |
| Sign schedule import | An AfroBIM CSV template, plus a column-mapping step for any other layout |
| Distribution | Proper installers, unsigned: Windows .msi and macOS .dmg |
| Units | Metric display options only (mm or m, decimals, station format, area and volume units, angle format). Stored values stay metric |

## Start condition

Building starts when CI runs again on PR #22. Nothing since commit 8a2e4e0 has been verified on
Windows or macOS, and the installer work package depends on working CI.

## Work packages

Rules carried by every brief: no engineering value from memory; every value from a standard
carries document, edition and clause or table; inputs stay inputs when the source is not held;
worked examples from the source are reproduced in tests; limits are stated plainly; one agent at
a time edits `afrobim-app`.

### Wave 4A: engines (parallel, no app edits)

| ID | Work package | Model | Crate |
|---|---|---|---|
| R1 | Speed band setting: project-level rule (round up, exact, ask, own table) used by restraint, markings and visibility; stated in each calculation | Sonnet | afrobim-kernel plus the three engines |
| R2 | Road markings, second set: transverse lines (stop, give way), arrows, hatching and chevrons, yellow lines, road studs, from UK Traffic Signs Manual Chapter 5 (2018, held) | Sonnet | afrobim-markings |
| R3 | Road markings, Uganda basis from the Uganda Traffic Signs Manual volumes held in the repository | Sonnet | afrobim-markings |
| R4 | South African pavement basis (SAPEM, TRH4) if the documents can be fetched | Sonnet | afrobim-pavement |
| R5 | South African drainage basis (SANRAL Drainage Manual) if it can be fetched; culvert inlet control from MoWT or a fetched source | Sonnet | afrobim-drainage |
| R6 | Annotation engine: text, dimension (aligned, linear, station and offset), leader, measure; DXF underlay reader (lines, polylines, arcs, circles, text, layers; read-only reference) | Sonnet | new afrobim-annotate, afrobim-dxf |
| R7 | Sign schedule import: AfroBIM template, column mapping, validation report | Sonnet | afrobim-signs or afrobim-fabrication |
| R8 | IFC linear placement along the alignment, and alignment curve geometry in the export | Sonnet | afrobim-ifc |

### Wave 4B: application (in sequence)

| ID | Work package | Model |
|---|---|---|
| U1 | Annotate tab live: text, dimension, leader, measure; DXF underlay; layer manager; units; sign schedule import with column mapping; speed band setting in project settings | Sonnet |
| U2 | New marking types and the Uganda and South African bases in the dialogs; culvert inlet control in the checks; report pack fix (only relevant inputs per check); slow analyses on a worker thread with progress | Sonnet |
| U3 | Installers: Windows .msi and macOS .dmg built by CI on demand, unsigned, with a note on first-run warnings | Sonnet |
| U4 | User manual rewritten in full from the app as built, including a first-hour walkthrough and every limit | Haiku, reviewed by the lead |

## Order and dependencies

1. R1 to R8 run in parallel (new or separate crates).
2. U1 then U2 (both edit the app). U3 can run beside them since it touches CI and packaging
   files only.
3. U4 last, when the app is final for this round.
4. The lead merges each job, runs the full test suite, looks at screenshots, and pushes.

## Not in this round

- Structural design of gantries and lighting columns (excluded by decision).
- Code signing and licensing for a product for sale.
- Imperial units.
- EN 13201-3 photometry, BCF 3.0, IFC boolean geometry.

## Still needed from the owner

- GitHub Actions working again (start condition).
- MoWT Road Design Manual Volume 1, or a limits CSV, for alignment checks.
- Current DMRB CD 224, CD 226, CD 365; BS 5489-1:2020; BS EN 13201-2 and -3; PD 6547 Table 2;
  BS EN 40-2 Table 7; a MoWT wind map.
- A sample of an existing sign schedule, to test the column mapping against a real layout.
- A try of the exported IFC, sheet PDF, LandXML, MS Project and P6 files in the real software.
