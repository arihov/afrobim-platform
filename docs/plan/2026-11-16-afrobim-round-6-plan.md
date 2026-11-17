# AfroBIM round 6 plan: the remaining civil components

Date: 2026-11-16. Owner: AfroBIM. Status: in progress.

Round 5 (merged as pull request 8) made points, surfaces, alignments, profiles, assemblies,
corridors, sample lines, quantities and parcels live. Round 6 adds the components it left out and
closes the gaps it reported.

## Wave 6A: engines (parallel, each in its own worktree, no application edits)

| ID | Work package | Crate |
|---|---|---|
| P1 | Pipe networks: structures, pipes, parts lists, rules, hydraulic and cover checks, interference, LandXML | new afrobim-pipes |
| P2 | Grading: feature lines, grading criteria, grading groups, infill, volume balance | new afrobim-grading |
| P3 | Intersections: at-grade junctions and roundabouts from alignments, kerb returns, profiles | new afrobim-junction |
| P4 | Survey database: control, setups, observations, field book import, least squares adjustment, figures | new afrobim-survey |
| P5 | Point clouds: LAS, LAZ, E57 and text readers, level of detail, classification, ground extraction | new afrobim-pointcloud |
| P6 | Reprojection: datum transformations fetched from the EPSG registry, an explicit and logged transform operation | afrobim-kernel |
| P7 | Data shortcuts: read-only references to objects in other project files, with stale detection | new afrobim-shortcuts |

## Wave 6B: application (in sequence)

| ID | Work package |
|---|---|
| A1 | Pipe networks, grading and intersections live in the toolspace, ribbon and views |
| A2 | Survey database, point clouds, data shortcuts and reprojection live |
| A3 | Round 5 gaps: IFC mapping of civil objects, corridor targets in the dialogs, the five partly rebuilt dialogs, wide tables |
| A4 | User manual |

## Rules carried by every brief

No engineering or geodetic value from memory; every value from a standard carries document,
edition and clause, table or code; inputs stay inputs when the source is not held. Objects stay at
source coordinates unless the user runs an explicit, logged transform. No em dashes.
