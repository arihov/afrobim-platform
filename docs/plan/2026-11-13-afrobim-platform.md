# AfroBIM platform: architecture and roadmap

Date: 2026-11-13. Owner decision recorded the same day: rebuild in Rust, in this repository,
under the name AfroBIM, for Windows and macOS, as a BIM application in which traffic signs are
one component. The UK Traffic Signs Manual is the core standard; other standards follow as packs.

## 1. What AfroBIM is

A programmable BIM platform for roads and infrastructure. A model is a set of typed elements
with properties; disciplines are modules that plug into a shared kernel. The first module is
traffic signs, carried over from SignCore.

Why Rust:

1. One codebase for Windows, macOS and Linux. The C# desktop app is WPF and runs on Windows only.
2. The same core can be exposed as a Python package and compiled to WebAssembly for a browser
   version, without a second implementation.
3. A single native binary with no runtime to install.

What stays in .NET: the Civil 3D and Revit add-ins, because Autodesk's APIs require it. They
become thin clients of the Rust core (command line or C ABI).

## 2. Architecture

```
afrobim-app  ──►  afrobim-signs  ──►  afrobim-draw
     │                                    ▲
     └──────────►  afrobim-kernel  ───────┘
```

| Crate | Responsibility |
|---|---|
| `afrobim-draw` | Neutral 2D primitives (fill, band, arc, glyph outlines, symbol placeholder), named colours |
| `afrobim-kernel` | Document, elements, layers, property sets, `.afrobim` file; `Module` trait; view transform, pick, window / crossing, grips, command state machine, snap, ortho, undo. No GUI types |
| `afrobim-signs` | Rule pack, font metrics, layout, glyph outlines for traffic signs |
| `afrobim-app` | Desktop application (egui + glow) |

Rules:

1. The kernel never knows a discipline. A module owns one element category and is the only code
   that reads that category's `data`.
2. Geometry is derived, never drawn freehand: `Module::generate(element)` returns a drawing in
   millimetres, checks (notes) and read-only derived values.
3. Standards are data. A rule pack carries the values with their clause citations; code carries
   the method. A new jurisdiction is a new pack.
4. Interaction logic is toolkit-free and unit-tested; a front end forwards input and draws state.
5. Every engine port is verified against the C# engine with golden fixtures before the C# path
   is retired.

## 3. Element model

| Field | Meaning |
|---|---|
| `id` | Stable identity within the document |
| `category` | The owning module, e.g. `sign` |
| `name` | Mark the element is scheduled under, e.g. `ADS-001` |
| `origin`, `width`, `height` | Placement and overall size in model millimetres |
| `layer` | Display and selection grouping |
| `data` | Module-owned definition (for a sign: type, speed, destinations...) |
| `properties` | Free-form property set: material, reference, cost code |

This maps onto IFC when exchange is added: element → `IfcProduct` subtype (a sign is
`IfcSign` in IFC 4.3), `properties` → property sets, `name` → `Name`/`Tag`, placement →
`IfcLocalPlacement`. The mapping is a design intention, not yet implemented or tested.

## 4. Roadmap

### Phase 0: foundation (this change)

Workspace, kernel, desktop shell, `.afrobim` file, UK stack-type direction sign ported and
golden-tested against the C# engine.

### Phase 1: sign engine parity

Port in this order, each with golden fixtures from the C# engine:

1. Direction, confirmatory and place-identification signs (shared destination column).
2. Map-type signs: crossroads, T-junction, roundabout.
3. Warning and regulatory signs (TSRGD diagrams), supplementary plates, sign-with-plate assembly.
4. Information signs, lane pre-selection, diversion.
5. XML folder packs, then the Uganda TSM 2004 pack.
6. DXF R12 export, quantities (BoQ), fabrication sheets (PDF), schedule import and export.
7. Standard-sign library browser and the catalog insert dialog.

Exit: every C# layout test has a Rust equivalent and passes; the C# desktop app is retired.

### Phase 2: BIM platform

1. Element types beyond signs: posts and foundations, road markings, alignments (station and
   offset placement for signs).
2. Schedules and quantities as live views of the model.
3. IFC 4.3 export, then import. Verify the available Rust IFC libraries first; if none is
   adequate, write STEP export for the subset AfroBIM produces.
4. 3D view (wgpu) of the same elements.
5. Sheets: title block, viewports, PDF.
6. Base drawings: DXF underlay, georeferencing.

### Phase 3: programmable

1. `afrobim` command-line tool (batch layout, export, validation) replacing the C# CLI.
2. Python package (`pip install afrobim`) over the kernel and modules.
3. MCP server so an AI assistant can query and edit a model.
4. Scripting inside the app; a documented module API for third parties.

### Phase 4: product

1. Installers for Windows (MSI) and macOS (signed, notarised DMG); auto-update.
2. Licensing: per-seat keys with offline activation; trial mode.
3. Web version from the WebAssembly build for demos and light use.
4. User guide, training material, sample projects.

## 5. Risks to close before selling

1. **Transport fonts** in `fonts/` are third-party. A paid product must either hold a licence
   that allows redistribution or read the user's own copies. Unresolved.
2. **Standards content.** UK TSM / TSRGD material is Crown copyright; the Uganda TSM is a
   Ministry publication. Confirm the terms for reproducing sign faces in a commercial tool.
   Not legal advice.
3. **Third-party manuals** in `Manuals/` (including TrafxPLAN documentation) must never ship.
4. **Platform testing.** The desktop shell has been checked by headless screenshot on Linux
   only. Windows and macOS runs, high-DPI displays and trackpad gestures are untested.
5. **IFC.** Rust IFC tooling is immature; treat IFC as its own phase with its own verification.
6. **Parity.** Until Phase 1 exits, the C# app is the only tool that can produce a full sign set.
