# afrobim-app

The AfroBIM desktop application: an egui / eframe 0.31 program (`afrobim` binary, no library) that
puts the engine crates behind one window with AutoCAD-style interaction. It holds no engineering
rule of its own: every calculation, catalogue, limit and drawing comes from the crate that owns it,
and the app's code is the wiring: dialogs, windows, the command table, painting and file handling.
Start with `afrobim/README.md` for what the platform does and `afrobim/docs/user-manual.md` for how
to use it; this file says where things are in the code.

```
cd afrobim
cargo run -p afrobim-app -- --demo          # sample signs and arbitrary demo road data
cargo run -p afrobim-app -- road.afrobim    # open a document
```

## Where things are (`src/`)

| Group | Modules |
|---|---|
| Shell | `main.rs` (command-line options, see below), `app.rs` (the `AfroBimApp` state, the frame loop, shortcuts, the command line, menus and the status bar), `ribbon.rs` (17 tabs of icon buttons with hover cards and the shortcut table), `commands.rs` (the one command table: 169 commands with aliases, help and the `Action` each runs), `icons.rs`, `win.rs` (every floating window is made by `win::new`, which opens it inside the central area and keeps it there) |
| Model space | `canvas.rs` (2D painting and the CAD cursor furniture), `render.rs` (element meshes), `view3d.rs` (the 3D site view: camera, painter's-algorithm triangles, world frames of elements), `aids.rs` |
| Signs and supports | `signs_glue.rs`, `supports_glue.rs`, `signsched.rs` (sign schedule import) |
| Alignments and design | `alignment_glue.rs`, `station_input.rs` (displayed to internal stations), `design_glue.rs`, `design_ui.rs` (plan editor and PI table), `design_profile.rs` (profile editor), `design_windows.rs` (limits check, options), `disp.rs` (display units for the design tools and sheet windows) |
| Road modules | `markings_glue.rs`, `ext_markings_glue.rs` and `ext_markings_ui.rs` (second marking set), `modules_glue.rs` (drains, culverts, gantries), `road_glue.rs` and `road_dialogs.rs` (lighting, barriers, pavement, corridors, ground surfaces), `lighting_inputs.rs`, `photometry_ui.rs`, `visibility_ui.rs` |
| Checks and reports | `check_inputs.rs` (inputs kept on elements as `check.*` properties), `check_ui.rs`, `check_all.rs` (the model-wide check), `report_pack.rs`, `projset.rs` (project settings and the speed band rule) |
| Coordination and programme | `coordination_glue.rs`, `coordination_ui.rs`, `issues_ui.rs`, `reference_models.rs` (IFC reference models), `schedule_glue.rs`, `schedule_ui.rs`, `schedule_ext.rs`, `cost_glue.rs`, `cost_ui.rs`, `construction_glue.rs`, `construction_ui.rs` |
| Drawings | `sheets_glue.rs`, `sheets_ui.rs`, `anno_glue.rs` (annotation commands as state machines, object snap), `anno_ui.rs`, `underlay.rs` (DXF underlays) |
| Output | `ifc_options.rs`; the export commands are in `app.rs` |
| Background work | `background.rs`: `Job` runs a closure on a worker thread and `poll` returns `Done` only if the fingerprint of the document is the one taken when the job started (otherwise `Stale`), `Cancelled` or `Lost`. Used for clash runs, the report pack, the sign visibility scene and runs, photometry runs, corridor surface rebuilds and the sections window's corridor. A worker builds what it needs inside itself (`worker_registry`) because modules are not `Send` |
| Demo data | `demo_glue.rs` and the `demo` functions of the glue modules; every number is an arbitrary demo value and says so in a comment |

## Command line

`afrobim [FILE] [--demo] [--landxml FILE] [--3d] [--tab TAB] [--run CMD[,CMD]] [--focus MARK]
[--screenshot out.png] ...` (the full list is `parse_args_from` in `main.rs`; the options are
described in `afrobim/README.md`). `--screenshot` renders a few frames once no background job or
import is running, saves the window and exits, which is how the interface is checked on a machine
with no display (`xvfb-run -a afrobim --demo ... --screenshot out.png`). The rule pack and fonts are
looked for next to the executable, in the bundle on macOS, and above the executable and the
working directory (`AFROBIM_PACK`, `AFROBIM_PACK_UG`, `AFROBIM_FONTS` override).

## How it is tested

`cargo test -p afrobim-app` runs the unit tests inside the modules and the glue test files
`u1_tests.rs`, `u2_*.rs` and the tests of `background.rs`, `disp.rs`, `anno_glue.rs`,
`sheets_ui.rs`, `visibility_ui.rs` and `photometry_ui.rs`. What they exercise is the code the
windows and the pointer events call: command parsing, glue between the engines and the document,
the formatting and parsing of display units, object snap candidates, the export of IFC and
quantities, and the background jobs' worker functions and polling. Some tests validate an export
with IfcOpenShell when `python3` can import it (and say so when it cannot). The windows themselves
are checked by headless screenshots looked at by a person, not by an automated test.

## Limits

- Pointer interaction (drags, hover, the 2D and 3D canvases) cannot be driven in a test or a
  headless run; only the functions the pointer events call are tested. The wiring of the pointer
  to them has been looked at only through screenshots.
- No test builds an `AfroBimApp` or runs a window: the code that applies a finished background job
  to the app (the `apply_*` methods) is covered by the build and by screenshots, not by a test.
- The 3D view paints triangles with a painter's algorithm (sorted by centre, the ground first, a
  triangle with a vertex behind the camera is dropped), so a close view of a large surface can
  lose triangles. It is a view, not a renderer.
- Debug builds and Linux only have been run; the Windows and macOS builds have not run since the
  hosted runners stopped starting.
- No file written by the app has been opened in Civil 3D, a BIM viewer other than IfcOpenShell's
  checks, a BCF tool, MS Project, Primavera P6 or AutoCAD.
