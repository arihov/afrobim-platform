# AfroBIM

A building-information-modelling platform for roads and infrastructure, written in Rust. One
codebase builds for Windows and macOS (and Linux). Traffic signs are the first module.

Status: early. This is the start of the Rust platform that will replace the C# SignCore desktop
app in `../src`. The C# engine remains the reference: the Rust sign engine is checked against its
output primitive by primitive.

## Layout

```
crates/
  afrobim-draw     neutral 2D drawing primitives every module and front end shares
  afrobim-kernel   the BIM document (elements, layers, properties, file format), the module
                   trait, and all GUI-independent editing: view, picking, window / crossing
                   selection, grips, MOVE / COPY / ERASE, snap, ortho, undo
  afrobim-signs    traffic-signs engine: rule pack, Transport font metrics, layout, glyph outlines
  afrobim-dxf      DXF R12 export, byte-identical to the C# writer for the same primitives
  afrobim-fabrication  sign-face quantities, A3 fabrication sheets (DXF and PDF) and the schedule PDF
  afrobim-alignment   horizontal alignments, LandXML import, station / offset placement
  afrobim-markings    road markings: TSM Chapter 5 line catalogue, layout along an alignment, quantities
  afrobim-drainage    side drains and pipe culverts: quantities, hydrology and hydraulics (MoWT, UK DMRB)
  afrobim-gantry      portal and cantilever sign gantries: geometry, hosted signs, requirement checks
  afrobim-supports    posts and foundations: definitions, solids, quantities (no structural design)
  afrobim-ifc      IFC 4.3 export (STEP file) of a document
  afrobim-mcp      MCP server (stdio) so an AI assistant can drive the engine and build documents
  afrobim-app      the desktop application (egui): menus, toolbar, Tool Sets, model space,
                   Properties, Layers, command line
```

Dependencies point one way: `app → signs, kernel → draw`. The kernel does not know what a sign
is. A discipline plugs in by implementing `afrobim_kernel::Module` for one element category.

## The model

A document is a list of **elements**. Each element has an id, a category (`"sign"`), a mark
(`ADS-001`), a placement in model space (millimetres, Y up), a layer, a module-owned definition
(`data`) and a free-form property set (material, drawing reference, cost code). The owning
module derives geometry, checks and read-only values from the definition; nothing is drawn
freehand.

Documents are saved as `.afrobim`: versioned JSON with units, the design standard, layers and
elements.

## Build and run

Rust 1.85 or later.

```
cd afrobim
cargo run --release -p afrobim-app            # the desktop app
cargo run --release -p afrobim-app -- --demo  # start with sample signs
cargo test                                    # kernel, sign engine (golden tests), app
```

The traffic-signs module needs the rule pack `packs/UK-TSM-2019.json` and the Transport fonts
in `fonts/`. Run from a checkout and they are found automatically; otherwise set `AFROBIM_PACK`
(the JSON file) and `AFROBIM_FONTS` (the folder). The fonts are third-party files and are not
part of AfroBIM: do not redistribute them without a licence that allows it.

`--screenshot file.png` renders a few frames, saves the window and exits. It is how the UI is
checked on machines without a desktop.

## Using it

The interaction follows AutoCAD conventions.

| Action | How |
|---|---|
| Select | Click an element. Clicks add to the selection; Shift + click removes; Esc clears |
| Window / crossing | Drag on empty space: left to right takes elements wholly inside, right to left takes elements touched |
| Move | Drag a selected element or a grip, or `M`: base point, second point |
| Copy | `CO`: base point, a point per copy, Enter to finish. Ctrl+C / Ctrl+V also work |
| Erase | Delete, or `E` |
| Insert a sign | Tool Sets, the Insert menu, or `STACK`, `MAP`, `TEE`, `ROUNDABOUT`, `DIRECTION`, `CONFIRM`, `PLACE`, `LANE`, `DIVERSION`; the sign hangs from the cursor; click to place |
| Edit | Select and use Properties; the model updates as you type |
| Zoom / pan | Wheel zooms about the cursor; middle button pans; `Z` extents; `ZW` window |
| Undo / redo | Ctrl+Z / Ctrl+Y |
| Drafting aids | F7 grid, F9 snap (50 mm), F8 ortho |
| 3D view | `3D` or the 2D / 3D switch in the status bar: drag to orbit, Shift + drag or middle button to pan, wheel to zoom, double-click to fit |

