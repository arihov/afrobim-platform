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
| Alignments | New (not a port): tangents, circular curves and clothoid spirals; LandXML import (File → Import LandXML, `LANDXML`); place a sign by station, offset and side (`STATION`). The placement rules are ported from C# and match it on 4,256 cases. Not tested against a file exported by Civil 3D. No vertical profile, superelevation or station equations |
| Posts and foundations | New (not a port): elements with dimensions the engineer enters, solids, and steel, concrete and excavation quantities by geometry (`SUPPORTS`). **No structural sizing, no wind loading, no standard section tables.** New supports carry placeholder dimensions and say so |
| 3D site view | Once a sign is placed on an alignment, the 3D view shows the site: alignments on the ground, signs at their stations facing their traffic, on their posts and foundations |
| Schedule CSV / XML import and export | Not ported in the app (the MCP server reads schedule CSVs). Use the C# CLI |
| 3D view | Done: View → 3D, the `3D` command or the status-bar switch. Orbit, pan, zoom, fit. View only; editing stays in 2D |
| Python package | Done: `afrobim/python` (PyO3, built with maturin); see its README |
| MCP server | Done: `afrobim-mcp` (11 tools: catalogue search, sign layout and checks, schedule validation, DXF export, and building `.afrobim` documents); see `crates/afrobim-mcp/README.md`. The five tools shared with the C# server returned identical results on 126 requests |
| IFC export | Done: File → Export IFC 4.3, `IFCOUT`, or `--export-ifc file.ifc` from the command line. Each sign is an `IfcSign` with coloured artwork, property sets and quantities. Checked with IfcOpenShell (schema and EXPRESS rules, 0 issues); not yet opened in a BIM viewer. Signs stand upright arranged as on the sheet, not at site positions. No import |

The roadmap is in `../docs/plan/2026-11-13-afrobim-platform.md`.
