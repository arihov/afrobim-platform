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
| Insert a sign | Tool Sets or `STACK`; the sign hangs from the cursor; click to place |
| Edit | Select and use Properties; the model updates as you type |
| Zoom / pan | Wheel zooms about the cursor; middle button pans; `Z` extents; `ZW` window |
| Undo / redo | Ctrl+Z / Ctrl+Y |
| Drafting aids | F7 grid, F9 snap (50 mm), F8 ortho |

Type over the drawing to use the command line: a name or alias runs a command, `?` lists them,
an empty Enter repeats the last one. While a command waits for a point, type `x,y` (absolute),
`@dx,dy` (relative) or `@distance<angle`. Millimetres.

## What is ported, and what is not

| Area | State |
|---|---|
| Kernel, interaction, undo, file format | Done, unit-tested |
| Desktop shell | Done; checked by headless screenshot on Linux. Not yet run on Windows or macOS |
| UK stack-type direction signs | Ported from the C# engine, golden-tested against it |
| Map-type, direction, warning, regulatory, information, supplementary signs | Not ported. Use the C# app |
| Uganda rule pack (XML) | Not ported |
| DXF export (R12, mm, one layer per sign colour) | Ported; File → Export DXF or `DXFOUT`. Matches the C# writer byte for byte on the reference files |
| Fabrication sheets, quantities, schedules | Not ported. Use the C# CLI |
| Python API, MCP server, IFC exchange, 3D | Not started |

The roadmap is in `../docs/plan/2026-11-13-afrobim-platform.md`.