Type over the drawing to use the command line: a name or alias runs a command, `?` lists them,
an empty Enter repeats the last one. While a command waits for a point, type `x,y` (absolute),
`@dx,dy` (relative) or `@distance<angle`. Millimetres.

## What is ported, and what is not

| Area | State |
|---|---|
| Kernel, interaction, undo, file format | Done, unit-tested |
| Desktop shell | Done; checked by headless screenshot on Linux. Not yet run on Windows or macOS |
| UK direction signs: stack-type, map-type (crossroads, T-junction, roundabout), direction, route confirmatory, place identification, lane pre-selection, diversion | Ported from the C# engine, golden-tested against it |
| UK standard signs: the pack's catalogue, the 24 TSRGD warning and regulatory diagrams with plates, supplementary plates, information signs, and the 66 destination symbols | Ported, golden-tested. Insert from Tool Sets → Standard signs, or `LIBRARY` |
| Golden fixtures | 444 signs compared with the C# engine primitive by primitive; worst deviation about 3e-14 sw |
| Warning triangles and regulatory roundels embedded on direction and map-type signs | Ported, golden-tested. Drawn as the standard frame only, as in the C# engine: the inner symbol is not drawn |
| Uganda rule pack (`packs/UG-TSM-2004`, XML) | Ported: loader and all 16 sign kinds, 105 catalogue entries, 241 golden fixtures. Switch with Format → standard, or `UK` / `UGANDA`. Embedded signs under the Uganda pack are not ported |
| DXF export (R12, mm, one layer per sign colour) | Ported; File → Export DXF or `DXFOUT`. Matches the C# writer byte for byte on the reference files |
| Quantities (BoQ) | Ported: per-sign CSV and per-colour summary, byte-identical to the C# output. File → Export quantities, `QUANTITIES` |
| Fabrication sheets | Ported: an A3 sheet per sign as DXF and PDF, plus the schedule PDF. DXFs are byte-identical to C#; PDFs match object for object apart from dates, ids, the producer name and compression. File → Export fabrication sheets, `SHEETS`, or `--export-sheets folder` |
| Ribbon | Tabs Home, Edit, Insert, Annotate, Modify, Signs, Roads, Structures, Output, Manage, View and Help, each a row of panels of icon buttons, above the classic menus. Every button runs a command from the one command table and shows a preview card on hover (what it does, the name to type, the shortcut). Tools that are not built yet (text, dimensions, road markings, drainage, pavement, barriers, lighting, corridor, gantries, clash check and others) are shown disabled and marked "Planned" |
| Alignments | New (not a port): tangents, circular curves and clothoid spirals; vertical profiles (PVIs with parabolic, asymmetric and circular curves); station equations (displayed and internal stations, gaps and overlaps); superelevation as stored cross-fall data. LandXML 1.2 import of all four, names checked against the schema (File → Import LandXML, `LANDXML`). A sign is placed by displayed station, offset and side (`STATION`) and takes the profile's centreline level. The placement rules are ported from C# and match it on 4,256 cases. **Not tested against a file exported by Civil 3D.** No superelevation design: no rates are computed, and LandXML states the cross-fall only at full superelevation |
| Road markings | New (not a port): longitudinal lines along an alignment to the Traffic Signs Manual Chapter 5, 2018 edition (`MARKINGS`, Roads tab): centre lines 1008 and 1008.1, lane lines 1005 and 1005.1, warning lines 1004 and 1004.1, edge lines 1009A, 1010 and 1012.1, and double white lines 1013.1 (continuous, and continuous with broken). Mark, gap and width options are those of the manual's Figures 2-1 to 2-6 and 3-1 and Tables 2-1 to 2-5, each cited. Shown in the 3D site view at the profile level; measured under Export quantities (length, marks, painted length and area). The tables are in mph: mapping a km/h limit to them is the engineer's decision. **Not covered:** stop and give-way lines, arrows, worded markings, hatching, yellow lines, road studs, box junctions, the Uganda manual's markings |
| Drainage | New (not a port): side drains along an alignment (`DRAIN`) and pipe culverts at a station (`CULVERT`), added with placeholder dimensions and edited in Properties. Quantities: drain length, excavation and lining; culvert pipe lengths. `CHECK` on a drain gives the capacity for an entered design flow by Manning's equation, to the MoWT Road Design Manual Vol. 2 (2010) or UK DMRB (CG 501, CD 521), the engineer's choice. The crate also holds the MoWT Rational method, time of concentration and pipe culvert outlet control. **Rainfall intensity and design flow are inputs.** Not covered: culvert inlet control, box culverts, headwalls, scour, subsurface drainage. A South African basis can be added beside the two |
| Gantries | New (not a port): portal and cantilever sign gantries at a station (`GANTRY`, `CANTILEVER`) with entered member envelopes; signs hang on a gantry over a lane or at an offset (`HANG`, `UNHANG`) and move with it. `CHECK` reports headroom to UK DMRB CD 127 (v1.0.1) and listed requirements of CD 365. **The CD 365 copy obtained is Revision 1 (2020), now superseded: confirm each clause against the current version. No structural design: members, wind loads, fatigue and foundations are not designed, and passing the checks does not show a gantry is adequate** |
| Posts and foundations | New (not a port): elements with dimensions the engineer enters, solids, and steel, concrete and excavation quantities by geometry (`SUPPORTS`). New supports carry placeholder dimensions and say so until they are edited or designed |
| Support design | New (not a port): `DESIGN` sizes S355 steel CHS posts, with either a planted foundation per post or one spread foundation under the sign, for wind to the UK method (IHE Sign Structures Guide 2021, BS EN 12899-1 and its National Annex) and shows the calculation. It reproduces the guide's worked Examples 1 and 2. Basic wind pressure for Uganda is offered from MoWT Road Design Manual Vol. 4 (2010) Table 2 (0.81 kN/m² open terrain, 0.37 kN/m² built-up, up to 5 m), or entered. For planted foundations the ground factor G and the BS EN 40-2 minimum planting depth are the engineer's inputs; spread foundations use the guide's assumed bearing resistances (Tables 5.2, 5.3), geotechnical checks only. **Not covered:** foundation concrete and reinforcement, slopes, funnelling, passively safe posts, eccentric signs, AASHTO sections, other steel shapes. The output is a calculation to be checked, not a certified design |
| 3D site view | Once a sign is placed on an alignment, the 3D view shows the site: alignments on the ground, signs at their stations facing their traffic, on their posts and foundations |
| Schedule CSV / XML import and export | Not ported in the app (the MCP server reads schedule CSVs). Use the C# CLI |
| 3D view | Done: View → 3D, the `3D` command or the status-bar switch. Orbit, pan, zoom, fit. View only; editing stays in 2D |
| Python package | Done: `afrobim/python` (PyO3, built with maturin); see its README |
| MCP server | Done: `afrobim-mcp` (11 tools: catalogue search, sign layout and checks, schedule validation, DXF export, and building `.afrobim` documents); see `crates/afrobim-mcp/README.md`. The five tools shared with the C# server returned identical results on 126 requests |
| IFC export | File → Export IFC 4.3, `IFCOUT`, or `--export-ifc file.ifc`. Signs are `IfcSign` with coloured artwork, property sets and quantities; posts are `IfcMember` (POST) and foundations `IfcFooting`, as swept solids, aggregated with their sign in an `IfcElementAssembly`; elements placed on site stand at their site position and rotation inside an `IfcRoad`; alignments are `IfcAlignment` with horizontal segments and a polyline axis. Checked with IfcOpenShell: schema and EXPRESS rules 0 issues, placements read back within 0.001 mm, solid volumes within 0.002 %. **Not yet opened in a BIM viewer, and not run through the buildingSMART validation service.** No vertical profile in the IFC alignment, no linear placement, no georeferencing; elements not placed on site stay upright as on the sheet |

The roadmap is in `../docs/plan/2026-11-13-afrobim-platform.md`.
