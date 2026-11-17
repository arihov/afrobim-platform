# AfroBIM user manual

This manual describes the AfroBIM desktop application as the source stood on 16 November 2026 (workspace version 0.1.0, `afrobim/Cargo.toml`). Where a crate README and the code disagree, the manual follows the code, and the disagreement is listed in section 15, "Notes for the maintainer".

The reader is a practising highway engineer. The manual does not explain CAD or road engineering basics. It explains what each command asks for, what it produces, the edition of each standard it uses, and the limits the crate authors wrote down.

## Contents

1. What AfroBIM is and is not
2. Installing and starting
3. The interface
4. Coordinate systems
5. A first hour with the civil tools
6. Civil objects
7. First hour walkthrough: signs and roads
8. The ribbon tabs, in ribbon order
9. Command reference (all 205 commands)
10. Standards and editions
11. Known slips in the source documents
12. What has not been verified
13. Planned, not built
14. Troubleshooting
15. Notes for the maintainer

## 1. What AfroBIM is and is not

AfroBIM is a building-information-modelling application for roads and infrastructure, written in Rust. It runs as a desktop window (egui). The code builds for Windows, macOS and Linux. Only Linux builds and tests have been run recently (section 12).

It covers:

- traffic signs to the UK Traffic Signs Manual and to the Uganda Traffic Signs Manual (2004), with posts, foundations and wind design;
- horizontal and vertical alignments, from LandXML or designed from points of intersection (PIs);
- civil design objects: COGO points, ground surfaces, alignments with profiles, assemblies and corridors, sample lines and section views, earthworks and mass haul, parcels and land acquisition, with LandXML and IFC exchange (chapter 6);
- road markings, side drains, pipe culverts, sign gantries, street lighting, safety barrier runs, pavements and corridors, each with its checks;
- a model-wide check report, sign visibility and lighting photometry from luminaire files;
- clash detection, issues and BCF 2.1 exchange, and IFC reference models;
- a construction programme (critical path, Gantt chart, baseline, 4D playback, MS Project XML and Primavera XER exchange);
- a bill of quantities, rate library, estimate, cash flow and interim payment certificate, with no rates built in;
- site progress records, earned value and a look-ahead;
- drawing sheets (PDF and DXF), title blocks, and a calculation report pack;
- exports: DXF, IFC 4.3, quantity CSV files and sign fabrication sheets.

**What AfroBIM is not.**

- It is not a certified design tool. Its checks are calculations for an engineer to check. A passed check does not show that a sign, post, gantry, drain, culvert, pavement or barrier is adequate.
- It holds no design value that its sources do not state. Where a value is not in a held source, it is an input that the engineer enters. Nothing is assumed in its place. Several inputs start empty, and a check that needs an empty input reports "input needed" (section 14).
- It does not structurally design gantries or lighting columns. That is excluded by decision (round 4 plan).
- It does not import every format. XML schedules are not read; IFC is read only as reference models; BCF 3.0 is not supported; DWG is opened only as an underlay, through ODA File Converter (see 5.2).
- It has no cost rates, exchange rates, percentages or standard bill structure. The user enters them.
- It does not reproject coordinates or shift datums between coordinate systems. Arc 1960 and WGS 84 are different datums, and no shift is applied between them (chapter 4).
- Round 5 did not build pipe networks, grading objects, intersections and roundabout wizards, point clouds, data shortcuts or reprojection between systems (round 5 plan).
- Two further interfaces exist outside the desktop app: a Python package (`afrobim/python`) and an MCP server (`afrobim-mcp`). Their READMEs are in `afrobim/python` and `afrobim/crates/afrobim-mcp/README.md`. This manual does not cover them.

### The eight BIM workflow stages

The roadmap (`docs/plan/2026-11-14-afrobim-bim-workflow-roadmap.md`) maps the BIM workflow to eight stages. The table shows which ribbon tabs serve each stage in the code as built.

| # | Stage | Tabs that serve it | What it does |
|---|---|---|---|
| 1 | Concept design | Design, Roads (Earthworks), Insert, Output | Alignments by PIs or LandXML, profile editor, alignment checks against a limits file you load, alignment options compared on figures from the model |
| 2 | BIM modelling | Signs, Roads, Structures, Insert, Annotate, Edit, Modify, Home | Signs, supports, gantries, markings, drains, culverts, lighting, barriers, pavements, corridors, ground surfaces, annotation, IFC export |
| 3 | Coordination | Coordination | Clash tests and results, issues, BCF 2.1 export and import, IFC reference models |
| 4 | Analysis | Design, Roads, Structures, Signs, Manage | Model-wide check (CHECKALL), sign support design, sign visibility, drainage capacity and culvert check, gantry requirements, lighting requirements and photometry, barrier requirements, pavement design by three methods, rational flow |
| 5 | 4D (time) | 4D | Programme, calendar, critical path, baseline, progress, links from elements to activities, 4D date and playback, MS Project XML and XER exchange |
| 6 | 5D (cost) | 5D | Bill of quantities with mapping rules, rate library, currency, estimate, cash flow, interim payment certificate, cost CSV reports |
| 7 | Documentation | Output, Manage, Insert | Sheet sets, title blocks, project information, PDF and DXF sheets, calculation report pack, DXF, IFC and quantity exports, sign schedule import and export |
| 8 | Construction | Construction | Site progress records, roll-up to the programme, earned value, look-ahead and delays, colouring by progress, interim certificate quantities from site records |

Two points remain open against the roadmap. Site records hold photographs and files as paths only, and the non-conformance and inspection records of roadmap item S2 are not in the code.


## 2. Installing and starting

Installation is in `afrobim/docs/installing.md`. That file covers the Windows, macOS and Linux packages, the workflow that builds them, and the building of the app from source. Read its "What has and has not been tested" section first: the workflow built the macOS `.dmg` and the Windows `.msi` on 2026-11-15, the `.dmg` has been run on one Apple silicon Mac, and the `.msi` has not yet been run on a Windows desktop. Both are unsigned and carry a placeholder icon.

To build and run from a source checkout (Rust 1.85 or later):

```
cd afrobim
cargo run --release -p afrobim-app            # the desktop app
cargo run --release -p afrobim-app -- --demo  # with sample signs and sample data
cargo run --release -p afrobim-app -- --demo civil  # with the civil demonstration data (chapter 5)
cargo test                                    # kernel, sign engine (golden tests), app
```

The `--demo` and `--demo civil` data are made up. Every demo number is commented in `crates/afrobim-app/src/demo_glue.rs` as an arbitrary value. Do not use demo values for a design.

### Rule pack and fonts

The traffic-signs module needs two things at run time, which are not compiled in:

| File | Where it is looked for | Override environment variable |
|---|---|---|
| `packs/UK-TSM-2019.json` (UK rule pack) | the environment variable, next to the executable, on macOS in the bundle's `Contents/Resources`, then the folders above the executable and above the working directory | `AFROBIM_PACK` (the JSON file) |
| `packs/UG-TSM-2004/pack.xml` (Uganda rule pack, a folder of 17 XML files) | the same order | `AFROBIM_PACK_UG` (the folder, or its `pack.xml`) |
| `fonts/tranh___.TTF`, `fonts/tranm___.TTF` (Transport fonts) | the same order, folder by folder | `AFROBIM_FONTS` (the folder) |

The Transport fonts are third-party files. The repository includes them in `fonts/`, but the project README says not to redistribute them without a licence that allows it. Check that licence before distributing a build.

### Command-line options

These come from `crates/afrobim-app/src/main.rs`. An argument that is not an option is the document to open. Options given after `--` on a `cargo run` line are passed to the app.

| Option | Effect |
|---|---|
| `file.afrobim` | Opens this document at start-up. The Windows file association passes the file this way. |
| `--demo` | Starts with the demo model (sample signs, alignment and data). |
| `--design` | With `--demo`, sizes the demo supports for wind (MoWT open terrain) and shows the first calculation. |
| `--tab NAME` | Starts on this ribbon tab, for example `Roads`. Case does not matter. |
| `--preview COMMAND` | Shows the hover card of the first ribbon button that runs this command. |
| `--3d` | Starts in the 3D site view. |
| `--standard ID` | Starts with this design standard: `UK-TSM-2019` (the default) or `UG-TSM-2004`. |
| `--landxml FILE` | Imports the alignments of this LandXML file at start-up. |
| `--focus MARK` | Selects the element with this mark, and frames the 3D view on it. |
| `--run COMMAND` | Runs this command once at start-up, on the element named by `--focus` when there is one. Several commands may be separated by commas. |
| `--date YYYY-MM-DD` | Shows the 3D view at this 4D date, coloured by the programme. |
| `--subtab N` | The sub-tab (from 0) of the Bill and Estimate windows. |
| `--zoom-clash N` | Zooms the 3D view to the Nth clash (from 0) after the demo clash run. |
| `--aid NAME` | Screenshot aid: fills a dialog with arbitrary test values (see `aids.rs`). |
| `--limits FILE` | ALIGNCHECK loads this limits table (CSV or JSON) when it opens. |
| `--limits-row LABEL` | With `--limits`, runs the checks against this row of the table. |
| `--new-alignment "Name,start"` | Starts ALIGNDESIGN with this name and start station (metres). |
| `--pis "x,y;x,y;..."` | With `--new-alignment`, the PIs (metres). |
| `--pis-finish` | With `--new-alignment` and `--pis`, makes the alignment. |
| `--ifcref FILE[,dx,dy,dz,rot]` | Imports this IFC file as a reference model. Offsets are in millimetres, rotation in degrees. |
| `--photometry FILE` | Opens the Photometry window with this IES or LDT file loaded. With `--photometry-run`, also runs it on arbitrary demo settings. |
| `--underlay FILE[,dx,dy,rot]` | Attaches this DXF as an underlay (millimetres, degrees). |
| `--signschedule FILE` | Reads this sign schedule CSV at start-up and opens its mapping and plan windows. |
| `--survey FILE` | Makes a ground surface from this file of survey points at start-up. Needs `--survey-order`. |
| `--survey-order ORDER` | The columns of the point file: `PNEZD`, `PENZD`, `NEZD` or `ENZD` (P point, N northing, E easting, Z level, D description). Not guessed. |
| `--survey-crs NAME` | The coordinate system of the point file, for example `"WGS 84 / UTM zone 36S"` or `EPSG:32736`. Optional. |
| `--survey-max-side M` | The longest side a triangle may have, in metres. Optional: with none, the surface fills the convex hull of the points. |
| `--survey-exclude CODES` | Descriptions whose points are left out, separated by commas; a trailing `*` matches any ending (`CULV-*`). |
| `--survey-name NAME` | The name of the surface; the file's name when not given. |
| `--save-as FILE` | Saves the document to this file once the start-up imports are done, then exits. |
| `--layers-on NAMES` | Switches these layers on at start-up, comma separated. |
| `--zoom-to x,y,width` | Frames the 2D view on this centre and width (millimetres). |
| `--xer-in FILE`, `--xer-out FILE` | Imports or exports a Primavera XER file at start-up, to see the reports without a dialog. |
| `--frames-dir FOLDER` | Writes the 4D playback as PNG frames of the 3D view into the folder, then exits. |
| `--frames-step DAYS` | Days between frames (7 when not given). |
| `--frames-from DATE`, `--frames-to DATE` | First and last frame date. |
| `--vis-view drawings` or `overlay` | After `--run VISIBILITYRUN`, shows only the drawings, or only the 3D overlay of the limiting sight line. |
| `--capture-viewpoint` | Takes a viewpoint and snapshot of the first issue at start-up. |
| `--export-ifc FILE` | Writes the starting model as IFC 4.3, then exits. |
| `--export-sheets FOLDER` | Writes quantities and fabrication sheets for the starting model, then exits. |
| `--export-set PATH` | Composes the drawing sheet set and writes it, then exits. A `.pdf` path gets one PDF, a `.dxf` path one DXF with every sheet, any other path a folder with `sheets.pdf` and one DXF per sheet. |
| `--sheet-set NAME`, `--sheet N`, `--sheet-form N` | The sheet set to export or preview; the sheet to show first; the add form of template N (0 plan, 1 profile, 2 sections, 3 schedule, 4 detail, 5 notes). |
| `--export-pack FILE` | Writes the calculation report pack as PDF, text and CSV (the other two variants are written beside the named file), then exits. |
| `--size WxH` | The window size at start-up in points (default 1440 by 860, at least 1000 wide), to look at a given width. |
| `--camera DIST,YAW,PITCH[,STATION]` | The 3D camera after the first fit: distance in millimetres, yaw and pitch in degrees, and optionally the station of the first alignment's centre line to look at. For close-up screenshots. |
| `--workspace NAME` | Starts in this workspace, for example `"Road Design"`. The name is matched loosely (section 3.1). |
| `--theme LOOK` | Interface look at start-up: `dark-glass`, `light-glass`, `solid-dark` or `solid-light` (section 3.10). |
| `--scale FACTOR` | Interface scale at start-up, 1.0 being normal. The Appearance dialog offers 0.85 to 1.75. |
| `--demo civil` | Starts with the civil demonstration data (chapter 5). |
| `--click ACTIONS` | Screenshot aid: pointer actions at start-up, `x,y` to click, `x,yh` to hover and `x,yr` to right-click, separated by semicolons. |
| `--landxml-dialog FILE` | Opens the LandXML import dialog on this file at start-up, instead of importing at once. |
| `--survey-dialog FILE` | Opens the survey points dialog on this file at start-up. |
| `--screenshot FILE.png` | Renders a few frames, saves the window and exits. Used to check the interface without a desktop. |


## 3. The interface

The window has three bands and two columns. From top to bottom: the menu bar, then the ribbon, with the workspace switcher at its left end. Below the ribbon are the left panel (the Tool Sets and the toolspace), the drawing area in the middle, and the right panel (Properties and Layers). The command line and the status bar sit at the bottom.

Every route to a command runs the same entry in one command table, `crates/afrobim-app/src/commands.rs`, which holds 205 commands. The menus, the ribbon, the Tool Sets, the command palette and the command line therefore always do the same thing. Section 9 lists all 205.

### 3.1 Workspaces

A workspace decides which ribbon tabs, which panels of a shared tab and which Tool Sets show. It does not stop any command. A command that belongs to another workspace still runs from the command palette, the command line and the menus.

Choose a workspace with the switcher at the left end of the ribbon, or with Window → Workspace. The switcher shows the current workspace's name and icon. Your choice is kept in the preferences, and AfroBIM opens in it next time. `--workspace "Road Design"` starts in a named workspace. The name can be typed loosely: `road design`, `roaddesign` and `signs` all resolve.

Home and Annotate show first in every workspace except All. Edit, Manage, View and Help come last in every workspace except All. The **All** workspace shows every tab and every panel, as the window did before workspaces were added.

Table 3.1 gives what each workspace shows, taken from `workspace.rs`.

| Workspace | Tabs and panels it adds to Home, Annotate, Edit, Manage, View and Help | Tool Sets in the left panel | Panels shown when opened |
|---|---|---|---|
| All | Every tab and every panel | Traffic signs, Modify and Zoom | Left and right |
| Civil | Civil (all panels), Insert (all panels) | None: the toolspace takes the whole left panel | Left and right |
| Survey and Terrain | Insert (all panels), Roads (Earthworks panel only) | Modify and Zoom | Left and right |
| Road Design | Roads (Alignment, Pavement and Earthworks panels), Design (all panels), Insert (all panels) | Modify and Zoom | Left and right |
| Signs and Markings | Signs (all panels), Roads (Alignment and Markings panels), Insert (all panels), Modify (all panels) | Traffic signs, Modify and Zoom | Left and right |
| Drainage | Roads (Alignment and Drainage panels) | Modify and Zoom | Left and right |
| Lighting and Restraint | Roads (Alignment, Lighting and Restraint panels) | Modify and Zoom | Left and right |
| Structures and Supports | Structures (all panels), Modify (all panels) | Modify and Zoom | Left and right |
| Coordination | Coordination (all panels) | Modify and Zoom | Right only |
| Construction | 4D, 5D and Construction (all panels) | Modify and Zoom | Right only |
| Documentation | Output (all panels), Insert (all panels) | Modify and Zoom | Neither |

The Home tab shows all its panels in every workspace. "Modify and Zoom" means the Modify and Zoom buttons of the left panel. They show when the left panel is open.

### 3.2 Ribbon

The ribbon has 18 tabs. In the All workspace they appear in ribbon order: Home, Edit, Insert, Annotate, Modify, Signs, Roads, Civil, Design, Structures, Coordination, 4D, 5D, Construction, Output, Manage, View and Help. Other workspaces show their tabs in the order of section 3.1, with Home and Annotate first.

Each tab is a row of panels. Each panel holds buttons. A large button has an icon and a label. A small button is stacked three high. Every button runs a command from the command table.

Hover over a button to see its preview card. The card gives what the command does, its command name and aliases, its shortcut if it has one, and the line "Click, or type the command and press Enter. Esc cancels." A button for a tool that is not built yet is shown disabled, with the card line "Planned: not built yet." On the current code there is one such button (section 13).

The Signs tab is built from the sign templates of the active design standard, so its buttons change between UK and Uganda. Section 8 covers each tab. The Civil tab is covered in section 6.

### 3.3 Menu bar

The menu bar has ten menus: File, Edit, View, Insert, Civil, Modify, Analyse, Tools, Window and Help. The menus repeat commands from the ribbon. Each item shows its shortcut at its right. An item with no shortcut shows its command name instead, so a menu also teaches the name to type.

- **File.** New, Open, Open recent, Save, Save as. Import: LandXML alignments, LandXML ground surface, survey points (CSV), sign schedule from CSV, DXF underlay, IFC reference model, issues from BCF, programme from MS Project XML, programme from Primavera XER. Export: DXF, IFC 4.3, alignment as LandXML, quantities (BoQ), fabrication sheets, sheet set as PDF and as DXF, calculation report pack, sign schedule (CSV), blank sign schedule template, issues as BCF, clash results (CSV), programme as MS Project XML and as Primavera XER, programme reports (CSV), cost reports (CSV), earned value tables (CSV). Then Project information, Project settings, Coordinate system and Quit.
- **Edit.** Undo, Redo, Copy, Paste, Erase, Select all.
- **View.** Zoom extents, Zoom window, Zoom to selection, Zoom previous, Zoom in, Zoom out. 2D model space or 3D view. Dark or light model space. Grid, Snap, Ortho and Outlined.
- **Insert.** Standard signs. The traffic sign templates of the active standard. Road markings (lines along an alignment, stop and give way, arrows, hatching and chevrons, yellow lines, road studs, worded markings, Uganda lane and edge lines). Road elements (side drain, pipe culvert, portal gantry, cantilever gantry). Street lighting, lighting column, safety barrier, pavement, corridor, survey points (CSV), ground surface (LandXML), DXF underlay and IFC reference model.
- **Civil.** Toolspace. Submenus for Points, Surfaces, Profiles, Corridors and sections, and Parcels. Earthworks and mass haul. Export civil data as LandXML. Demonstration data. Section 6 covers each command.
- **Modify.** Move, Copy, Erase and Arrange. Sign actions: place the sign on an alignment, add supports, design supports, remove supports, hang the sign on a gantry, take it off a gantry. Design an alignment, edit the alignment in plan, profile editor, edit text.
- **Analyse.** Check the whole model, check the selected element, alignment design checks, pavement design, rational method flow, sign visibility, photometry and corridor cross-sections, with a Coordination submenu.
- **Tools.** Layer manager, Units, Object snap settings, Text styles, Dimension styles. Submenus for Sheets (sheet sets, sheet preview, title block), Annotate, Measure, Programme (4D), Cost (5D) and Construction. Design standard, with UK and Uganda.
- **Window.** Workspace (the list of workspaces). Panels: Tool sets and model tree, Properties and layers. Look: Dark glass, Light glass, Solid. Appearance. Dark / light. Command palette.
- **Help.** Search commands (the palette), Command list, About AfroBIM.

### 3.4 Tool Sets and the toolspace (left panel)

The left panel has two parts, one above the other.

**Tool Sets** appear when the workspace shows them.

- **Traffic signs**: one button for each sign template of the active standard, and Standard signs (`LIBRARY`). The sign buttons are disabled until the rule pack has loaded; the status bar says why.
- **Modify**: Move (`M`), Copy (`CO`), Erase (`E` or Delete) and Arrange.
- **Zoom**: Extents (`Z`), Window (`ZW`) and Selection (`ZS`).

**Toolspace** is below the Tool Sets, or fills the panel in the Civil workspace. It has two tabs.

- **Prospector** is the model tree. It lists the civil objects first (points, point groups, surfaces, alignments with their profiles, profile views, sample line groups, corridors, assemblies and parcels), then the other element categories. Each row has an icon and a show or hide eye. A civil object's status (Draft, For review, Approved or Superseded) shows in its tooltip. Right-click a row for its menu, which holds zoom, rename, delete and the commands of that object. The eye shows or hides the object. Double-click a row to zoom to its object.
- **Settings** shows the project coordinate system, the units and the design standard, each with a button that opens its dialog.

`TOOLSPACE` (alias `PROSPECTOR`, View tab) and Window → Panels → Tool sets and model tree show or hide the left panel.

### 3.5 Properties panel (right, top)

The Properties panel shows the selected element. When several elements are selected it shows the last one picked, and says so. It has these parts:

- the element's name (its mark), its category and its layer, which is a list of the document's layers;
- its position and size;
- **Definition**: the fields the module owns. These are the sign fields, the dimensions of a post, drain or gantry, and the other inputs of each module. A placeholder dimension is marked as such.
- **Derived**: values the module calculates. These cannot be edited.
- **Checks**: notes from the module, coloured by severity.
- **Property set**: free key and value pairs, such as Material. The check inputs entered in the CHECK dialogs are stored as properties too, under keys such as `check.drain`, and are hidden here.

Right-click the Properties panel for Copy element, Paste, Erase, Select all, Undo, Redo, Layer manager, Units, Project settings and Coordinate system.

### 3.6 Layers panel (right, below Properties)

The LAYERS list shows each layer with a visibility tick and a Lock toggle. Locked layers cannot be selected. Hidden layers do not show on the canvas, and nothing on a hidden layer is picked by object snap. Create, rename, delete, colour and weight are in the layer manager (`LAYER`, section 8, Manage tab).

### 3.7 Command line (bottom)

The bottom panel shows the last three lines of the command history, then a prompt and an input box. The prompt reads `Command:` when nothing runs. While a command runs it shows what that command needs next, for example `Specify first extension line origin:`. The input box has the hint "? commands · Enter repeats · Esc cancels". The command line takes the keyboard when nothing else has focus, so typing over the drawing starts a command.

### 3.8 Command palette and command list

Press Ctrl+K, or Ctrl+Shift+P, to open the command palette. Type words to search. Every word must match part of a command's name, its aliases or its help text. The results are ranked: an exact name first, then an exact alias, then names and aliases that start with the word, then names and aliases that contain it, then help text. Up and Down move through the list, Enter runs the selected command, and Escape or a click outside closes the palette. The palette shows up to 80 matches, each with its icon, name, aliases and help. It lists every command, including those of other workspaces.

`?` on the command line, or Help → Command list, lists every command in the same table. Section 9 of this manual is the same list.

### 3.9 Status bar (bottom)

From left to right the status bar holds:

- the cursor position: easting and northing in metres of the project coordinate system, or the model position in millimetres (in the display units you chose with `UNITS`) when the document has no coordinate system;
- the toggles **GRID** (F7), **SNAP** (F9, snaps picked points to a 50 mm grid), **ORTHO** (F8, holds moves to the X or Y axis) and **OSNAP** (F3, object snap for the annotation commands, `MOVE`, `COPY`, grips and placing a sign);
- the **annotation scale** control, shown as `1:n`. New text, dimensions and leaders are sized for this scale. Existing annotation keeps its own scale;
- **2D** and **3D**, to switch between the model space and the 3D site view;
- **Real** or **Outlined** display;
- the latest status message;
- on the right: the selection ("No selection", the name and size of one element, or the number of elements selected), the module status (the standard in use, or why the sign tools are unavailable), the scale as "1 px = x mm" (the zoom read-out), the project coordinate system (or "No coordinate system", which is a button that sets one), and the latitude and longitude of the cursor where the system allows.

The segments never overlap. The message takes the width the right-hand read-outs leave. When the window is narrow, the read-outs are dropped in a fixed order, least useful first: the zoom read-out, the module status, the latitude and longitude, the selection, then the coordinate system. The message is never dropped; it is shortened. A path in the message is cut in the middle and keeps its file name, for example `/Users/anna/…/Untitled.afrobim`. Hold the pointer over a shortened message to read it in full.

### 3.10 Appearance

**Window → Appearance**, or `APPEARANCE` (aliases `LOOK`, `SCALE`, `INTERFACE`), opens the Appearance dialog. A change applies at once and is kept in the preferences file. The same choice of look is also under Window → Look.

- **Look.** Dark glass (the default), Light glass, or Solid. A solid look has a palette of Dark or Light. "Glass" means translucent tinted panels and floating windows, with rounded corners and soft shadows, so the model shows faintly through a floating window. **The glass look has no background blur.** The renderer used by AfroBIM offers no live blur, and none is promised. Solid turns off the translucent fills.
- **Scale.** The interface scale is one of 85%, 100%, 115%, 130%, 150% and 175%. The default is 100%.
- **Dark / light** (`THEME`) switches between the light and dark themes. **Canvas** (`CANVAS`) switches the model space between dark and light.

`--theme` takes `dark-glass`, `light-glass`, `solid-dark` or `solid-light`, and `--scale` takes a factor such as `1.15`. Both set these at start-up (section 2).

### 3.11 Right-click menus

**Drawing canvas.** When elements are selected the menu first shows "N selected", then Move, Copy, Erase and Copy to clipboard, then the element actions (place on an alignment, add supports, design supports, check). Then Paste, Select all, Undo and Redo. Then Zoom extents, Zoom window, Zoom to selection, Zoom previous and 2D / 3D. Last, Command palette.

**Properties panel.** Listed in section 3.5.

**Tool Sets and toolspace rows.** Each row of the Prospector has its own menu, listed in section 3.4.

### 3.12 Zoom commands

Zoom does not stop at any scale. Section 4.6 gives the precision limit that remains.

| Command | Aliases | What it does |
|---|---|---|
| `ZOOM` | `Z`, `ZE` | Zooms to show every element (zoom extents). |
| `ZOOMWINDOW` | `ZW` | Zooms to a window picked with two corners. |
| `ZOOMSELECTION` | `ZS`, `ZOOMOBJECT` | Zooms to the selected objects. Says so on the command line when nothing is selected. |
| `ZOOMPREVIOUS` | `ZP` | Returns to the previous view. The last 50 views are kept. |
| `ZOOMIN` | `ZI` | Zooms in about the centre of the view. |
| `ZOOMOUT` | `ZO` | Zooms out about the centre of the view. |

With the pointer, the mouse wheel zooms about the cursor, in 2D and in 3D. The middle button pans in 2D. A double-click with the middle button zooms to the extents of what is visible. In the 3D view the wheel zooms, and a double-click fits the view. A row in the Prospector zooms to its object on double-click, or from its right-click menu.

### 3.13 2D canvas and 3D site view

The canvas is the model space. It is in millimetres with Y up.

- **Select**: click an element. Click more to add to the selection; Shift and click removes one; Esc clears the selection.
- **Window and crossing**: drag on empty space. Left to right takes elements wholly inside; right to left takes elements the box touches.
- **Move**: drag a selected element or one of its grips, or type `M`. A drag honours object snap (OSNAP, F3) as `MOVE` does: pressed near a snap point, the grab point is that point; dragged near one, the element lands with the grab point on it. The element being dragged does not snap to itself, and the snap marker is drawn.
- **Open and import by dropping**: drop an `.afrobim`, `.xml` (LandXML), `.dxf` (underlay) or `.ifc` (reference model) file on the window. A question says what will happen, and names unsaved changes if opening would lose them; nothing happens until you confirm. On macOS, double-clicking an `.afrobim` file in Finder starts the app without the file, because the framework does not pass on the open-document event (see `afrobim/docs/installing.md`).
- **Copy**: type `CO`, give a base point, then a point for each copy, then Enter. Ctrl+C and Ctrl+V also work.
- **Erase**: Delete, or `E`.
- **Undo and redo**: Ctrl+Z and Ctrl+Y. Every edit goes through undo, including the 4D, 5D and check inputs.
- **Drafting aids**: F7 grid, F9 snap to a 50 mm grid for picked points, F8 ortho.

The 3D site view is switched on with `3D`, the 2D / 3D switch in the status bar, or View → 2D / 3D. Drag to orbit; Shift-drag or middle-drag pans; the wheel zooms; double-click fits the view. The 3D view is for looking only. Editing stays in 2D. It shows the alignments at their profile levels, signs at their stations on their posts and foundations, and the road modules at their site positions. The ground is painted first as a backdrop. A triangle with a corner behind the camera is cut at the near plane and the part in front is drawn, so large pieces such as the ground and the pavement stay in view when the camera is close. Clash markers, IFC reference models, underlays draped at a level, the 4D states and the progress colour, and the sign visibility overlay are drawn here too. A civil surface with more than 40 000 triangles is shown in 3D as a thinned copy; the 2D view and every calculation use the full surface.

### 3.14 How commands work

**Typing a command.** Type a name or an alias and press Enter. Case does not matter and spaces around the name are ignored. An unknown name gives the message `Unknown command "name". Type ? for the list.`

**Listing commands.** Type `?`, or use Help → Command list. The list also shows how to type points.

**Enter repeats.** Press Enter on an empty command line to repeat the last command.

**Esc cancels.** Esc cancels the command in progress. When no command runs, Esc clears the selection.

**Prompts and options.** A command asks for each thing it needs in the prompt line. Options are shown in brackets in the prompt; type the option's word or letter to choose it.

**Points.** A command that waits for a point takes:

- `x,y` for an absolute point (millimetres, or the display unit you chose);
- `@dx,dy` for a relative point from the last point;
- `@distance<angle` for a polar point, the angle in degrees.

Numbers typed for lengths and angles follow the display units. Stations can be typed in the display format (for example `1+250.500`), and angles in degrees, minutes and seconds (for example `45d30m15s`). The stored values stay in millimetres. The design tools (PI table, profile editor, plan editor labels), the sheet source forms, the `ALIGNCHECK` results table (values and limits that are lengths), the lengths of the `OPTIONS` comparison (areas, volumes, percentages and counts keep their own units) and the programme's location ranges and time-location station scale (shown and typed as `1+250`, with the station decimals) follow the display units too: see the Units row of section 8.15. Engine calculation text and CSV files stay as the engines write them.

**Undo.** Ctrl+Z undoes the last edit; Ctrl+Y redoes. The sign schedule import is one undo step.

**Esc in a field.** Esc in a Properties field belongs to that field unless a command is running.

**Shortcuts in the code.** Ctrl+N New, Ctrl+O Open, Ctrl+S Save, Ctrl+Shift+S Save as, Ctrl+Z Undo, Ctrl+Y Redo, Ctrl+A Select all, Ctrl+K command palette, F3 object snap, F7 grid, F8 ortho, F9 snap, Delete Erase. Ctrl+C and Ctrl+V copy and paste the clipboard.

## 4. Coordinate systems

AfroBIM handles coordinates the way Civil 3D handles a drawing. A document has one **project coordinate system**. Every object keeps the coordinates of its source. Nothing is reprojected, shifted or re-based when a file is imported, when the system is set, or when it is changed. A file in another system is reported, and the report says what to do about it.

### 4.1 What the project coordinate system is

The project coordinate system is a setting of the document. It says what the numbers in the model mean. It does not move anything. It is stored with the document under the project settings, with the key `project_crs`. A document with no key reads as having no coordinate system.

Set it in any of these places, which all open the same dialog:

- the command `CRS` (alias `COORDSYS`), on the Manage tab, Project panel, or in the File menu as Coordinate system;
- the **Settings** tab of the toolspace, with the Edit coordinate system button;
- a click on the system's name at the right of the status bar, which reads "No coordinate system" until one is set.

A new document has no coordinate system. The status bar then shows the model's own coordinates, and the IFC export says that none is written.

The dialog shows the system's kind, its area of use and where it came from (typed, read from a LandXML file, read from an IFC file, or read from a survey file). The horizontal and vertical datum are free text that you type. The application records them and never interprets them.

### 4.2 Objects stay where they are

- **Nothing is reprojected or moved.** Setting or changing the system changes only what the existing coordinates are taken to mean. The dialog says how many elements the document holds, and that none of them moves.
- **Millimetres in the model.** The model is in millimetres. An easting of 450 123.456 m is stored as 450 123 456 mm.
- **Civil objects.** A civil object keeps the coordinates it was given. Nothing is moved to suit the drawing.
- **Imports come in at the file's coordinates.** A LandXML file, a survey file or an IFC reference model comes in at its own numbers, and the check described in section 4.5 says whether those numbers agree with the project. The import never shifts them to fit.

Setting a coordinate system on a document with objects does not make the objects line up with a survey taken in another system. The application has no datum shift and no reprojection between systems. Converting a dataset is done in a survey tool, and then the converted file is imported.

### 4.3 The kinds of system

Each system has one of three kinds. The kind decides what the application can compute.

| Kind | What it means | Systems in this kind |
|---|---|---|
| **Projected, with maths** | The application can place a coordinate on the Earth: latitude, longitude, grid convergence and point scale factor. It can check an import against the system's area of use. | WGS 84 / UTM zones 1 to 60 North and South (EPSG:32601 to 32660 and 32701 to 32760). Arc 1960 / UTM zones 35S, 36S and 37S, and 35N, 36N and 37N (EPSG:21035 to 21037 and 21095 to 21097). |
| **Named only** | The system is recorded by name, code or pasted WKT. The application has no maths for it. Coordinates are shown as eastings and northings. There is no latitude, longitude, convergence or scale. An import is checked only as far as the checks in section 4.5 can go without a projection. | Any other EPSG system. Search by code or by name, or paste WKT. |
| **Local site grid** | The numbers have no geodetic meaning. Nothing can be placed on the Earth, and an import cannot be checked against it. | Any name the user types for a plant or site grid. |

The application searches the systems it knows by code or by part of a name. "UTM 36S" is read as WGS 84 / UTM zone 36S. The Arc 1960 zones are listed above; other Arc 1960 zones are named only.

The projection maths is a third-order Krüger series in both directions, after Karney (2011). The kernel README records that the parameters of each system were fetched from the EPSG registry (epsg.io), not typed from memory. Forward and inverse projection agree to under a millimetre over twelve East African systems in the tests. The area of use for zones other than 36N and 36S follows the same six-degree rule; it was not fetched for each zone.

### 4.4 Latitude, longitude and the datum

Arc 1960 and WGS 84 are different datums. **No datum shift is applied between them.** Latitude and longitude from the application are on the datum of the system they came from. The status bar says so: "Latitude and longitude are on the datum of the system. No datum shift to WGS 84 is applied." An Arc 1960 system gives Arc 1960 latitude and longitude, not WGS 84.

Hover over the latitude and longitude in the status bar to see the point scale factor and the grid convergence, where the system has maths. Grid convergence is the angle from grid north to true north, counted positive counterclockwise.

The application does not combine the grid scale factor with the elevation factor. The point scale factor is given; the reduction of a distance to the ellipsoid by ellipsoidal height is left out, because the cited formula was not fetched.

### 4.5 What each import check means

When a dataset comes in, the application checks its declared system and its extent against the project. The check does not change any coordinate. It returns plain sentences. The check runs when a dataset is imported: LandXML alignments and surfaces (in the import dialog, under "Against the project coordinate system"), point and survey files, and IFC reference models.

| Finding | When it appears | Warning? | What to do |
|---|---|---|---|
| Same system | The file and the project are the same system. | No | Nothing. |
| Different system | The file and the project are both named, and they differ. The file is still imported, at its own coordinates. | Yes | Check that the difference is intended. Nothing will line up with the rest of the project until the file is converted in a survey tool. |
| Undeclared | The file does not say which system it uses, or the project has none. | Yes | Check the coordinates against the survey before you rely on the position. The coordinates are taken as they are. |
| Outside the area of use | The extent of the file is outside the EPSG area where the system is defined. It may belong to another zone or system. | Yes | Check the zone. This is a prompt to look, not proof of an error. |
| Likely swapped | The extent falls inside the area only if easting and northing are swapped. | Yes | If the first column is the northing, say so and import. Otherwise check the file. |
| Likely millimetres | The values divided by 1000 fall inside the area. | Yes | Check the file's units. |
| Likely feet | The values multiplied by 0.3048 fall inside the area. | Yes | Check the file's units. |
| Cannot check | The system is named only, or a local grid, so its extent cannot be tested against the Earth. | No | The check gives no answer for the extent. |

The unit and swap checks run only where the system is projected with maths. The heuristics cannot tell a US survey foot from an international foot, which differ by about 2 parts in a million.

The LandXML dialog reads the file's units and converts lengths to metres. Its toggle "Use the file's coordinate system for the project" is on by default when the project has no system. Switching it off imports the file's objects and leaves the project without a system.

### 4.6 Precision and zoom

**There is no fixed zoom limit** in 2D or in 3D. The only guards are that a scale stays finite and between 1e-24 and 1e24 pixels per millimetre, and that every stored number stays finite. A zoom or pan that would break a guard is ignored. The status bar's zoom read-out reads in any range: "1 px = 2.5 mm", "1 px = 1.20 m", "1 px = 0.0031 mm" or "1 px = 4.2 um".

**The one rounding that remains** is the spacing of double precision at the coordinate, which is 2.2e-16 times the coordinate's size. At a northing of 10 000 km (1e10 mm) that spacing is 1.9e-6 mm. A point cannot be told from its neighbour that is closer than the spacing, whatever the zoom. The kernel README records the tests: zooming in about the cursor from 10 m across past 1e5 pixels per millimetre keeps the point under the cursor within 1e-3 mm; a 10 000 km extent fits on screen; and panning a million pixels and back changes a position by less than 1e-3 pixels.

Element meshes are drawn in single precision relative to each element's own corner. A vertex is good to about 1e-7 of the element's size, which is 0.5 mm on a 5 km alignment. This affects the picture only. Snaps, measures and the civil calculations use the exact numbers.

The civil engines keep coordinates exactly as given. The cogo crate was tested at a northing near 10 000 000 m with no loss at 0.0001 m. The terrain crate shifts the data to its south-west corner before triangulating and returns results in source coordinates.

### 4.7 IFC georeferencing

**Export.** With a project coordinate system, `IFCOUT` writes an `IfcProjectedCRS` and an `IfcMapConversion`. The projected system's name is its EPSG code, for example `EPSG:32736`. Its datums are the ones the project states, with `UTM` and the zone where the system is known, and the map unit is the metre. The map conversion has no rotation and a scale of 0.001, which is the factor from the file's millimetres to the map's metres.

Geometry is written near a local origin, so that viewers which keep positions in 32-bit numbers do not lose accuracy. The origin is the least easting and the least northing of everything with a site position and every alignment element, rounded down to a whole kilometre, at height 0. The map conversion carries that origin. Levels are not shifted and nothing is rotated.

With no coordinate system, the file is written with no `IfcProjectedCRS` and no `IfcMapConversion`, and the coordinates are written as they are in the model. The export dialog says so. A project system that is a local site grid is not written either. A model with nothing on the site gets no map conversion.

The IFC 4.3 attribute order was checked against the IFC4X3_ADD2 schema in IfcOpenShell. The online IFC documentation could not be fetched during the work, so the names were checked against the schema and the buildingSMART sample files instead.

**Import.** A reference model's map conversion is applied by default. The application checks the model against the project coordinate system with the findings in section 4.5.

### 4.8 Limits, stated plainly

- No datum shift between any two systems, and no reprojection of any object. Arc 1960 and WGS 84 are different datums.
- Named-only systems have no maths. Local site grids have no geodetic meaning.
- Only the systems listed in section 4.3 have projection maths. A system is added only after its definition is fetched.
- The area-of-use test is a prompt to look, not proof.
- Heights are never converted. The vertical datum is a label.
- Metres only. The linear unit is one value.
- The accuracy of the projection is sub-millimetre inside a UTM zone at the latitudes of East Africa, as tested. It is not the nanometre accuracy of the higher-order series.
- Points more than 80 degrees from the central meridian return no answer.

## 5. A first hour with the civil tools

This walkthrough uses the civil demonstration data. `CIVILDEMO` builds it, and `--demo civil` starts AfroBIM with it. Every number in the demonstration is an **example value, not a design, a survey or a standard**. The objects are named "(example)" or "(example values)" and their source records say so.

Start in a new document (`NEW`) so the demonstration does not mix with other work. Each step names the command to type or the place to click. Steps 3 to 13 each open one object family described in section 6.

1. **Open the Civil workspace.** Use Window → Workspace → Civil, or the workspace switcher at the left end of the ribbon. The ribbon now shows Home, Annotate, Civil, Insert, Edit, Manage, View and Help. The left panel holds only the toolspace, which fills it.

2. **Build the demonstration data.** Type `CIVILDEMO` (alias `DEMOCIVIL`), or use Civil → Demonstration data, or the Demo button in the Civil tab's Quantities panel. The command adds:
   - a grid of 256 points, 16 by 16 at 20 m spacing, with field codes `GR` (ground) and `TR1` (tree) and the description key table that turns them into full descriptions;
   - a point group, "Ground", and a surface, "Demo existing ground (example data)", built from that group;
   - an alignment, "Demo road (example)", with a design profile, "Demo design profile (example values)";
   - an existing ground profile and a profile view, both placed south of the site;
   - an assembly, "Demo assembly (example values)", and a corridor, "Demo corridor (example values)";
   - sample lines every 30 m, with an 18 m swath each side;
   - two parcels, "Demo parcel 1" and "Demo parcel 2", and a road reserve of 15 m each side that splits them.

   If the document has no coordinate system, the command sets WGS 84 / UTM zone 36S. The status line repeats that the data are example values. Run the command once in a document. Each run adds a further set of objects.

3. **Check the coordinate system.** In the toolspace's Settings tab, the project coordinate system reads WGS 84 / UTM zone 36S, with easting about 450 000 m and northing about 9 960 000 m. The status bar shows the cursor in those metres, with latitude and longitude beside them. Nothing has moved: the data were built at those coordinates (section 4).

4. **Look at the points.** Type `POINTTABLE` (alias `PTTABLE`) to open the point table. It is sortable and filterable, and cells are edited there. Changes are written back with Apply and close. Then `POINTGROUPS` shows the "Ground" group, which includes the raw descriptions that start `GR`. `DESCKEYS` shows the description keys that the demonstration set up.

5. **Look at the surface and its style.** Type `SURFACESTYLE` (alias `CONTOURS`) to set the triangles, the border, the contours and the slope bands for "Demo existing ground". Type `SURFACEDEF` to see its definition: one operation, which adds the points of the "Ground" group, with the build report beside it. Then `SPOT` (alias `SPOTELEVATION`) reads the surface at a point inside the grid, for example easting 450 150 and northing 9 960 150, in metres. `FLOWPATH` traces the water path from a point over the surface.

6. **Look at the alignment.** Open the Prospector, and under Alignments select "Demo road (example)". Its design profile is "Demo design profile (example values)". Type `ZOOM` to see the whole site, or double-click the alignment in the Prospector to zoom to it.

7. **Look at the ground profile and the profile view.** Type `GROUNDPROFILE` (alias `EXISTINGPROFILE`) to open "Demo ground profile (example data)", which is sampled from the demonstration surface along the alignment at 10 m. The profile view "Demo profile view (example values)" is drawn in the model with its grid, its ground and design lines, and its bands. Type `PROFILEVIEW` (alias `PVIEW`) to draw another one. Use `ZOOM` or the Prospector to find it.

8. **Open the assembly editor.** Type `ASSEMBLY` (alias `ASSEMBLIES`) to open "Demo assembly (example values)". It has a lane of 3.5 m, a shoulder of 1.5 m and a daylight part at 2:1 on the right side. Its live section preview shows the result. The part kinds are listed in section 6.4.

9. **Open the corridor.** Type `CORRIDORMODEL` (alias `NEWCORRIDOR`) to open "Demo corridor (example values)". It has one baseline, "Demo baseline", which holds one region, "Whole length". The region runs the full length with the demonstration assembly, and its ground is the demonstration surface. Slow builds such as a corridor run on a worker thread.

10. **Look at the sample lines and the section views.** Type `SAMPLELINES` (alias `SL`) to see "Demo sample lines" with its stations and swaths. Then `SECTIONVIEWS` (alias `SECVIEWS`) draws the section views of the group as an array in the model, from the insertion point of the demonstration. Use `ZOOM` to find them.

    `SECTIONS` (Roads → Earthworks panel, Analyse menu) does **not** show the demonstration corridor. It reads the older corridor element, `CORRIDOR`, and the civil corridor model is a different object. Use `SECTIONVIEWS` for the civil model.

11. **Look at the earthworks and the mass haul.** Type `EARTHWORKS` (aliases `CIVILQUANTITIES` and `MASSHAUL`). The window has the earthworks table, the material volumes by shape code and the mass haul diagram. Enter a free haul distance to see the haul figures. Export CSV from the window, or use **Send to the bill** to add the totals to the bill's Civil earthworks and Civil materials categories. Those categories have no price until you enter rates for them.

12. **Look at the parcels and the acquisition schedule.** Select "Demo parcel 1" in the Prospector and type `PARCELPROPS` to see its owner ("Example owner A") and land reference. Type `PARCELTOPOLOGY` to check the two parcels for gaps and overlaps. The demonstration also splits the parcels by the road reserve, which makes take and remainder parcels. The originals are hidden, not deleted. Type `ACQUISITION` (alias `PARCELSCHEDULE`) for the land acquisition schedule and the area table, with CSV export.

13. **Export the civil data.** Type `CIVILXML` to write everything civil to a LandXML file, with the project coordinate system and the source coordinates. Then type `IFCOUT`. The demonstration surface is written as terrain. The other civil objects are not mapped to IFC, and the export lists what it left out. They stay in the document and in the LandXML file.

14. **Save.** Type `SAVEAS` (Ctrl+Shift+S) to keep the document. Load the LandXML file again with `LANDXML` (alias `ALIGNMENT`) to see the import dialog and its check against the project coordinate system.

## 6. Civil objects

The civil objects are the design objects of a road or site: points, ground surfaces, alignments with their profiles, assemblies and corridors, sample lines and section views, quantities, parcels, and their exchange with LandXML and IFC. Each one is a document element with a name, a status (Draft, For review, Approved or Superseded), a source record and a property set. Each one sits at its source coordinates (section 4).

Every dimension, interval, spacing, tolerance and factor in these dialogs is an input. A field starts empty, or blank means "none". Where a dialog has an **Example values** button, it fills demonstration numbers, which are not from any standard. The civil dialogs are in the Civil tab, the Insert tab (survey points, surfaces and the LandXML import) and the Design tab for alignments. The Civil menu gives the same commands.

Slow work, such as building a surface or a corridor, volumes and earthworks, runs on a worker thread, so the window keeps responding.

### 6.1 Points

A point set is one document element. It holds the points, the point groups and the description keys. Keeping the set in one element keeps the undo history and the change check quick. A test with 100 000 points builds and reloads in about 5 seconds in a debug build.

| Command | Aliases | What it does |
|---|---|---|
| `POINTS` | `IMPORTPOINTS`, `COGOPOINTS` | Imports COGO points from a point file. The format is detected and previewed, the coordinate system is checked, and the duplicate policy is yours. |
| `POINTCREATE` | `CREATEPOINT`, `PT` | Creates a point by typed easting, northing and elevation, or by picking in the model. |
| `POINTTABLE` | `PTTABLE` | Edits the points of a point set in a table: sort, filter and edit any cell. |
| `POINTGROUPS` | `PTGROUPS` | Point groups: include and exclude rules, surface points, order. |
| `DESCKEYS` | `DESCRIPTIONKEYS` | Edits the description keys that turn field codes into full descriptions, styles and layers. |
| `POINTEXPORT` | `EXPORTPOINTS` | Exports the points of a point set or group to a point file. |
| `COGO` | `INVERSE`, `COGOTOOLS` | COGO tools: inverse, bearing and distance, intersections, station and offset to an alignment. |
| `TRAVERSE` | `TRAV` | Enters a traverse and adjusts it by the compass or transit rule, with a misclosure report. |

**How to.** For a point file, run `POINTS`, choose the file, and check the preview by eye. The order of the columns cannot be known from the numbers alone. Choose the order when two orders fit. The file's units can be metres, international feet or US survey feet, and the points are converted to metres. The import checks the file's system against the project (section 4.5). Then choose the duplicate policy:

- stop at the first clash and change nothing;
- skip points whose number is taken;
- give clashing points the next free number.

Point groups take include and exclude rules, a surface-points flag and an order. A description key turns a field code, such as `TR*`, into a full description, a point style, a label style, a layer and a classification slot. The table's changes are written back with Apply and close.

**Limits.**

- Plane grid only. There is no scale factor, no grid convergence and no earth curvature: distances are grid distances.
- Point files are read in metres, feet or US survey feet only, and the factor is the one the dialog states.
- No least-squares network adjustment. The traverse is adjusted by the compass (Bowditch) or the transit rule only.
- The column order of a point file is detected by a heuristic. Always check the preview, and set the order yourself when the dialog says the order is ambiguous.
- Point groups are rules, not stored lists. A point deleted from the store leaves its groups at the next refresh.
- The first description key in the set that matches wins.
- A point group's "surface points" flag selects points for a surface. No surface is built by the group itself.
- The engine has no default tolerance for duplicate positions, closure or angles. Each one is an input.
- Classification slots take a scheme and a code of your choosing. Nothing checks them against a published table.

### 6.2 Surfaces

A surface has a name, a role (existing ground, design, subgrade, formation or other), a status, a source record, and a definition. The definition is an ordered list of operations. It is stored as data and the surface is rebuilt from it on load. The build report lists what was used, merged, excluded and left out, and every line crossing the build found.

| Command | Aliases | What it does |
|---|---|---|
| `SURFACENEW` | `CREATESURFACE`, `NEWSURFACE` | Creates a surface from a definition: a name, a role and operations. |
| `SURFACEDEF` | `SURFACEDEFINITION`, `DEFINITION` | Edits the definition of a surface: operations, order, rebuild and the build report. |
| `SURFACESTYLE` | `CONTOURS`, `SURFSTYLE` | Surface style: triangles, border, contours, elevation and slope bands, slope arrows. |
| `SURFACE` | `TIN`, `TERRAIN` | Imports ground surfaces (TIN) from a LandXML file. |
| `SURVEY` | `SURVEYPOINTS`, `POINTFILE`, `TOPO` | Makes a ground surface from a file of survey points: column order, coordinate system, longest triangle side, descriptions to leave out. |
| `SPOT` | `SPOTELEVATION`, `SPOTELEV` | Reads the elevation of a surface at an easting and northing. |
| `FLOWPATH` | `WATERDROP` | Traces the steepest-descent path of water from an easting and northing over a surface. |
| `VOLUMES` | `VOLUMEDASHBOARD`, `CUTFILL` | Volumes between two surfaces, or a surface and a level, bounded by an optional polygon, with a stage-storage table. |

**How to.** A surface is built from operations, grouped on four tabs of the definition dialog: Points, Breaklines, Boundary and Edits. Points come from a point group. Breaklines are standard (a 3D line whose vertices become surface points) or proximity (a plan line whose elevations come from the surface of the other data). A boundary is outer (keep inside), hide (remove inside) or show (restore inside). The edits are the longest triangle edge, excluded elevations, raise or lower, adding or deleting a point, and deleting or swapping a triangle edge. Data operations act on the data gathered before them in the list. Triangulation operations (maximum edge length, boundaries, and the delete and swap edits) act on the triangulation of all the data.

Volumes between two surfaces are exact for the triangulated surfaces. They are not a grid estimate. The first surface is the existing ground and the second the design. Cut is existing above design. The net is cut less fill.

**Limits.**

- Breaklines come from typed vertices, or from an alignment at its design profile. Picked polylines are not offered.
- Contour data become surface points only. They are not breaklines, so a surface built from contour lines alone has flat triangles along them.
- The volumes have no swell, shrinkage or compaction factor. Those are not applied anywhere in the surface volumes.
- The grid volume method is a check, not a substitute for the exact method.
- Crossing breaklines are given the mean of their two elevations at the crossing, and both are reported. This is the crate's own choice, not a standard. Check the report.
- Points closer than the duplicate tolerance merge. The engine default is 1 mm, with a choice of policy: first, lowest, highest or average.
- Flow paths stop in every pit, however small. Pits are not filled, and a path never splits.
- A surface cannot have wall breaklines, watershed delineation (only a flow path from a point), depression filling, or natural-neighbour or kriging interpolation.
- Survey files need their column order given. It is never guessed.
- The 3D view shows a thinned copy of a surface above 40 000 triangles. The 2D view and every calculation use the full surface.

### 6.3 Alignments and profiles

An alignment is a horizontal geometry with stations, and a profile is its vertical geometry. A designed alignment is made from points of intersection (PIs). Its tangents, spirals and arcs are worked out from the PIs. Its profile is made from points of vertical intersection (PVIs), each with a parabolic curve given by length or by K.

| Command | Aliases | What it does |
|---|---|---|
| `ALIGNDESIGN` | `NEWALIGNMENT`, `DESIGNALIGNMENT` | Designs an alignment from PIs picked on the plan or typed as coordinates: name, start station. Curves start with no radius. |
| `ALIGNEDIT` | `PLANEDIT`, `PIEDIT` | Edits a designed alignment in plan: drag PIs, insert a PI on a tangent, delete a PI. A move the engine refuses shows its reason. |
| `PITABLE` | `PIS`, `CURVETABLE` | The PI table: station, coordinates, deflection, radius, spirals, tangent and curve length, with the feasible radius range. |
| `ALIGNLABELS` | `CURVELABELS` | Shows or hides the curve data labels on the plan. |
| `PROFILEDESIGN` | `PROFILEEDIT`, `PVI` | Profile editor of the designed alignment: ground line, PVIs to click and drag, the PVI table, vertical exaggeration and Fit to ground. |
| `ALIGNCHECK` | `DESIGNCHECK`, `GEOMCHECK` | Checks a designed alignment against a limits table you load (CSV or JSON), or against the built-in MoWT basis: per element, with the source of each limit. |
| `OPTIONS` | `ALIGNOPTIONS`, `OPTIONSET` | Adds the current design as an option, compares options on figures worked out from the model, and scores them with your own weights. |
| `LANDXMLOUT` | `EXPORTLANDXML` | Exports the designed alignment and its profile as LandXML. |
| `LANDXML` | `ALIGNMENT` | Imports alignments from a LandXML file, with their profiles where the file has them (section 6.8). |
| `GROUNDPROFILE` | `EXISTINGPROFILE`, `SAMPLEPROFILE` | Samples an existing ground profile from a surface along an alignment. |
| `PROFILEVIEW` | `PVIEW` | Draws a profile view in the model: grid, ground and design lines and bands. |

**How to.** Run `ALIGNDESIGN`, give the name and the start station, then pick or type the PIs. Enter each curve's radius in the PI table (`PITABLE`). Edit the plan with `ALIGNEDIT`. Design the profile with `PROFILEDESIGN`, and use Fit to ground for a starting profile. Then run `ALIGNCHECK` with a limits file, or with the built-in MoWT basis. A profile view shows the ground and design lines of the alignment at the insertion point you give.

**Limits.**

- The design builds tangents, spirals and arcs from PIs. A PI with spirals needs each spiral to turn less than the deflection of the PI. Compound curves, which are curves of different radius in sequence, are not made from PIs.
- Vertical curves in the design are parabolas, given by length or by K. Asymmetric and circular vertical curves are read and written by the alignment and LandXML code, not offered by the design.
- Superelevation is not computed. There is no rate from speed and radius, no design table, and no runoff or runout length. Only the cross-fall the data state is returned.
- The design checks run on the design's own limits. They do not check lateral sight distance on horizontal curves, passing or decision sight distance, superelevation rates and runoff, the combination of horizontal and vertical alignment, or cross-section elements.
- The built-in MoWT basis is the Uganda MoWT Road Design Manual Volume I (January 2010). It was taken from a third-party copy and not compared with a ministry original. The design's own README states this, and the values are stated with their table and page.
- Fit to ground gives a starting point only. It is a greedy fit, not an optimisation.
- The first PI sits at the start station. A profile must cover the alignment's stations for the corridor to use it, and a warning says so when it does not.
- An imported alignment cannot be made editable. The alignment engine holds it as lines, arcs and spirals, not as PIs with radii. The ribbon shows that tool as a disabled "Planned" button (section 13).
- The LandXML reader has been tested against one Civil 3D export, an alignment with a design profile and an existing ground profile in US survey feet. Other LandXML files were written by hand to the schema (section 6.8).

### 6.4 Assemblies and corridors

An **assembly** is a typical cross-section built from parts. The parts are described going outward from the centreline, on the left side and the right side. A **corridor** carries one or more assemblies along a baseline, which is an alignment, between stations. It is cut and filled against a ground surface.

| Command | Aliases | What it does |
|---|---|---|
| `ASSEMBLY` | `ASSEMBLIES`, `SUBASSEMBLY` | Assembly editor: builds a typical section from the subassembly library with a live preview. |
| `CORRIDORMODEL` | `NEWCORRIDOR`, `CORRIDOR2` | Corridor from a baseline, regions with an assembly each, a frequency and a ground surface. |
| `CORRIDOR` | none | The older corridor: a typical section along an alignment against a surface. It keeps working on old documents. Use `CORRIDORMODEL` for new work. |
| `SECTIONS` | `XSECTIONS` | Shows the cross-sections of the selected older corridor, station by station. |

**Part kinds in the assembly library.** The editor offers 14 part kinds: Lane, Shoulder, Footway, Verge, Kerb, Kerb and channel, Raised median, Depressed median, V ditch, Trapezoid ditch, Retaining edge, Daylight, Lined V drain and Lined trapezoid drain. Each takes its own parameters: for example a lane takes a width and a cross-slope, a kerb takes a height, a width and a bed depth, and a daylight part takes a cut run and a fill run per unit of rise. Layers take a shape code, a depth and a material.

**How to.** Build the assembly in `ASSEMBLY`, with its left and right sides, and check its preview. Then in `CORRIDORMODEL`, give the baseline alignment, add regions along it with an assembly for each, set the frequency of sections on tangents, curves and spirals, and choose the ground surface. Sections fall at the spacings you enter, and at the geometry points (element starts, PVIs and vertical curve ends) and superelevation stations if you ask.

**Limits.**

- Everything is straight links in section. Targets (a fixed offset, a table of stations and offsets, a polyline, or an elevation from a profile) are evaluated at each section only. Between sections the surface is a straight strip.
- A kerb or median is a rectangle. There is no batter, no radius and no kerb type library.
- The daylight is the first meeting of slope and ground. Benches exist in the assembly model only.
- Shapes overlap if their parts overlap. Nothing checks this.
- The corridor surface is open, not a closed solid, and quantities do not come from it.
- The sheet set reads the older corridor element. The civil corridor model does not feed the existing section sheets.
- Nothing in the corridor model has been compared with Civil 3D. Its hand calculations use arbitrary example numbers.
- No IFC export of corridor shapes. Corridor objects are not mapped to IFC.
- Width and slope transitions are not built, except through targets.

### 6.5 Sample lines and section views

A **sample line group** is a set of lines across an alignment, each with its own left and right swath. The surfaces are cut where each line crosses them. A **section view** is the drawing of one line: the ground of each surface within its swath, the profile level and, when a corridor is given, the design.

| Command | Aliases | What it does |
|---|---|---|
| `SAMPLELINES` | `SAMPLELINEGROUP`, `SL` | Sample line group: stations, swath widths and the surfaces cut at each line. |
| `SECTIONVIEWS` | `SECVIEWS` | Draws the section views of a sample line group as an array in the model. |

**How to.** Run `SAMPLELINES`, choose the alignment and the stations: at an interval (from the start, every interval, and the end), listed, at the geometry points (element starts and the end), or at the superelevation stations. Give each line's left and right swath and the surfaces to cut. Then run `SECTIONVIEWS` and give the insertion point, the number of columns and the vertical exaggeration. The views appear in the model as an array.

**Limits.**

- The design view in a section is not cut to the swath.
- A surface that does not cover a line gives no run, and a note says so.
- The section views are not an input to the drawing sheet set.

### 6.6 Quantities

Quantities from the civil objects are collected in one window, `EARTHWORKS`.

| Command | Aliases | What it does |
|---|---|---|
| `EARTHWORKS` | `CIVILQUANTITIES`, `MASSHAUL` | Earthworks table, material volumes by shape code and the mass haul diagram, with CSV export and a send to the bill. |

**How to.** Run `EARTHWORKS` on the corridor. The earthworks table gives the volume between sections by average end area, or by the prismoidal formula where you choose it. The material volumes sum each shape code between consecutive sections, with its classification and cost item code. The mass haul diagram plots the cumulative volume along the stations. Enter the free haul distance to see the balanced volume, the free haul volume and the overhaul. Export CSV from the window. **Send to the bill** adds the totals to the bill's Civil earthworks and Civil materials categories (section 8.12).

The cut and fill factors are inputs. A blank field means no factor.

**Limits.**

- Earthworks use average end area, with the prismoidal option where you choose it. The material volumes use average end area on the centreline length, with no curvature correction and no prismoidal option.
- The surface-to-surface earthworks are cut where the existing surface is above the proposed one, over the swaths of the sample lines.
- The mass haul pairs crossings from the start of the curve. A curve that could balance either way is balanced the forward way.
- No haul cost, and no choice of borrow or waste sites.
- Nothing has been compared with Civil 3D. The corridor's hand calculations use example numbers.
- Quantities are not priced by AfroBIM. The rates are yours (section 8.12).

### 6.7 Parcels

Parcels are plane polygons with boundaries of lines and circular arcs, carrying an owner, a land reference, a land use and a status.

| Command | Aliases | What it does |
|---|---|---|
| `PARCELPICK` | `CREATEPARCEL`, `PARCEL` | Creates a parcel from typed or picked corner points. |
| `PARCELDEED` | `DEED` | Creates a parcel from the courses of a deed (bearing and distance), with a closure report. |
| `PARCELPROPS` | `PARCELPROPERTIES` | Parcel properties: owner, land reference, land use and status. |
| `PARCELTOPOLOGY` | `TOPOLOGY` | Checks the parcels of the site for gaps and overlaps. |
| `PARCELSPLIT` | `RESERVE`, `ROADRESERVE` | Splits the parcels by an alignment offset (road reserve): take and remainder parcels. |
| `ACQUISITION` | `PARCELSCHEDULE`, `LANDSCHEDULE` | Land acquisition schedule and parcel area table, with CSV export. |

**How to.** Create a parcel from its corners, or from a deed's courses. A deed parcel is made only when its closure is within the tolerance you enter, and the closure is reported. The topology check reports shared boundaries, gaps and overlaps for the snap tolerance you enter. The road reserve split takes a width to the left and a width to the right of an alignment. It makes take parcels and remainder parcels, and **the original parcels are hidden, not deleted**. The acquisition schedule groups the pieces under the parcel they came from.

**Limits.**

- Boundaries must not cross themselves. The check reports a crossing and repairs nothing.
- Parcels have no holes (islands).
- A deed arc is given by its radius, its direction and its chord, or its arc length and tangent bearing. Compound and reverse curves are written as consecutive calls. Spirals are not supported.
- Gaps are found only where a gap is fully enclosed by parcel boundaries. A sliver open to the outside is not reported.
- Vertices closer than the snap tolerance merge. A snap larger than the real misfit of the data hides gaps of that width.
- Topology and split work on boundaries flattened to a chord tolerance you give. The areas of overlap, gap and take pieces are short by at most two-thirds of the arc length times the tolerance, for each arc involved. The engine returns that bound. The take plus the remainder always equals the exact parcel area.
- An offset that reverses a segment or crosses itself is refused, not trimmed. Arcs must be flattened first.
- Plane grid only. No datum or projection handling, and no scale factor.

### 6.8 LandXML and civil exchange

**Import.** `LANDXML` (alias `ALIGNMENT`) opens the LandXML import dialog. It reads LandXML 1.x files, converts lengths to metres, and shows tabs for the file, alignments, surfaces, points, parcels and profiles. Tick what to bring in. Alignments come in with their profiles, station equations and superelevation where the file has them. Surfaces come in as ground surfaces (TIN). Points come in as point sets. Ground profiles come in for profile views. The dialog's "Coordinate system in the file" section and the check in section 4.5 are shown before you import. Objects come in at the file's coordinates. Nothing is reprojected, shifted or re-based, whatever the check says.

`SURFACE` (aliases `TIN`, `TERRAIN`) imports only the ground surfaces of a LandXML file, and `SURVEY` makes a surface from a survey point file (section 6.2).

**Export.**

- `CIVILXML` (aliases `LANDXMLALL`, `LANDXMLCIVIL`) writes everything civil that the document holds to one LandXML file, with the project coordinate system and the source coordinates. Alignments are written with their profiles, point sets as control points, parcels, surfaces (with the breaklines of a definition), ground profiles, and the cross-sections of the sample line groups.
- `LANDXMLOUT` (alias `EXPORTLANDXML`) writes the designed alignment and its profile.
- `IFCOUT` maps only the civil surfaces, which become terrain (`IfcGeographicElement` with the predefined type `TERRAIN`). Alignments are written as before. Point sets, parcels, ground profiles, profile views, assemblies, corridors and sample line groups are **not mapped to IFC**. The export says what it left out, and they remain in the document and in `CIVILXML`.

**Limits.**

- The LandXML reader has been tested on one Civil 3D 2023 export, an alignment with a design profile and an existing ground profile, in US survey feet. Other LandXML files in the tests were written by hand to the schema. Nothing has been read from Civil 3D beyond that one file.
- Read from that file: units, coordinate system, horizontal geometry, the ground profile, the alignment profile and the plan features. No surface, parcel, point, cross-section or superelevation from Civil 3D has been read in the tests.
- Elements the whole-file reader does not read are counted and reported, not treated as errors. They include roadways, pipe networks, survey data, monuments, grade models, cant, watersheds, volumes and feature dictionaries.
- Station labels and other text in the plan drawing are not written.
- The LandXML export has been checked against the schema and the AfroBIM importer, not against a file read by Civil 3D or another program.
- A spiral in a LandXML parcel boundary is carried as its chord.

## 7. First hour walkthrough: signs and roads

This walkthrough follows the order a road design takes, for signs, markings, drainage, barriers and lighting. For the civil design objects, see chapter 5. Each step names the commands to type or the ribbon tab to use. Start with a new document (`NEW`) or open one (`OPEN`), or use `--demo` to see a populated model. The demo's values are arbitrary.

1. **Settings and units.** Choose the design standard with `UK` (UK Traffic Signs Manual) or `UGANDA` (Uganda Traffic Signs Manual 2004). Set the project information with `PROJECTINFO` (Manage tab). Set the display units with `UNITS` (length, stations, areas, volumes, angles). Set the speed band rule for km/h limits with `PROJECTSETTINGS`. The rule is stored in the document and is stated in every calculation that uses it.
2. **Alignment.** Either import one with `LANDXML` (File → Import LandXML, or Roads → Alignment), or design one. To design one: `ALIGNDESIGN`, name the alignment, give its start station, then pick PIs on the plan or type them as `x,y` on the command line. Curves start with no radius; enter each radius in `PITABLE`. Edit the plan with `ALIGNEDIT` and the vertical profile with `PROFILEDESIGN`.
3. **Check the alignment.** Load a limits table (CSV or JSON) and run `ALIGNCHECK`, or use the built-in MoWT Road Design Manual Vol. I basis, which holds its own limits and their sources. Compare design options with `OPTIONS`.
4. **Ground surface and corridor.** Import a survey surface from a LandXML TIN with `SURFACE`, or make one from a file of survey points with `SURVEY`. Build a corridor with `CORRIDOR` (the older corridor; the civil corridor model is `CORRIDORMODEL`, section 6.4): a typical section (lane, shoulder, formation depth, side slopes, all entered), along the alignment against the surface. View cross-sections of the older corridor with `SECTIONS`. The earthworks by average end area are under `QUANTITIES`.
5. **Signs.** Insert a sign with `STACK`, `DIRECTION`, `CONFIRM`, `PLACE`, `LANE`, `DIVERSION`, `MAP`, `TEE` or `ROUNDABOUT`, or pick a standard sign from `LIBRARY`. Place the selected sign on the alignment by station and offset with `STATION`, entering the station as shown on the drawings. If the alignment has station equations, pick the stationing region.
6. **Sign supports.** Select the sign and type `SUPPORTS` to add posts and foundations. Enter the real dimensions in Properties. Type `DESIGN` to size the posts and foundations for wind. Enter the ground and wind inputs in the dialog. For a sign on a gantry, use `GANTRY` or `CANTILEVER`, then `HANG`.
7. **Road markings.** Lay longitudinal lines with `MARKINGS`, choosing the line type, speed limit, width and station range. Lay transverse, arrow, hatching, yellow-line, stud and worded markings with `MARKTRANSVERSE`, `MARKARROW`, `MARKHATCH`, `MARKYELLOW`, `MARKSTUDS`, `MARKWORD`, and the Uganda lines with `MARKLINES2`.
8. **Drainage and culvert check.** Add a side drain with `DRAIN` and a pipe culvert with `CULVERT`. Both are added with placeholder dimensions: set the dimensions in Properties. Work out the peak flow with `RATIONAL` (catchment areas, runoff coefficients and an intensity you enter), and keep it on the selected drain or culvert. Run `CHECK` on the drain for its capacity. Run `CHECK` on the culvert to open its dialog, which takes the design flow, the basis (MoWT, UK DMRB or SANRAL) and the culvert inputs, and then runs the culvert check.
9. **Pavement design.** Choose a method with `PAVEDESIGN`: MoWT Volume 3 Part I (Uganda), UK DMRB CD 224 and CD 226 (thickness design of superseded revisions only), AASHTO 1993, or SAPEM Chapter 10 (which returns no layers). If the method returns layers, the dialog can make a pavement from them. Lay a pavement along the alignment with `PAVEMENT`.
10. **Barriers and lighting.** Lay a safety barrier run with `BARRIER` (containment, working width class, impact severity and product data entered). Run `CHECK` on it with the speed limit. Lay a lighting installation along the alignment with `LIGHTING`, or a single column with `LIGHTCOLUMN`. Enter the luminaire file for `PHOTOMETRY` if you have one, and run `CHECK` on the installation for the lighting requirements.
11. **Sign visibility.** For each sign, run `VISIBILITY` to enter the inputs (accept the eye height), then `VISIBILITYRUN` for the results.
12. **Model-wide check.** Run `CHECKALL` (Design tab, Checks panel, or Manage tab). Every element with an input needed names the dialog to open. Fill those dialogs and run `CHECKALL` again.
13. **Clash detection.** Add a clash test with `CLASHTESTS` (selection A and B, hard or clearance, tolerance in millimetres that you set). Run it with `CLASH`. Review results with `CLASHES`, and show markers in the 3D view with `CLASHMARKERS`. Raise issues from clashes with `ISSUES`. Export them with `BCFOUT`.
14. **Programme and 4D.** Create the programme with `PROGRAMME`: name, start date, working weekdays and activities (durations in working days). Set the calendar's days off with `CALENDAR`. Run `SCHEDULE` for the critical path. Link elements to activities with `LINK4D`, check the links with `LINKREPORT`, and set `BASELINE`. Show the model at a date with `FOURD`, or play it with `PLAY4D`.
15. **Bill, rates and estimate.** Create the bill of quantities with `BILL` (sections, items and mapping rules). Enter rates with `RATES` (each rate with its currency, source note and date). Set currency labels and exchange rates with `CURRENCY`. Read the priced bill with `ESTIMATE`. Phase it with `CASHFLOW`. Work out an interim payment certificate from your own payment terms with `CERTIFICATE`.
16. **Sheets and the report pack.** Make a sheet set with `SHEETSET` (plan, profile, section, schedule, detail and notes sheets, each with its paper and scale entered). Check the composed set with `SHEETPREVIEW`. Edit the title block with `TITLEBLOCK`. Export the set with `SHEETPDF` (one PDF) or `SHEETDXF` (one DXF per sheet). Compose the calculation report pack with `REPORTPACK`.
17. **Exchange.** Export the model as IFC 4.3 with `IFCOUT` (choose the alignment mode in its dialog). Export quantities with `QUANTITIES`, DXF with `DXFOUT`, and the fabrication sheets with `SHEETS`. Save the document with `SAVE`.


## 8. The ribbon tabs, in ribbon order

Each tab chapter has four parts: what the tab is for; a table of its commands with their aliases, what each asks for, and what it produces; and a Limits list. The limits come from the crate READMEs and from the dialogs. Every command on a tab appears in its table. Section 9 lists every command in the command table.

The "Asks for" column names the fields of the dialog as they are labelled on screen. Dialogs open as windows inside the central area, below the ribbon. A dialog's Cancel button leaves the document unchanged. Where a command is a toggle or view setting, "Produces" says so.

### 8.1 Home

**Purpose.** The everyday tab: file commands, the sign and support commands for a selected sign, placing a sign on an alignment, the view, and the drafting aids. Its panels are File, Signs, Modify, Supports, Alignment, Navigate and Drafting aids.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `NEW` | none | Nothing | An empty document. |
| `OPEN` | none | A file to open | The document is loaded. |
| `SAVE` | `QSAVE` | Nothing | The document is saved to its file. |
| `SAVEAS` | none | A file name | The document is saved under the new name. |
| `UNDO` | `U` | Nothing | Reverts the last edit. |
| `REDO` | none | Nothing | Re-applies the edit that was undone. |
| `STACK` | `SIGN` | A click to place (the sign hangs from the cursor) | A stack-type direction sign. |
| `LIBRARY` | `INSERT`, `LIB` | The Standard signs window: a Find box and entries grouped by class | A catalogued sign, inserted by a click and placed by a second click. |
| `MOVE` | `M` | A base point, then a second point (picked, or typed as `x,y` or `@dx,dy`) | The selection moved. |
| `COPY` | `CO`, `CP` | A base point, then a point for each copy; Enter to finish | Copies of the selection. |
| `ERASE` | `E`, `DELETE` | The selection, or a pick if none is selected | Elements removed (undoable). |
| `ARRANGE` | none | Nothing | Every element set out in rows. |
| `SUPPORTS` | `POSTS` | The selected sign. The Supports dialog: Mounting height (mm), Number of posts, Edge inset (mm) | Posts and foundations with placeholder dimensions, hosted by the sign. |
| `DESIGN` | `SIZE` | The selected sign. The Design dialog: the wind basis, ground and foundation inputs (see below) | A calculation window and the sized posts and foundations. |
| `STATION` | `CHAINAGE` | The selected sign. The Place dialog: Alignment, Station (m), Stationing region (when the alignment has station equations), Offset from centreline (m) (0 to 200), Side (Left or Right, looking up-station) | The sign's site placement; its supports move with it. The status line gives the mark, station, offset, side and the centreline level. |
| `ZOOM` | `Z`, `ZE` | Nothing | The view fits every element. |
| `ZOOMWINDOW` | `ZW` | Two corners | The view fits the window. |
| `3D` | `ORBIT`, `PLAN` | Nothing | Switches between 2D model space and the 3D site view. |
| `GRID` | none | Nothing | Toggles the grid (F7). |
| `SNAP` | none | Nothing | Toggles snap of picked points to a 50 mm grid (F9). |
| `ORTHO` | none | Nothing | Toggles ortho, which holds moves to the X or Y axis (F8). |

*Supports (Home, Modify and Structures tabs):* the Supports dialog asks for a mounting height above zero (the hint says "ground to the sign's lower edge"), a number of posts (1 to 6, starting at 2) and an edge inset (0 to 5000 mm, starting at 150). A sign hanging on a gantry takes no posts; take it off with `UNHANG` first.

*Design (Home, Modify and Structures tabs):* the Design dialog asks for the mounting height, posts and edge inset as above, and for the basic wind pressure (MoWT Volume 4 Table 2, open terrain or built-up, or an entered value in kN/m²). For a planted foundation: the method (PD 6547 or CD 354 rev 1), the foundation diameter, the ground factor G and the minimum planting depth. For a spread foundation: the assumed ground class (Poor, Average or Good), breadth, thickness and top below ground. Lines that show FAILS, or start with Check, are shown in amber. The Copy button copies the whole calculation. The standards and limits of the design are in 5.9 (Structures).

**Limits.**

- The placement rules are a port of the C# `SignPlacement` and match it on 4,256 cases. Stations are read through the alignment's station equations. A station that occurs in more than one region is refused until you pick the region.
- A sign's supports follow it when it moves. Supports are not checked against other elements.
- `MOVE` and `COPY` typed points accept the document's length unit (a bare number is in the display unit; a trailing `m` or `mm` overrides it).
- Object snap applies to the base point and second point of `MOVE` and `COPY`, to dragging a grip of an element and to placing a sign hung from the cursor (the sign's centre lands on the snap point); a press-and-drag move of the selection snaps to the grid only. See section 8.4 for what provides each snap. The plan editor of the alignment tools has its own snap to PIs and alignment ends.

### 8.2 Edit

**Purpose.** History, the clipboard and selection.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `UNDO` | `U` | Nothing | Reverts the last edit. |
| `REDO` | none | Nothing | Re-applies the edit that was undone. |
| `COPYCLIP` | none | The selection | The selection on the clipboard. |
| `PASTECLIP` | `PASTE` | Nothing | The clipboard pasted into the model space. |
| `ERASE` | `E`, `DELETE` | The selection, or a pick | Elements removed (undoable). |
| `SELECTALL` | `ALL` | Nothing | Every element selected. |

**Limits.** Undo covers the document's edits, including the layer commands for names, colour and weight. It does not cover the layer on and lock settings, which the kernel keeps outside its undo snapshot. The Layers panel's visibility and lock changes are therefore not undone.

### 8.3 Insert

**Purpose.** Standard signs, imports of alignments, ground surfaces, a sign schedule, DXF underlays and IFC reference models.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `LIBRARY` | `INSERT`, `LIB` | The Standard signs window (Find box; entries grouped by class) | A catalogued sign, placed by a click. |
| `LANDXML` | `ALIGNMENT` | A LandXML 1.2 file | One alignment element for each alignment read, on a hidden layer. Its notes go to the command history. |
| `OPEN` | none | An AfroBIM document | The document is loaded. |
| `SIGNSCHEDULE` | `SIGNSCHED` | A CSV file (comma, semicolon or tab; quoted; with or without a byte order mark). The column mapping, with a dropdown per file column and a unit for lengths; save or load the mapping as JSON; then the import plan | Sign elements from the rows. Each row that cannot be read gives its reason and line number. Duplicate marks are skipped or updated. Apply is one undo step and ends in a report. |
| `UNDERLAY` | `XREF`, `XATTACH`, `DXFUNDERLAY` | One or more DXF files, or DWG files when ODA File Converter is installed; the file units and millimetres per unit (can be overridden); insertion offset (mm), rotation (degrees) and scale; visibility and fade; per-layer switches; optionally draped flat at a level in the 3D view | A read-only underlay element, stored by path and an FNV-1a hash. |
| `SURFACE` | `TIN`, `TERRAIN` | A LandXML file with a TIN | A ground surface element, used by the profile editor, the corridor and the dimension of a level. |
| `SURVEY` | `SURVEYPOINTS`, `POINTFILE`, `TOPO` | A text file of survey points, one a line (comma, semicolon, tab or blanks; with or without a heading). The column order (`PNEZD`, `PENZD`, `NEZD`, `ENZD`), with the first points shown as they are read; the coordinate system (a WGS 84 / UTM zone, optional); the longest triangle side in metres (optional); the descriptions to leave out, ticked from the list of those in the file with their counts | A ground surface element triangulated from the points, with its coordinate system kept. The account of the import is in the command history: points read, left out, merged and used, triangles, extent, and the latitude and longitude of the middle of the survey. |
| `IFCREF` | `REFERENCEMODEL`, `IMPORTIFC` | One or more IFC files (IFC2X3, IFC4 or IFC4X3). A tick box applies the file's IfcMapConversion when it has one; the default is the file's own coordinates | Read-only reference models, stored by path and hash and read again on open. |
| `REFMODELS` | `REFERENCES`, `REFLIST` | The reference models window: visibility, transparency, offset and rotation, the import report, and the properties of a picked product | Nothing new; it changes how the models are shown. |

**Limits.**

- *LandXML alignments:* not tested against a file exported by Civil 3D. Only the first ProfAlign of a profile is read. Superelevation is stored data only; no rate is computed. Spirals are clothoids only. `IrregularLine` and `Chain` are not supported. LandXML is not written by this command (`LANDXMLOUT` on the Design tab writes it).
- *Survey surfaces:* the LandXML reader was tried on hand-built test files only, not on a file exported by Civil 3D.
- *Survey points:* the column order is yours to state. With a coordinate system given, a file whose coordinates cannot be in it is refused (the usual cause is northing and easting the wrong way round); with none given nothing can be checked. Only WGS 84 / UTM zones are known, and nothing is reprojected: the system is a record. Points at one plan position (to the millimetre) are merged and the first one read is kept. The triangulation follows Delaunay's rule only: **breaklines are not applied**, so a triangle side can cut across a kerb, a channel or a top of bank, and contours are not read. Without a longest side the surface fills the convex hull of the points, across ground nobody surveyed. Which descriptions are not ground levels is the surveyor's convention: nothing is left out unless you tick it. Coordinates and levels are read as metres. A file dropped on the window is not offered as a survey: use the command.
- *Sign schedule:* the CSV is read and written; XML is not read. A column mapping step is needed for any layout other than the AfroBIM template. `SIGNSCHEDULEOUT` and `SIGNSCHEDULETEMPLATE` are on the Output tab.
- *DWG:* AfroBIM does not read DWG itself. When ODA File Converter is installed (free from the Open Design Alliance, opendesign.com; looked for in `/Applications` on macOS, `C:\Program Files\ODA` on Windows, `/usr/bin` or `/opt` on Linux, or wherever `AFROBIM_ODA_CONVERTER` points), a DWG attached as an underlay (`UNDERLAY`, or dropped on the window) is converted to a DXF 2018 in a cache folder in the system's temporary folder and that DXF is read; the DWG itself is only read. Without the converter a DWG is refused with these instructions. The converted DXF is several times the size of the DWG. Civil 3D objects (alignments, profiles, surfaces, corridors, COGO points, labels) come through as the picture Civil 3D saved with them, drawn as lines, arcs, faces and text: **linework, not design objects**. To bring alignments, profiles and surfaces in as design data, export LandXML from Civil 3D and use `LANDXML` and `SURFACE`. An object saved with no picture is left out and counted. Tried on Civil 3D 2018-format (AC1032) drawings with ODA File Converter 27.10 on macOS.
- *DXF underlays:* Read as DXF R12 to current, ASCII. Line types, line weights, hatch patterns, fonts and styles are not read. Drawn in one muted ink at the chosen fade, not in the file's colours. At most 250 000 segments are drawn a frame; the 3D draping uses the first 40 000. The sheet layout and site coordinates are different systems, so an underlay in survey coordinates appears far from the signs (see Troubleshooting).
- *IFC reference models:* read-only, for reference. Openings are not subtracted. Colours, materials, layers and type objects are not read. Files of 4 GiB or more are refused. Not opened in a commercial BIM tool.

### 8.4 Annotate

**Purpose.** Text, dimensions, leaders, measuring, text and dimension styles, and object snap. These are the AutoCAD-style annotation commands. Each runs a small prompt sequence, fed by clicks and by the command line.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `MARKS` | `LABELS` | Nothing | Toggles the element mark under each element. |
| `TEXT` | `DT`, `DTEXT` | A start point, a height, a rotation, then the text | A single line of text. |
| `MTEXT` | `MT` | Two corners (the width), then the text; type `\n` for a new line | Multi-line text in a box. |
| `TEXTEDIT` | `ED` | The selected text or leader (also a double-click on it) | The text edited. |
| `DIM` | none | Two points, then the dimension line; linear or aligned is inferred from the picks | A dimension. |
| `DIMLINEAR` | `DLI` | Two points, then the dimension line (horizontal or vertical) | A linear dimension. |
| `DIMALIGNED` | `DAL` | Two points, then the dimension line (along the line between them) | An aligned dimension. |
| `DIMANGULAR` | `DAN` | The vertex and two points | An angle dimension. |
| `DIMRADIUS` | `DRA` | A centre and a point on the circle | A radius. |
| `DIMDIAMETER` | `DDI` | A centre and a point on the circle | A diameter. |
| `DIMARC` | `DAR` | Centre, start point, end point | The arc length. |
| `DIMCONTINUE` | `DCO` | Continues the selected (or last) linear dimension from its last extension line | A chained dimension. |
| `DIMBASELINE` | `DBA` | Stacks from the first extension line of the selected (or last) linear dimension | A baseline dimension. |
| `DIMSTATION` | `DSTA` | A point on an alignment | Station and offset of the point, in the stations shown on the drawings. |
| `DIMLEVEL` | `DLEV` | A point; the level from a survey surface or the alignment profile, or typed | A level annotation. |
| `LEADER` | `LE` | An arrow point, further points, then the text | A leader with its text. |
| `MLEADER` | `MLD` | An element; the leader shows its mark and follows a rename | A mark leader attached to the element. |
| `DIST` | `DI`, `MEASURE` | Points; more points give a running total | The distance, shown on the command line and kept in `MEASURERESULTS`. |
| `MEASUREANGLE` | `ANGLE` | The vertex and two points | The angle. |
| `MEASURERADIUS` | `RADIUSOF` | Three points | The radius and centre of the circle through them. |
| `AREA` | `AA` | Points; Enter closes the polygon | The area and perimeter. |
| `ID` | none | A point | Coordinates, with station, offset and level on the nearest alignment or a named one. |
| `MEASURERESULTS` | `RESULTS` | Nothing | The measure results window, with a copy button. |
| `STYLE` | `ST`, `TEXTSTYLE` | The named text styles | Styles stored in the document. |
| `DIMSTYLE` | `DST` | The named dimension styles | Styles stored in the document. |
| `OSNAP` | `OS` | Nothing | Toggles object snap (F3). |
| `OSNAPSETTINGS` | `DSETTINGS`, `SNAPSETTINGS` | Which modes are on: endpoint, midpoint, centre, quadrant, intersection, node, grid | Snap settings. The dialog says what provides each mode. |

**Limits.**

- Text is laid out with Helvetica metrics whatever font a style names. No kerning, tabs or inline formatting. Wrapping breaks at spaces.
- Text on the canvas is the engine's single-stroke preview lettering, not a font.
- Dimension text is plain text. No tolerances, stacked fractions or alternate units. A dimension's text can be overridden in Properties; the measured value stays shown.
- Station and offset dimensions assume site millimetres equal alignment metres times 1000.
- No drafting standard is claimed. Every size is an editable default: paper millimetres times the annotation scale on the status bar.
- Object snap gives: for alignments, the ends, middles and arc centres of their elements, and the intersections of straight elements; for annotations, their definition points; for every other element, where its generated drawing has lines or arcs, the ends and middles of the drawn lines, the ends and middles of arcs, the centres of arcs and circles and the quadrant points of circles (the first 2000 lines and arcs of an element are read, and 400 drawn line segments are kept for intersections over all elements), and otherwise (a drawing of filled shapes and lettering, or none) the corners, edge middles and centre of its bounding rectangle; for DXF underlays, the reader's snap points. Nothing snaps on a hidden layer. Snap works in the annotation commands, when dragging an annotation grip, for the base point and second point of `MOVE` and `COPY`, when dragging a grip of an element and when placing a sign (its centre lands on the point). The element being moved never snaps to itself. A press-and-drag move of the selection snaps to the grid only. The pointer cannot be driven in a test: the function that finds the candidates and the controller's use of a snap point are tested, the wiring of the pointer to them has been looked at in screenshots of the interface only.
- The kernel picks by element bounds only, so a pick inside an annotation's box selects it even between its strokes.
- Annotation is written to DXF (real TEXT; arrows as SOLID). DXF R12 cannot keep a dimension as a dimension.
- Annotation is drawn on plan sheets. It is not drawn on profile, section or detail sheets.
- Layer visibility and lock are not in undo (see 5.2).

### 8.5 Modify

**Purpose.** Edit the selection, and place and support signs. This tab repeats commands from Home, with the sign and support commands grouped together.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `MOVE` | `M` | Base point, second point | The selection moved. |
| `COPY` | `CO`, `CP` | Base point, a point per copy | Copies. |
| `ERASE` | `E`, `DELETE` | The selection, or a pick | Elements removed (undoable). |
| `ARRANGE` | none | Nothing | Every element in rows. |
| `SUPPORTS` | `POSTS` | The Supports dialog (see 5.1) | Posts and foundations with placeholder dimensions. |
| `DESIGN` | `SIZE` | The Design dialog (see 5.1 and 5.9) | A calculation window and the sized supports. |
| `NOSUPPORTS` | none | The selected sign | Its posts and foundations removed. |
| `STATION` | `CHAINAGE` | The Place dialog (see 5.1) | The sign placed on the alignment. |
| `GRID` | none | Nothing | Toggles the grid. |
| `SNAP` | none | Nothing | Toggles snap. |
| `ORTHO` | none | Nothing | Toggles ortho. |

**Limits.** As for 5.1. Supports are sized for wind by the UK method (IHE Sign Structures Guide 2021 with BS EN 12899-1). The Uganda basic wind pressure is an entered value or the MoWT Volume 4 table (5.9).

### 8.6 Signs

**Purpose.** Traffic signs to the active design standard. The tab's buttons are built from the standard's templates, so they change with the standard. The buttons are disabled when the rule pack did not load.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `STACK` | `SIGN` | A click to place (the sign hangs from the cursor) | A stack-type direction sign. |
| `DIRECTION` | `DS` | A click to place | A rectangular direction sign. |
| `CONFIRM` | `RC` | A click to place | A route confirmatory sign. |
| `PLACE` | none | A click to place | A place identification sign. |
| `LANE` | none | A click to place | A lane pre-selection sign. |
| `DIVERSION` | none | A click to place | A temporary diversion sign. |
| `MAP` | `CROSSROADS` | A click to place | A map-type crossroads sign. |
| `TEE` | none | A click to place | A map-type T-junction sign. |
| `ROUNDABOUT` | `RBT` | A click to place | A map-type roundabout sign. |
| `LIBRARY` | `INSERT`, `LIB` | The Standard signs window | A catalogued sign (warning, regulatory, information and destination signs). |
| `VISIBILITY` | `SIGHTLINE`, `SIGNVISIBILITY` | The inputs of the selected sign (tick to store them on every placed sign) | The stored visibility inputs. See 5.8 (Design). |
| `VISIBILITYRUN` | `VISRUN`, `RUNVISIBILITY` | Nothing. Runs the stored inputs of the selected sign, or of every placed sign when none is selected | The results window and the 3D overlay. |
| `UK` | `UKTSM` | Nothing | Switches the active standard to the UK Traffic Signs Manual. The signs are laid out again. |
| `UGANDA` | `UG`, `UGTSM` | Nothing | Switches the active standard to the Uganda Traffic Signs Manual (2004). |
| `QUANTITIES` | `BOQ` | The name of a CSV file (the default is `sign-quantities.csv`) | `<name>.csv` (one row per sign), `<name>.summary.csv` (totals by ground colour), and posts, foundations, supports, drains, culverts, gantries and marking files when the model has them. |
| `SHEETS` | `FABRICATION` | A folder | An A3 fabrication sheet per sign as DXF and as PDF, and the schedule PDF. The status line says how many files were written, and warns of any layout error. |

*Sign properties (Properties, Definition).* Type (read only); Variant (Entry or Exit, only on place identification signs); Design speed (10 to 130 km/h); Approach class (Primary, Secondary or Local); Background (Green, Blue, White, Brown or Yellow); Destinations (name, direction, distance, route number, route status and symbol, added with Add destination); Embedded signs (warning triangles and regulatory roundels on a direction or map-type sign, at most two in the editor); for standard signs, Diagram or symbol, Plate legend (lines separated by `|`), and Temporary, which gives a yellow ground. Changing a sign's standard re-lays its artwork.

**Limits.**

- The UK pack covers the direction, map-type, route, place, lane and diversion signs, and the standard catalogue of warning and regulatory diagrams with plates, supplementary plates, information signs and 66 destination symbols. The sign engine is golden-tested: 444 signs compared with the C# engine, primitive by primitive, worst deviation about 3e-14 sw.
- The Uganda pack has 16 sign kinds, 105 catalogue entries and 241 golden fixtures. Embedded signs under the Uganda pack are not ported.
- Warning triangles and regulatory roundels embedded on direction and map-type signs are drawn as the standard frame only, as in the C# engine; the inner symbol is not drawn.
- DXF export matches the C# writer byte for byte on the reference files. Quantities are byte-identical to the C# output. Fabrication DXF sheets are byte-identical; PDF sheets match object for object apart from dates, ids, the producer name and compression.
- Sign visibility is an analysis aid, not a compliance statement (5.8).

### 8.7 Roads

**Purpose.** The road modules: alignments and their import, markings, drainage, lighting, barriers, pavements, earthworks and the checks of each. Panels in ribbon order are Alignment, Markings, Drainage, Lighting, Restraint, Pavement and Earthworks.

**Dialogs.** Road dialogs take the station shown on the drawings and convert it to the internal station. A station that occurs in more than one region is refused until the region is picked. Each dialog says which station is meant. Every dimension starts empty or as a placeholder, and a placeholder is marked as one.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `LANDXML` | `ALIGNMENT` | A LandXML 1.2 file | Alignment elements. |
| `STATION` | `CHAINAGE` | The Place dialog (see 5.1) | The sign placed on the alignment. |
| `MARKINGS` | `MARKING`, `ROADMARKING` | Alignment; Line type (catalogue entry); Pattern (read only); Speed limit (mph, 5 to 70, with its km/h equivalent); Width (mm); Start and End station; Offset from the alignment (0 to 50 m, Left or Right); Material (free text); Speed band (take the band, or keep the exact value). Lay the line; the dialog stays open | A longitudinal marking element, with its plan and 3D line and its quantities. |
| `MARKTRANSVERSE` | `STOPLINE`, `GIVEWAY` | Basis (UK Chapter 5 2018 or Uganda 2004); the family: stop line, give way line, give way triangle or pedestrian crossing; its size and placement as the dialog asks | A transverse marking. Where the Uganda manual defines nothing, "use the UK value" asks for a reason that is stored with the marking. |
| `MARKARROW` | `ARROWS` | Basis; lane arrow or deflection arrow; size; station | An arrow. UK arrows are placeholder blocks: "placeholder block: not the true shape; area is the block, not paint". |
| `MARKHATCH` | `HATCHING`, `CHEVRONS` | Basis; hatched marking, chevron marking, island marking or zig-zag line | Hatching or chevrons along the road. |
| `MARKYELLOW` | `YELLOWLINE`, `WAITING` | Basis; waiting restriction or dividing line | A yellow line. |
| `MARKSTUDS` | `STUDS`, `ROADSTUDS` | Basis; the line along which the studs lie; a row of the manual's stud tables | Road studs at the table spacing. |
| `MARKWORD` | `WORDED`, `SLOWMARK` | Basis; the word (for example STOP or SLOW) | A worded marking as a placeholder block, not the true letters. |
| `MARKLINES2` | `UGLINES` | Uganda lane, edge or double line, and the parking-bay family in the same dialog | The Uganda marking. |
| `DRAIN` | `SIDEDRAIN` | Alignment; Start station and End station; Offset from the alignment (m), Left or Right | A side drain with placeholder dimensions to edit in Properties. |
| `CULVERT` | none | Alignment; Station (m). The culvert crosses the alignment at that station | A pipe culvert with placeholder dimensions. |
| `RATIONAL` | `RUNOFF` | The parts of the catchment (description, area, runoff coefficient C), urban parts (C2 factor, within the printed range), time of concentration by reach, rainfall (always an input), area reduction factor, and the catchment kind where the basis asks | The peak flow, with its calculation. With a drain or culvert selected, the result can be kept on it. |
| `CHECK` | `VERIFY` | Depends on the element: a side drain (design flow, basis); a culvert (design flow, basis, Manning's n, entrance loss coefficient Ke, inlet edge from HDS-5 Table A.1, bed slope reading, barrel friction, road class, category); a street lighting installation (appraisal, speeds, set-back and its source, maintenance factors, maintained results from a calculation made elsewhere, the class selection parameters); a barrier run (speed limit); a pavement (its design method) | A calculation window. The inputs are kept on the element, so `CHECKALL` can repeat the check. |
| `LIGHTING` | `LIGHTS`, `STREETLIGHTING` | Reference (alignment); Arrangement; First column on; Spacing; At the end; Carriageway width; Carriageway centre offset; Set-back from the edge; Lighting class (a label you enter); the column: Mounting height, Bracket outreach, Tilt, Luminaire product, Lumen output, Power | A lighting installation, laid out by station and drawn as a schematic column, bracket and luminaire block. |
| `LIGHTCOLUMN` | `COLUMN` | Mark; Offset from the alignment; Bracket points; Lighting class; the column fields | One lighting column at a station and offset. |
| `PHOTOMETRY` | `IES`, `LDT`, `LUMINAIREFILE` | An IES or LDT file; the installation; plane pairs; the C-plane facing the carriageway and whether the orientation is confirmed; vertical illuminance (optional); edge illuminance ratio (optional, with strip width); an r-table file for luminance; an observer (optional); the grid: offsets, points along and across; the area for drawings and class; contour levels | Results per area: average, minimum, maximum, uniformity, vertical and edge results, and luminance when calculated. Contours and grid values in the 3D view and on sheets. CSV. |
| `BARRIER` | `RESTRAINT` | Mark; Traffic face at; Traffic on the; Containment level; Working width class; Impact severity level; Beam height; Post spacing; Set-back; System or product; end treatments | A safety barrier run with plan, schematic solids, quantities. |
| `PAVEMENT` | `PAVE` | Mark; carriageway width; cross-fall; shape; layers, top first; optional shoulders | A pavement with its cross-section, slabs at the profile level and layer quantities. |
| `PAVEDESIGN` | `PAVEMENTDESIGN` | Method: MoWT Volume 3 Part I, UK DMRB, AASHTO 1993, or South African SAPEM Chapter 10. Each method's own form (see 5.7 Limits) | The calculation lines and warnings. For the MoWT, DMRB and AASHTO methods the result can make a pavement from its layers. SAPEM returns no layers. |
| `SURFACE` | `TIN`, `TERRAIN` | A LandXML file with a TIN | A ground surface element (also under Insert). |
| `SURVEY` | `SURVEYPOINTS`, `POINTFILE`, `TOPO` | A file of survey points; see Insert (5.2) | A ground surface element triangulated from the points (also under Insert). |
| `CORRIDOR` | none | Ground surface; Name; Lane width (each side); Lane cross-fall; Formation under the shoulder; Formation depth below paved links; Cut slope and Fill slope (horizontal for 1 vertical); Section interval; Stretch. A "Fill from MoWT class" button fills lane and shoulder widths and cross-fall, with the citation shown | A corridor: a typical section along the alignment, cut and filled against the surface. |
| `SECTIONS` | `XSECTIONS` | The selected corridor; a station; vertical exaggeration | The cross-section window, station by station. Labels are set beside the drawing, not written to DXF. |

*The MoWT, DMRB, AASHTO and SAPEM forms (`PAVEDESIGN`).* MoWT: traffic in one direction (commercial vehicles per day), growth rate, design period, road (paved width, narrow road factor), design subgrade CBR, climate, drainage, maintenance, base and subbase. DMRB: commercial vehicle flows at opening, scheme, design period, lanes in one direction, past traffic, foundation class (from CD 225), flexible pavement. AASHTO: design lane 18-kip ESALs (W18), reliability, overall standard deviation (S0), serviceability loss (delta PSI), effective roadbed modulus (MR), and the structure to check, top first. SAPEM: road category, traffic loading (daily E80 per design lane, or heavy vehicles and E80 per heavy vehicle), growth, years to opening, design period, climatic class and subgrade CBR. Every field starts empty.

**Limits.**

- *Markings:* horizontal geometry only; a marking has one line type, width and offset from end to end. Tapers and lines that move across the carriageway are not made. The catalogue checks a marking against the speed limit and the manual's tables; it does not decide whether a marking is warranted (the visibility distances of Table 2-4, the double white line procedure of 3.9, and junction layouts are the engineer's). The laying tolerances of Table 12-2 are not applied. Not done: raised profile edge lines (1012.2, 1012.3), 1009B, double white lines version B and the change-over version C, 1013.5, splayed and inclined double lines; for the second set, worded markings beyond placeholder blocks, lane destinations (1035), bifurcation arrows, hatching 1040.3 to 1040.5, tapers, the mini-roundabout markings, yellow box junctions, loading restriction and red route lines, clearways, and the Uganda curved arrows (M42 to M45), M35, M51, M52, M63, M71 and the tapers of M53, which are catalogued or drawn as blocks. Rates and costs are not held. The display solids of every marking, first and second set, sit at the profile level of the road at their stations (the 5 mm thickness and the level are for display only and enter no quantity); before round 4 the solids of the second set's polygon marks (stop lines, arrows, the triangle, the diagonals of hatching and chevrons, studs) stood at level zero and appeared away from the pavement.
- *Side drains and culverts:* uniform flow only; no backwater, drawdown or bends. Rainfall intensity is always an input under every basis. Manning's n is always an input. The culvert check is the complete MoWT check (inlet control by HDS-5 equations, outlet control, the greater of the two). Inlet control covers circular concrete and corrugated pipe and concrete boxes with the inlet edges of HDS-5 Table A.1; skewed, offset, tapered, embedded, arch and pipe-arch inlets are not implemented. The culvert check is one part of a culvert design, not a certificate.
- *Gantries:* see the Structures tab for the gantry limits.
- *Lighting:* stations must lie on one alignment; the carriageway is symmetrical with one width over an installation; one column and one luminaire per installation. Columns are not checked against other elements. The lighting class is a label the engineer enters. Photometry is not an EN 13201-3 calculation and is not validated against DIALux, Relux or AGi32 (section 12).
- *Restraint:* the check does not perform or replace the Road Restraint Risk Assessment Process. Lengths and any higher containment level from that assessment are inputs. Terminal classes T80/3, T110 and T110/1 are names only, because PD CEN/TS 1317-7 is not held. Not covered: vehicle parapet design, crash cushions, pedestrian restraint, temporary barriers, a tapered or flared offset (a run has one offset), product catalogues and costs.
- *Pavement design:* the three methods are not interchangeable. The MoWT method is the one the crate's author assumed to govern; that is an assumption, not a project instruction. Not covered: rigid pavement, gravel wearing course, overlays, mechanistic analysis, surfacing design, materials selection, drainage layer design, superelevation, widening on curves, tapers and side slopes. SAPEM returns no layers because the TRH4 catalogue could not be read. The DMRB method uses superseded revisions (section 10).
- *Ground surfaces and corridors:* every width, slope, depth and interval is the engineer's. The crate holds no design value, and no bulking or shrinkage factor is applied. Expected values in its tests were worked by hand, not from Civil 3D: compare a real job against a package you trust before using the volumes for payment or a bill of quantities. Sections are straight lines square to the alignment and can cross on the inside of tight curves. The daylight is the first meeting of slope and ground. Template changes are steps. A surface of 100 000 points adds about 6.4 MB to the file.

### 8.8 Design

**Purpose.** Alignment design from PIs, the profile editor, design checks against a limits file, the model-wide check, sign visibility and alignment options. The Design tab uses `afrobim-design`, which holds no value from any standard. Limits come from the built-in MoWT basis or from a file you load.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `ALIGNDESIGN` | `NEWALIGNMENT`, `DESIGNALIGNMENT` | The name of the alignment and the start station (typed as `1+250.000` or as a number of metres, 1250). Then PIs picked on the plan (object snap to PIs and alignment ends; SNAP and ORTHO apply) or typed (`x,y`, `@dx,dy`, `@d<a`) | A designed alignment: an `alignment` element and an `alignment_design` element. A curve starts with no radius and no spiral. |
| `ALIGNEDIT` | `PLANEDIT`, `PIEDIT` | Nothing. Drag a PI grip; Insert PI on a tangent; Delete PI | The alignment changed in place. A move the engine refuses shows its reason on the status line and in red at the cursor, and the PI goes back. |
| `PITABLE` | `PIS`, `CURVETABLE` | Nothing. The table: station, east and north, deflection, radius (with the feasible radius range), spiral in and out, tangent and curve length | Edits to the PI values. Blank radius shows "radius needed". |
| `ALIGNLABELS` | `CURVELABELS` | Toggles the curve data labels | The labels on the plan. |
| `PROFILEDESIGN` | `PROFILEEDIT`, `PVI` | The profile editor: the ground line from a survey surface; PVIs added by click and dragged; the PVI table (station, level, grade in and out, curve by length or by K, high or low point); vertical exaggeration (blank fits the window); Fit to ground | The vertical profile. Fit to ground proposes a profile by a least-squares fit of straight segments; it is labelled STARTING POINT ONLY and inserts nothing until you accept. |
| `ALIGNCHECK` | `DESIGNCHECK`, `GEOMCHECK` | The limits source: the built-in MoWT Road Design Manual Volume I (January 2010), with class, terrain, design speed and emax pickers and the desirable or absolute maximum gradient; or your own limits file (CSV or JSON); or the built-in row with your file laid over it. The design speed row. The minimum grade in cut check | Checks per element, with status and the source of each limit. Values and limits that are lengths are shown in the document's display unit (see `UNITS`); the source of a limit keeps the unit its table prints, and the message is the engine's own text. The limits source is stored on the alignment (`check.alignment`), and `CHECKALL` uses it. |
| `CHECKALL` | `CHECKMODEL`, `CALCPACK` | Nothing. Filters by status and category, select and zoom, Run again | One report window: a status per check of each element, the calculation lines, CSV, and a plain-text calculation pack. |
| `VISIBILITY` | `SIGHTLINE`, `SIGNVISIBILITY` | The inputs for the selected sign or all signs (see below) | The stored inputs. Not run until confirmed. |
| `VISIBILITYRUN` | `VISRUN`, `RUNVISIBILITY` | Nothing | The results window: each sign's station, overall result, clear distance and limiting obstruction, the check lines with citations, the plan envelope and sight-line profile drawings, and the 3D overlay of the limiting sight line. |
| `OPTIONS` | `ALIGNOPTIONS`, `OPTIONSET` | An option set. Add the current design as an option (name it), rename, duplicate, delete, Make this option current. Scoring criteria and weights you enter (none prefilled) | The comparison of options on figures worked out from the model (length, earthworks, and so on, each with how it was computed), a weighted score when you enter weights, and CSV. |
| `LANDXMLOUT` | `EXPORTLANDXML` | A file name | LandXML 1.2 of the designed alignment and its profile. |

*Sign visibility inputs (`VISIBILITY`).* The driver's lateral position; the driver's eye height (1.05 m, shown as a value to confirm, and accepted by ticking); traffic keeps to the left or right; the direction of travel; the approach speed (85th percentile) and the UK speed band, with an override that is recorded; the large-figure option for 70 mph dual carriageways and motorways; the UK provision (terminal limits in mph, and the set-back case: over a footway or overhead, kerbed road, hazard or junction at a station); the manuals to check (UK, Uganda). Every input starts empty except the eye height, which is marked as a default to confirm.

*Limits.*

- Design checks cover the geometric checks the engine has. Not checked by `ALIGNCHECK`: lateral sight distance on horizontal curves, passing and decision sight distance, superelevation rates and runoff lengths, combinations of horizontal and vertical alignment, and cross-section elements.
- Vertical curves are parabolas by length or K. Asymmetric and circular vertical curves are not offered by the design, though the alignment crate and LandXML read them. A PI with spirals needs the spirals to turn less than the deflection. Compound curves are not made from PIs.
- Fit to ground is a greedy fit, not an optimisation, and is a starting point only.
- An imported alignment cannot be made editable. The alignment engine holds it as elements (lines, arcs, spirals); the design engine works from PIs with radii, and no faithful conversion exists. The ribbon shows that tool as a disabled "Planned" button (section 13).
- The plan editor draws in site metres on its own view, not on the millimetre canvas. Typed coordinates are metres.
- Land take is a plan footprint, not a cadastral take. Earthworks are average end area, with no prismoidal correction. Crossings are plan intersections with the other alignments of the model, with no levels.
- LandXML export is checked against the schema and AfroBIM's own importer. It has not been read by Civil 3D. Superelevation, existing ground and cross-sections are not written.
- Visibility: the road surface is the centreline level of the profile. Cross-fall and superelevation are not applied to the eye, the road or the mounting height. Obstruction is geometric only (no vegetation, vehicles, night or glare). The lateral obstruction is an offset line from the centreline. The UK speed band, the hazard station, the lane and the provision that applies are the engineer's. The check lines compare and cite; they do not decide.
- `OPTIONS` figures are computed from the model; the scoring weights are yours.

### 8.9 Structures

**Purpose.** Posts and foundations for signs, sign gantries and their checks.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `SUPPORTS` | `POSTS` | The Supports dialog (see 5.1) | Posts and foundations with placeholder dimensions. |
| `DESIGN` | `SIZE` | The Design dialog (see 5.1) | The wind calculation and sized supports. |
| `NOSUPPORTS` | none | The selected sign | Its supports removed. |
| `GANTRY` | `PORTAL` | Alignment; Station (m) | A portal gantry at the station, with placeholder dimensions to edit. |
| `CANTILEVER` | none | Alignment; Station (m) | A cantilever gantry at the station, with placeholder dimensions to edit. |
| `HANG` | `GANTRYSIGN` | Gantry; Position (an Offset from the centreline in metres, or an Over lane number from 1 to 12); Faces traffic travelling (Up-station or Down-station, when the position is an offset); Lower edge above the boom underside (mm; negative hangs below the boom) | The selected sign hung on the gantry, which it then follows. |
| `UNHANG` | none | The selected sign | The sign taken off its gantry. |
| `CHECK` | `VERIFY` | The selected gantry | The headroom and requirement calculation. The serviceability deflection is entered in Properties; without it the headroom line reads INCOMPLETE. |

*Standards for the gantry checks.* CD 127 Cross-sections and headrooms, Version 1.0.1 (29 July 2021), for headroom (Table 4.1) and sag compensation (Table 4.3). CD 365 Portal and cantilever signs/signals gantries, Revision 1 (March 2020), which is watermarked SUPERSEDED: the current version was not obtained. CD 377 was read only and no value is taken from it, because its copy held is also watermarked SUPERSEDED.

*What the gantry checks cover.* Headroom: the lowest point of boom, carried signs and equipment against the highest surface under the gantry, less the deflection, measured at right angles to the surface, against the standard headroom plus the sag compensation. The report reads OK, FAILS or INCOMPLETE. The distance from the edge of the paved width to each leg's near and far face is reported without a pass or fail. For a dual carriageway with a non-passively-safe portal, no leg between the carriageways unless the Overseeing Organisation agrees (CD 365 Revision 1, clause 3.21). The beams end at the same level for a portal. A clear distance to an over-bridge of at least twice its maximum height (clause 3.23). Listed for the engineer and not checked: technical approval, Eurocode design and fatigue, type selection, working life, vehicle restraint, set-back of verge supports, maintenance, permanent access, and deformation limits.

**Limits.**

- Not a structural design. Passing the checks does not show the gantry, its connections or foundations are adequate. Legs, boom and foundations are schematic envelopes of entered dimensions. A gantry needs Eurocode analysis, fatigue assessment and technical approval.
- A straight gantry line through one station. Levels come from the centreline profile; cross-fall and verge levels are not modelled.
- One boom level; no pre-camber, inclined or stepped booms. Two legs or one; no twin portals, double cantilevers or legs in pairs.
- Sign faces are bounding rectangles. The elevation carries no text or dimension figures.
- Not checked because no applicable value is held: a limit on span or outreach, and a minimum set-back of legs from the carriageway.
- `afrobim-supports` does not know about gantries and will add posts to a gantry-hosted sign if asked. `support_conflicts` finds such signs.
- Not done: structural analysis and member design; wind, collision and other actions; deflections; fatigue; foundation design; passive safety; variable message signs and signal equipment and their loads; walkways and handrails; electrical works; costs.

### 8.10 Coordination

**Purpose.** Clash detection between elements of the model, issues raised from clashes or by hand, BCF 2.1 exchange and IFC reference models.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `CLASHTESTS` | `CLASHSETUP` | A test: name; type (hard or clearance); selection A and selection B by category and layer (nothing ticked means all); Tolerance (mm, your requirement; no default); Touching (faces that only touch count); Host chain (ignore pairs where one element hosts the other) | The clash tests, kept in the document. |
| `CLASH` | `CLASHRUN`, `CLASHDETECT` | Nothing | Runs every test and merges the clashes into the stored results. A clash seen again keeps its status, comment and assignee. |
| `CLASHES` | `CLASHRESULTS` | Nothing. Filters by test and status; editable status, assignee and comment; zoom to clash | The results table: id, test, both marks, kind, depth or distance, status (new, active, reviewed, approved, resolved), assignee and comment. |
| `CLASHMARKERS` | none | Nothing | Toggles the markers in the 3D view. |
| `CLASHCSV` | none | A file name | The clash results as CSV. |
| `ISSUES` | `ISSUELIST`, `TOPICS` | Your name; a title; issues raised from a clash, or a group of clashes (Group clashes within, in mm, yours); Status; Priority; Type; Assignee; Due date (YYYY-MM-DD); Labels; comments; viewpoints | Issues kept in the document. A viewpoint is the 3D camera with a PNG snapshot, restored with "Go to viewpoint". |
| `BCFOUT` | `EXPORTBCF` | A file (`.bcf` or `.bcfzip`) | BCF 2.1 with the issues. |
| `BCFIN` | `IMPORTBCF` | A BCF 2.1 file | Issues read from it, with a report of what could not be carried. |
| `IFCREF` | `REFERENCEMODEL`, `IMPORTIFC` | IFC files (see 5.3) | Reference models, which clash tests can select by model and by entity type. |
| `REFMODELS` | `REFERENCES`, `REFLIST` | The reference models window (see 5.3) | Nothing new. |

*How a clash is found.* Every element that has solids becomes a body in world millimetres, moved with the transform the 3D view draws by. Hard tests flag overlaps and faces that touch when touching counts. Clearance tests flag pairs closer than your tolerance. Corridor and ground surface meshes are included.

**Limits.**

- Round solids are faceted. A solid of n flat sides lies inside the true round one, so measured distances are larger than true by at most the sagitta. For the 32-sided posts of the sign modules that is 0.48 % of the radius, 0.24 mm at 50 mm radius. Hard overlaps shallower than that are missed.
- Open meshes (terrain and surfaces) get intersection and distance tests only. Containment is never reported for them.
- Reversed winding breaks the touching and overlapping decision for coplanar faces. The site-solid meshes of lighting, restraint, pavement and corridors have windings the crate relies on, so a flush contact on those may read as overlap or contact either way.
- Identical coincident solids are not a hard clash unless touching counts.
- Hard depth is an estimate, not a minimum translation distance.
- Saved viewpoints are data; the crate renders nothing, so the app supplies the camera and the snapshot. BCF exports have been read back and checked against the BCF 2.1 schemas, but not opened in any other BCF tool. BCF 3.0 is refused.
- Not done: other disciplines' IFC in clash tests beyond reference models, time-based (4D) clashes, rule sets from templates, penetration depth.
- For a large model, sweep and prune slows where an element's box spans the whole model (a long pavement layer).

### 8.11 4D

**Purpose.** The construction programme: activities, logic, calendars, the critical path, the Gantt chart, the baseline, progress, the links from elements to activities, the 4D date and playback, and exchange with MS Project and Primavera P6. Nothing is assumed: there are no default durations, productivities or holidays.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `PROGRAMME` | `ACTIVITIES`, `PROG` | The first time: Programme name, Project start date (YYYY-MM-DD), Calendar name, working weekdays (none ticked for you). Then the activity table: id, WBS, name, working days (0 is a milestone), milestone kind and constraints; dependencies FS, SS, FF and SF with lags (blank is 0); several constraints per activity (start no earlier than, finish no later than, must start on) | The programme element. |
| `WBS` | `SUMMARYTASKS`, `ROLLUPWBS` | Summary tasks with an id, WBS code and name; a parent picker; Indent and Outdent | Summary tasks; the roll-up on the basis you choose (duration or weight), with its formula shown. |
| `CALENDAR` | `WORKDAYS` | The working weekdays and the days off you enter; more calendars can be added | The programme calendar. No holiday is built in. |
| `SCHEDULE` | `CPM` | Nothing | The critical path method: early and late dates, total and free float in working days, the critical flag, and findings in plain words (open starts and ends, negative float, constraint conflicts). |
| `CRITICAL` | `CRITICALBASIS`, `LONGESTPATH` | Total float of zero or less (the default), or the longest path | The critical basis. The Gantt chart, the table and the time-location chart follow it. |
| `GANTT` | none | Nothing | The Gantt chart: critical bars red, others blue, finished grey, progress as a strip, milestones as diamonds, dependency lines, baseline bars, and the data date line. Summary tasks fold. |
| `TIMELOCATION` | none | The alignment to chart (one per call); time runs up the page (option); location ranges in stations; direction and dwell | A time-location chart. Critical activities are marked; data date and baseline are drawn. |
| `BASELINE` | none | A name for the baseline | The current dates, durations and float kept under the name. |
| `VARIANCE` | none | Nothing | The current minus baseline for start, finish, duration and float, with added and removed activities and the project finish variance. |
| `PROGRESS` | `DATADATE` | The data date; per activity, the percent complete and actual start and finish | The programme rescheduled. Work done keeps its actual dates; the work left is scheduled from the next working day. |
| `LINK4D` | `LINKACTIVITY` | Selection, or a rule (category, layer, mark pattern with `*` and `?`); the activity; the action: construct, demolish or temporary | Links from model elements to activities. |
| `LINKREPORT` | none | Nothing | Elements no activity reaches, and activities with no elements. |
| `FOURD` | `DATE4D` | Nothing. The date control: a slider, a date field and day and week steps; Colour the 3D view by this date; hide elements not started (otherwise ghosted); the colour by | The 3D view coloured by state at the end of the chosen day: not started (ghosted or hidden), in progress (amber), complete (normal), removed or temporary. |
| `PLAY4D` | none | Play and pause; speed (days of the programme per second of viewing, a viewing preference); a scrub bar | The date shown large in the 3D view, with the element states redrawn on the dates something changes. |
| `MSPIN` | `IMPORTMSP` | An MS Project XML file | The programme replaced by the import (undo restores it), with a report of what the file could not carry. |
| `MSPOUT` | `EXPORTMSP` | A file name | MS Project XML (MSPDI). |
| `PROGCSV` | `PROGRAMMEREPORTS` | A folder | The activity table, the variance and the S-curve as CSV. The S-curve weighs each activity by its working days. |
| `XERIN` | `IMPORTXER`, `P6IN` | A Primavera P6 XER file | The programme replaced by the import, with a report of what the file could not carry; links to model elements are kept where the activity id survives. |
| `XEROUT` | `EXPORTXER`, `P6OUT` | A file name | A P6 XER file, and a report of what the file does not carry. |

**Limits.**

- Resources, productivities, levelling and multiple baselines are not done. The S-curve weight is the working day count, not cost.
- Constraint kinds: the engine's three kinds only (start no earlier than, finish no later than, must start on). Multiple baselines, activity splitting and expected-finish dates are not done.
- Playback is a static state at each date, with the elements ghosted, amber, normal or hidden; there is no animation inside an activity.
- MS Project XML: summary and null tasks, other constraint types, elapsed lags and calendar work weeks are reported as lost. Not tested in MS Project.
- Primavera XER: resources, costs, activity codes, user fields, several projects per file, and calendars with exceptions are not carried. The files have not been opened in P6. The P6 column names follow the Oracle data map and the open-source readers named in the schedule README; the sample files of the tests were read, not P6 itself.
- The time-location chart plots one alignment per call and uses internal stations. Labels are drawn by the app.
- The choices of method (critical basis, roll-up basis, chart alignment) are properties of the programme element and are not in any exchange file.

### 8.12 5D

**Purpose.** The bill of quantities, rates, currency, the estimate, cash flow, the interim payment certificate and the cost reports. The tab uses `afrobim-cost`. **No rate, price, exchange rate, percentage, item number or standard bill is built in.**

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `BILL` | `BOQSETUP` | Structure: a structure CSV (`kind,code,section,description,unit`), or numbered sections you title, or typed sections and items. Mapping rules (load or save a CSV): a category, a trailing `*` for a prefix, and conditions `key=value`, `key=a\|b`, `key~text`, `key>=n`, `key<=n`, `key?`, separated by `;`. Manual items with a quantity you enter and a reason | The bill and its mapping. Measured quantities are shown with drill-down to the contributing elements (click selects the element). Unmapped lines and ambiguous lines are listed with totals by category. A warning flags an element measured by lines of different kinds that may be priced twice. |
| `RATES` | `RATELIBRARY` | A rate library CSV (`item_code,description,unit,rate,currency,source_note,date`), or each rate typed with its currency, source note and date | The rate library. Items with no rate are listed as unpriced, never priced at zero. |
| `CURRENCY` | `EXCHANGE` | The primary currency label and decimals (UGX with 0 decimals is the starting label); an optional secondary currency; exchange rates (1 FROM = RATE TO, each dated, with a source note); an optional as-of date | The currency settings. A rate converts directly or by reciprocal, never through a third currency. |
| `ESTIMATE` | `PRICE` | Nothing. Tabs for the priced bill, additions, audit trail and comparison | The priced bill with section totals, a secondary-currency column, additions in list order, the grand total, banners for unpriced items, unmapped lines and possible double counting, and comparison with a saved snapshot or another estimate. |
| `CASHFLOW` | `SCURVE` | A window (start and finish) per item or section, by hand, or taken from the linked activities of the programme | A monthly table and an S-curve. |
| `CERTIFICATE` | `IPC`, `INTERIM` | Payment rules you enter, in the order applied (retention with an optional cap, advance recovery, percent of gross, lump); the quantities to date, typed or taken from the site records at a date; the previous certificate | An interim payment certificate. |
| `COSTCSV` | `COSTREPORTS` | A folder | Every CSV the crate offers: quantity lines, bill structure, mapping rules, rates, the priced and unpriced bill, summary, audit trail, unmapped lines, cash flow, comparison and certificate. |

**Limits.**

- No rates, exchange rates, percentages, item numbers or standard bill are held. The MoWT General Specifications numbering is not offered: its contents list was not read from an official source. `Bill::neutral` gives numbered sections to rename.
- The certificate encodes no FIDIC clause and no other contract's terms. Every percentage, cap, threshold and amount is your contract data, and AfroBIM only does the arithmetic. It does not decide what a contract entitles anyone to.
- Quantities are only those the collectors list. One collected line per quantity per element, and a line cannot be split between two items. Mapping is first-match, with no weighting or percentage split.
- Post mass uses the supports crate's nominal steel density.
- Cash flow is linear by calendar day, by month. No weeks or working-day calendars. Non-linear distributions are not done.
- Exchange rates convert directly or by reciprocal only.
- Not done: price adjustment and escalation, variations, dayworks, claims, final account, xlsx output (CSV only).

### 8.13 Construction

**Purpose.** Site progress records, roll-up to the programme, earned value, look-ahead and delays, the progress colour in the 3D view, and the earned value tables. The tab uses `afrobim-progress`. No rate, percentage, weight, threshold or formula is built in, and no actual cost is derived.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `SITEREC` | `SITERECORD` | Elements (the selection, or a rule by category, layer and mark pattern); what was measured: Percent complete, or Quantity done (with a target quantity in the same unit); Date of the measurement; Recorded by; Note; Files and photos (paths only); a correction, with its reason | A site record for each element. Corrections keep the earlier record. |
| `SITELOG` | `SITERECORDS` | Filters by element and date | The records table, with CSV export through `EVMCSV`. |
| `ROLLUP` | `PROGRESSUPDATE` | The data date. The activities with their percent, weight basis (priced value, or equal, with the reason), number of elements with records, and inferred actual dates marked inferred | Applies the inferred actual dates and the percent to the programme, and reschedules, when you accept. |
| `EVM` | `EARNEDVALUE` | Data date; the budget (from the estimate, or per activity); actual costs (date, activity or project, amount; until entered, cost figures read input needed); an EAC formula chosen from a list (none is chosen for you); CPI and SPI weights; your estimate of the remaining work (ETC) | Budget at completion, planned and earned value, actual cost, EAC, TCPI to EAC, per-activity and monthly tables, and an S-curve. |
| `LOOKAHEAD` | `DELAYS`, `DELAYLIST` | Days from the data date | The activities due to start or finish in those days, and the delay list (activities finishing later than their baseline). |
| `SITEVIEW` | `PROGRESSCOLOUR` | Nothing; also a radio in the 4D date window | The 3D view coloured by recorded percent at the 4D date (grey with no record, amber at the start to green at 100 %), with a legend. Switches back to the programme's state. |
| `FOURD` | `DATE4D` | See 5.11 | As in 5.11. |
| `CERTIFICATE` | `IPC`, `INTERIM` | See 5.12. "Take quantities from site records at date" fills quantities from each element's quantity times its recorded percent | As in 5.12. Lists recorded elements that no bill item measures. |
| `EVMCSV` | `CONSTRUCTIONCSV` | A folder | Every earned value table as CSV: the figures, the activity table, the periods, the S-curve, the look-ahead and delay tables, and the site records. |

**Limits.**

- Activities with no linked element have no progress source; their earned value is zero and they are flagged. There are no activity-level or resource-based progress records.
- An element reached by more than one activity counts its progress in each; its priced value cannot be split, so the budgets must be entered.
- Planned value is linear in programme-calendar working days between baseline dates. One reporting currency; reporting by calendar month only.
- Actual cost is entered by hand. There is no import from accounts, no accruals and no escalation.
- The earned value formulae (EAC, TCPI) are the ones written in the crate's README. No standard was held or quoted: PMI and PMBOK were not available. If your contract defines a quantity differently, check the README against that definition before using the figure.
- Inferred actual dates are inferences from records. Replace them in the programme by hand if the site diary says otherwise; re-applying updates overwrites them.
- Photos and files are paths only; they are not opened, stored or checked.
- Not done: forecast S-curves, claims, extension of time analysis, variation valuation, and non-conformance and inspection records.

### 8.14 Output

**Purpose.** Exports (DXF, IFC, quantities, fabrication sheets), drawing sheets (sheet sets, preview, title block, PDF and DXF), the calculation report pack, the sign schedule out and template, and saving.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `DXFOUT` | `EXPORT` | A file name | One R12 DXF file in millimetres of the selection, or of everything when nothing is selected. Elements with no geometry are left out. Sign drawings are on one layer per colour. |
| `IFCOUT` | `IFC` | The IFC export dialog: alignment mode Simple (the default), Geometry or Linear; a file name (default `model.ifc`) | An IFC 4.3 file (schema IFC4X3_ADD2) of the whole model. The choice of alignment mode is kept in the document. |
| `QUANTITIES` | `BOQ` | See 5.6 | The quantity CSVs. |
| `SHEETS` | `FABRICATION` | See 5.6 | The fabrication sheets and the schedule PDF. |
| `SHEETSET` | `DRAWINGSHEETS`, `SHEETMANAGER` | A sheet set (name; New set); its numbering (prefix, digits, start, step); sheets added from templates: plan series (alignment, station range, sheet length, overlap, scale 1:n, rotation, tick and label intervals), profile (station range, horizontal and vertical scales, datum, grid, data band, optional ground surface), cross-sections (corridor, range, interval or list), schedule (sign schedule, a module's quantities, the priced bill, the activity table, clash results, the model-wide check summary), detail (a sign face, the Gantt chart, the programme S-curve, the cost S-curve), notes; each sheet's paper, orientation, margins, frame and title | The sheet set, stored in the document. Table and chart contents are read from the document every time the set is composed. |
| `SHEETPREVIEW` | `PREVIEWSHEETS` | The set to compose; Previous, Next, a list of sheets, Fit, Zoom in and out | The composed sheets, with the composer's warnings. |
| `TITLEBLOCK` | `TBLOCK` | Placement (a column down the right, or a block at the lower right) and width; blocks of rows and cells (a field, fixed text or nothing, with its width); the revision table; the logo box; load or save as JSON | The title block layout, stored in the document. |
| `PROJECTINFO` | `PROJECT` | Project, client, contract number, drawn, checked and approved, issue date, logo caption, revisions (rev, date, description, by, checked) and other fields. Every field starts empty | Project information used by the title blocks and the report pack. |
| `SHEETPDF` | `PLOTSHEETS` | A file name | One PDF of the whole set, a page per sheet, with real text. |
| `SHEETDXF` | `SHEETSDXF` | A folder | One R12 DXF per sheet, named by sheet number, at 1 drawing unit to a millimetre of paper. |
| `REPORTPACK` | `CALCREPORT`, `REPORTS` | The report pack window, which composes the PDF, text and CSV variants | One A4 PDF with a cover, contents, design basis (with every superseded-edition and secondary-source warning), a section per check area, the clash, quantity, estimate and programme summaries; sections with no data are listed as not included. Plain-text and CSV variants. |
| `SIGNSCHEDULEOUT` | `SIGNSCHEDOUT` | A file name | The model's signs in the sign schedule CSV format. |
| `SIGNSCHEDULETEMPLATE` | `SIGNSCHEDBLANK` | A file name | A blank template. Its example line starts with `#`, so an import skips it. |
| `SAVE` | `QSAVE` | Nothing | Saves the document. |
| `SAVEAS` | none | A file name | Saves the document under the new name. |
| `QUIT` | `EXIT` | Nothing | Closes AfroBIM. |

**Limits.**

- *IFC 4.3, markings:* a road marking (first or second set) that a pavement of the document carries (same alignment, covering its stations and its offset) is an `IfcSurfaceFeature` adhering to the pavement (`IfcRelAdheresToElement`), not contained in the road. The predefined type follows the kind of marking: lines, transverse lines, the zig-zag line and crossings of two lines are `LINEMARKING`; arrows, the give way triangle and worded markings are `SYMBOLMARKING`; hatching, chevrons and island markings are `HATCHMARKING`; crossings of stripes are `PAVEMENTSURFACEMARKING`; road studs are `USERDEFINED` with `ObjectType` `ROAD_STUD`, because the IFC 4.3 enumeration has no enumerator for a stud (this one is a judgement, not a documented mapping). The enumerators were read from the schema and the documentation text shipped with IfcOpenShell 0.9.0.post1 and the buildingSMART IFC4.3.x-development repository; details are in the `afrobim-ifc` README. A marking no pavement carries is an `IfcBuildingElementProxy` in the road (an `IfcSurfaceFeature` must adhere to an element). A placeholder block (UK arrows and lettering, the Uganda curved arrows) is written as its block and carries the properties `PlaceholderBlock` (true) and `ShapeStatus` ("placeholder block, not its true shape"). Standard property sets are written only where the IFC 4.3 template applies to the type (`Pset_RoadMarkingCommon`, `Qto_SurfaceFeatureBaseQuantities`). `Pset_MarkingLinesCommon` (`DashedLine`, `DashedLinePattern`, `NominalWidth`, names and value types checked against the IFC4X3 templates shipped with IfcOpenShell) is written for a second-set marking only when it is a `LINEMARKING` feature whose dash pattern is one pattern for the whole element: a longitudinal line or double line of two equal patterns, and a stop or give way line. It is left out, with a note in the export notes, for the zig-zag line, a crossing with side lines, a double line of two different patterns, and a type with no layout. A document with a pavement and one marking of every kind, UK and Uganda, was checked with IfcOpenShell: 0 schema and rule issues. The solids of a marking sit at the profile level of the road at their station.
- *IFC 4.3:* not exported: linear placement and alignment curve geometry in Simple mode (Geometry and Linear modes write them, see below); cant; child alignments; georeferencing; fixings, reinforcement and excavation; material grades and profile sets; structural design results; type objects and classifications. The buildingSMART Validation Service has not been run. Checked with IfcOpenShell 0.9.0.post1: schema and EXPRESS rules with no issues, placements read back within 0.001 mm, solid volumes within 0.002 %. Not opened in a BIM viewer.
- *Quantities:* there is no rate or cost in any export.
- *Fabrication sheets:* the sheets are DXF and PDF; the DXF sheets are byte-identical to the C# output.
- *Drawing sheets:* A-series paper only; no scale, length, overlap or interval is prefilled. Stations on the forms are entered as the stations shown on the drawings (`1+250.000` or metres, with a stationing region where the alignment has station equations) and converted to internal stations; lengths and intervals on the forms are in the display length unit. Times are UTC. The R12 DXF has no line weights, hatching or layouts, and text uses Helvetica metrics. Plan match lines are straight across the alignment and so are skewed on a curve. Annotation is not drawn on profile, section or detail sheets.
- *Calculation report pack:* the cover records calculations for an engineer to check. It is not a certification. It repeats every superseded-edition and secondary-source warning.
- *Sign schedule:* the schedule is CSV only.

### 8.15 Manage

**Purpose.** The design standard, the model-wide check, the rational method, layers, units, project settings, project information and the appearance.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `UK` | `UKTSM` | Nothing | The UK Traffic Signs Manual is the active standard. |
| `UGANDA` | `UG`, `UGTSM` | Nothing | The Uganda Traffic Signs Manual (2004) is the active standard. |
| `CHECKALL` | `CHECKMODEL`, `CALCPACK` | See 5.8 | The model-wide report. |
| `RATIONAL` | `RUNOFF` | See 5.7 | The rational method result. |
| `LAYER` | `LA`, `LAYERS` | The layer manager: name, on, lock, colour, line weight, element count, current layer; create, rename, delete (refused unless empty, or the elements are moved to a chosen layer) | The layers. Colour (black means automatic) and line weight (a pen width, 1 to 6 pixels on the canvas) apply on the canvas. |
| `UNITS` | `UN` | Length (mm or m), length decimals, station decimals, area (m2 or ha), area decimals, volume decimals, angle format and decimals; Also set the dimension styles to these units | The display units. Stored values stay in millimetres. |
| `PROJECTSETTINGS` | `PSET` | The speed band rule for a km/h limit against the UK mph thresholds: round up within conversion rounding (the default), always the next band up, round up within a tolerance in mph, exact, ask each time, or your own table | The rule, stored in the document. Each place that maps a km/h limit to a UK mph band uses it, and its sentence is written into the calculation lines. The thresholds belong to each check. |
| `PROJECTINFO` | `PROJECT` | See 5.14 | Project information. |
| `THEME` | none | Nothing | Switches the light and dark theme. |
| `CANVAS` | none | Nothing | Switches the model space between dark and light. |

**Limits.**

- The display units apply to the status bar, Properties, the typed points (`MOVE` and `COPY` included), the annotation commands, the station fields of the road dialogs and the measure results. Since round 4 they apply to the design tools and the sheet source forms as well, through the same formatters and parsers: the PI table (east, north, radius, spiral lengths, tangent and curve length, feasible radius), the profile editor (the PVI table: station as `1+250.000`, level, curve length and K as a length per percent, high and low point; the fit form; the cursor readout and the axes), the plan editor's curve labels and cursor readout, the start station of a new alignment and typed PI points, and the sheet forms (station range as `1+250.000`, sheet length, overlap, tick and label intervals, datum level, grid and band intervals, the interval between sections, a list of stations). A bare number is in the display unit and a trailing `m` or `mm` overrides it; a station is typed `1+250.500` or as metres. Scales (1:n), the vertical exaggeration and counts stay as typed, grades are in percent and angles in degrees. Retyping a shown value is not an edit, so a rounded display does not move a point. Stored values stay metric. Still shown raw: the derived values modules report (sizes, areas), the schedule windows, the check results table and the option comparison, the quantity CSVs, and the check and report text. Engine calculation text and CSV are left as the engines wrote them.
- The speed band rule has no thresholds of its own. The restraint check uses 50 mph; sign visibility uses 20, 30, 40, 50, 60 mph; road markings use 40, 50, 60, 70 mph. The lighting inputs dialog still takes its speed limit in mph as typed. The Uganda classes are unchanged.
- Undo covers the layer names, colours and weights. It does not cover on and lock.

### 8.16 View

**Purpose.** Zoom, the 2D and 3D views, display, the grid and appearance. Most View commands repeat Home commands.

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `ZOOM` | `Z`, `ZE` | Nothing | Fits every element. |
| `ZOOMWINDOW` | `ZW` | Two corners | Fits the window. |
| `ZOOMSELECTION` | `ZS`, `ZOOMOBJECT` | Nothing (a selection) | Fits the selected objects, in 2D or 3D. |
| `ZOOMPREVIOUS` | `ZP` | Nothing | Returns to the view before the last zoom. |
| `CRS` | `COORDSYS` | Nothing | Opens the project coordinate system (see the section on coordinate systems). |
| `ZOOMIN` | `ZI` | Nothing | Zooms in about the centre of the view. |
| `ZOOMOUT` | `ZO` | Nothing | Zooms out about the centre of the view. |
| `3D` | `ORBIT`, `PLAN` | Nothing | Switches the 2D model space and the 3D site view. |
| `REAL` | none | Nothing | Shows elements in full colour. |
| `OUTLINED` | `WIREFRAME` | Nothing | Shows elements as outlines. |
| `MARKS` | `LABELS` | Nothing | Shows or hides the mark under each element. |
| `GRID` | none | Nothing | Toggles the grid. |
| `SNAP` | none | Nothing | Toggles snap. |
| `ORTHO` | none | Nothing | Toggles ortho. |
| `THEME` | none | Nothing | Switches the light and dark theme. |
| `CANVAS` | none | Nothing | Switches the model space between dark and light. |

**Limits.** The 3D view is for looking only; editing stays in 2D. Viewpoints for issues are taken from the 3D view (5.10).

### 8.17 Help

| Command | Aliases | Asks for | Produces |
|---|---|---|---|
| `HELP` | `?` | Nothing | The command list window, with how to type points. |
| `ABOUT` | none | Nothing | The About window. |

**Limits.** None beyond those of the commands the list names.

The Civil tab, and the toolspace that goes with it, are described in section 6 (civil objects) and section 3.4 (the toolspace).


## 9. Command reference

This is every command in `crates/afrobim-app/src/commands.rs`: **205 commands**, the same count as the table in that file. Type a name or an alias on the command line, or find it in the command palette (Ctrl+K). Each command is listed once, under the first ribbon tab that holds its button, in ribbon order. The "Also on" column lists the other tabs that hold the same command. The sign templates (STACK, DIRECTION, CONFIRM, PLACE, LANE, DIVERSION, MAP, TEE and ROUNDABOUT) are the sign buttons of the active design standard, on the Signs tab and in the Insert menu. The help text is the one the command table holds, so the same words show on its hover card and in the `?` list.

The counts by group are: Home 22, Edit 3, Insert 7, Annotate 27, Modify 1, Signs 8, Roads 20, Civil 30, Design 10, Structures 4, Coordination 8, 4D 19, 5D 7, Construction 7, Output 13, Manage 10, View 6 and Help 3. Those add to 205. The palette and the command line reach every command.

### Home: 22 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `3D` | `ORBIT`, `PLAN` | View | Switch between the 2D model space and the 3D view |
| `ARRANGE` | none | Modify | Arrange every element in rows |
| `COPY` | `CO`, `CP` | Modify | Copy the selection: base point, a point per copy |
| `DESIGN` | `SIZE` | Modify, Structures | Size the selected sign's posts and foundations for wind (UK method) |
| `ERASE` | `E`, `DELETE` | Edit, Modify | Erase the selection, or ask for one |
| `GRID` | none | Modify, View | Toggle the grid (F7) |
| `LIBRARY` | `INSERT`, `LIB` | Insert | Open the standard-sign library |
| `MOVE` | `M` | Modify | Move the selection: base point, second point |
| `NEW` | none | - | Start a new document |
| `OPEN` | none | Insert | Open an AfroBIM document |
| `ORTHO` | none | Modify, View | Toggle ortho: moves held to X or Y (F8) |
| `REDO` | none | Edit | Redo the edit that was undone |
| `SAVE` | `QSAVE` | Output | Save the document |
| `SAVEAS` | none | Output | Save the document under a new name |
| `SNAP` | none | Modify, View | Toggle grid snap for picked points (F9) |
| `STACK` | `SIGN` | Signs | Insert a stack-type direction sign |
| `STATION` | `CHAINAGE` | Modify, Roads | Place the selected sign on an alignment by station and offset |
| `SUPPORTS` | `POSTS` | Modify, Structures | Add posts and foundations to the selected sign |
| `UNDO` | `U` | Edit | Undo the last edit |
| `ZOOM` | `Z`, `ZE` | View | Zoom to show every element |
| `ZOOMSELECTION` | `ZS`, `ZOOMOBJECT` | View | Zoom to the selected objects |
| `ZOOMWINDOW` | `ZW` | View | Zoom to a window picked with two corners |

### Edit: 3 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `COPYCLIP` | none | - | Copy the selection to the clipboard |
| `PASTECLIP` | `PASTE` | - | Paste the clipboard into the model space |
| `SELECTALL` | `ALL` | - | Select every element |

### Insert: 7 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `IFCREF` | `REFERENCEMODEL`, `IMPORTIFC` | Coordination | Import IFC files as read-only reference models, stored by path and hash, shown in the 3D site view and usable in clash tests |
| `LANDXML` | `ALIGNMENT` | Roads, Civil, Design | Import horizontal alignments from a LandXML file |
| `REFMODELS` | `REFERENCES`, `REFLIST` | Coordination | List the reference models: visibility, transparency, alignment offset and rotation, import report, and the properties of a picked product |
| `SIGNSCHEDULE` | `SIGNSCHED` | - | Import signs from a CSV schedule: column mapping, import plan, apply as one undo step |
| `SURFACE` | `TIN`, `TERRAIN` | Roads | Import ground surfaces (TIN) from a LandXML file |
| `SURVEY` | `SURVEYPOINTS`, `POINTFILE`, `TOPO` | Roads | Make a ground surface from a file of survey points (CSV or text): column order, coordinate system, longest triangle side, descriptions to leave out |
| `UNDERLAY` | `XREF`, `XATTACH`, `DXFUNDERLAY` | - | Attach DXF drawings (or DWG, through ODA File Converter) as read-only background references, by path and hash |

### Annotate: 27 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `AREA` | `AA` | - | Area and perimeter of the polygon of the points picked (Enter closes it) |
| `DIM` | none | - | Dimension between two points; linear or aligned is inferred from the picks |
| `DIMALIGNED` | `DAL` | - | Dimension along the line between two points |
| `DIMANGULAR` | `DAN` | - | Angle at a vertex between two points |
| `DIMARC` | `DAR` | - | Length of an arc: centre, start point, end point |
| `DIMBASELINE` | `DBA` | - | Stack dimensions from the first extension line of the selected (or last) linear dimension |
| `DIMCONTINUE` | `DCO` | - | Continue the selected (or last) linear dimension from its last extension line |
| `DIMDIAMETER` | `DDI` | - | Diameter of a circle given by its centre and a point on it |
| `DIMLEVEL` | `DLEV` | - | A level at a point: from the surface or the alignment profile, or typed |
| `DIMLINEAR` | `DLI` | - | Horizontal or vertical dimension: two points, then the dimension line |
| `DIMRADIUS` | `DRA` | - | Radius of a circle given by its centre and a point on it |
| `DIMSTATION` | `DSTA` | - | Station and offset of a point on an alignment, in the stations shown on the drawings |
| `DIMSTYLE` | `DST` | - | Manage the named dimension styles stored in the document |
| `DIST` | `DI`, `MEASURE` | - | Distance between points; more points give a running total (the result is also kept in MEASURERESULTS) |
| `ID` | `WHERE` | - | Coordinates of a point, with station, offset and level on the nearest or a named alignment |
| `LEADER` | `LE` | - | A leader: arrow point, further points, then the text |
| `MARKS` | `LABELS` | View | Show or hide the mark under each element |
| `MEASUREANGLE` | `ANGLE` | - | Angle at a vertex between two points |
| `MEASURERADIUS` | `RADIUSOF` | - | Radius and centre of the circle through three points |
| `MEASURERESULTS` | `RESULTS` | - | Open the measure results window: every result, with a copy button |
| `MLEADER` | `MLD` | - | A leader that can attach to an element, so its text is the mark of the element and follows it |
| `MTEXT` | `MT` | - | Place multi-line text in a box: two corners (the width), then the text; type \\n for a new line |
| `OSNAP` | `OS` | - | Toggle object snap (F3) |
| `OSNAPSETTINGS` | `DSETTINGS`, `SNAPSETTINGS` | - | Choose which object snap modes are on, and see what provides each |
| `STYLE` | `ST`, `TEXTSTYLE` | - | Manage the named text styles stored in the document |
| `TEXT` | `DT`, `DTEXT` | - | Place a line of text: start point, height, rotation, then the text |
| `TEXTEDIT` | `ED` | - | Edit the text of the selected text or leader (also a double-click on it) |

### Modify: 1 command

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `NOSUPPORTS` | none | Structures | Remove the selected sign's posts and foundations |

### Signs: 8 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `CONFIRM` | `RC` | - | Insert a route confirmatory sign |
| `DIRECTION` | `DS` | - | Insert a rectangular direction sign |
| `DIVERSION` | none | - | Insert a temporary diversion sign |
| `LANE` | none | - | Insert a lane pre-selection sign |
| `MAP` | `CROSSROADS` | - | Insert a map-type crossroads sign |
| `PLACE` | none | - | Insert a place identification sign |
| `ROUNDABOUT` | `RBT` | - | Insert a map-type roundabout sign |
| `TEE` | none | - | Insert a map-type T-junction sign |

### Roads: 20 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `BARRIER` | `RESTRAINT` | - | Add a safety barrier run along an alignment (containment, working width and impact severity are entered) |
| `CHECK` | `VERIFY` | Structures | Run the check of the selected gantry, side drain, culvert, street lighting, barrier run or pavement, with the inputs you enter (kept on the element for CHECKALL) |
| `CORRIDOR` | none | - | Build a corridor on an alignment and a surface: a typical section, cut and fill, earthworks |
| `CULVERT` | none | - | Add a pipe culvert across an alignment at a station, with placeholder dimensions to edit |
| `DRAIN` | `SIDEDRAIN` | - | Add a side drain along an alignment, with placeholder dimensions to edit |
| `LIGHTCOLUMN` | `COLUMN` | - | Place one lighting column at a station and offset on an alignment |
| `LIGHTING` | `LIGHTS`, `STREETLIGHTING` | - | Lay a street lighting installation along an alignment (spacing, arrangement and the column are entered) |
| `MARKARROW` | `ARROWS` | - | Lay a lane or deflection arrow (UK arrows and Uganda curved arrows are placeholder blocks, not the true shape) |
| `MARKHATCH` | `HATCHING`, `CHEVRONS` | - | Lay hatching, chevrons, island markings or a zig-zag line along the road |
| `MARKINGS` | `MARKING`, `ROADMARKING` | - | Lay a road marking line along an alignment (Traffic Signs Manual Chapter 5) |
| `MARKLINES2` | `UGLINES` | - | Lay a lane, edge or double line of the Uganda Traffic Signs Manual |
| `MARKSTUDS` | `STUDS`, `ROADSTUDS` | - | Lay road studs along a line, from a row of the manual's stud tables |
| `MARKTRANSVERSE` | `STOPLINE`, `GIVEWAY` | - | Lay a stop line, a give way line or the give way triangle across the road, to the UK Chapter 5 or the Uganda manual |
| `MARKWORD` | `WORDED`, `SLOWMARK` | - | Lay a worded marking such as STOP or SLOW (a placeholder block, not the true letters) |
| `MARKYELLOW` | `YELLOWLINE`, `WAITING` | - | Lay a yellow line: waiting restriction or yellow dividing line |
| `PAVEDESIGN` | `PAVEMENTDESIGN` | - | Design a pavement structure by the MoWT manual, UK DMRB or AASHTO 1993, and make a pavement from the layers |
| `PAVEMENT` | `PAVE` | - | Add a pavement along an alignment: width, cross-fall and a list of layers |
| `PHOTOMETRY` | `IES`, `LDT`, `LUMINAIREFILE` | - | Load an IES or LDT file for the selected lighting installation, check it and see its polar diagram, then calculate illuminance on grids you define |
| `RATIONAL` | `RUNOFF` | - | Peak flow by the rational method from the catchment areas, runoff coefficients and intensity you enter; keep it on the selected drain or culvert |
| `SECTIONS` | `XSECTIONS` | - | Show the cross-sections of the selected corridor, station by station |

### Civil: 30 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `ACQUISITION` | `PARCELSCHEDULE`, `LANDSCHEDULE` | - | Land acquisition schedule and parcel area table, with CSV export |
| `ALIGNDESIGN` | `NEWALIGNMENT`, `DESIGNALIGNMENT` | Design | Design an alignment from PIs picked on the plan or typed as coordinates: name, start station; curves start with no radius |
| `ASSEMBLY` | `ASSEMBLIES`, `SUBASSEMBLY` | - | Assembly editor: build a typical section from the subassembly library with a live preview |
| `CIVILDEMO` | `DEMOCIVIL` | - | Build a small set of demonstration civil data at UTM-sized coordinates (example values, not a design) |
| `CIVILXML` | `LANDXMLALL`, `LANDXMLCIVIL` | - | Write everything civil that the document holds to a LandXML file, with the project coordinate system |
| `COGO` | `INVERSE`, `COGOTOOLS` | - | COGO tools: inverse, bearing and distance, intersections, station and offset to an alignment |
| `CORRIDORMODEL` | `NEWCORRIDOR`, `CORRIDOR2` | - | Corridor from a baseline, regions with an assembly each, a frequency and a ground surface (the older CORRIDOR keeps working on old documents) |
| `DESCKEYS` | `DESCRIPTIONKEYS` | - | Edit the description keys that turn field codes into full descriptions, styles and layers |
| `EARTHWORKS` | `CIVILQUANTITIES`, `MASSHAUL` | - | Earthworks table, material volumes by shape code and the mass haul diagram, with CSV export and a send to the bill |
| `FLOWPATH` | `WATERDROP` | - | Trace the steepest-descent path of water from an easting and northing over a surface |
| `GROUNDPROFILE` | `EXISTINGPROFILE`, `SAMPLEPROFILE` | - | Sample an existing ground profile from a surface along an alignment |
| `PARCELDEED` | `DEED` | - | Create a parcel from the courses of a deed (bearing and distance) with a closure report |
| `PARCELPICK` | `PARCEL`, `CREATEPARCEL` | - | Create a parcel from typed or picked corner points |
| `PARCELPROPS` | `PARCELPROPERTIES` | - | Parcel properties: owner, land reference, land use, status |
| `PARCELSPLIT` | `RESERVE`, `ROADRESERVE` | - | Split the parcels by an alignment offset (road reserve): take and remainder parcels |
| `PARCELTOPOLOGY` | `TOPOLOGY` | - | Check the parcels of the site for gaps and overlaps |
| `POINTCREATE` | `CREATEPOINT`, `PT` | - | Create a COGO point by typed easting, northing and elevation, or by picking in the model |
| `POINTEXPORT` | `EXPORTPOINTS` | - | Export the points of a point set or group to a point file |
| `POINTGROUPS` | `PTGROUPS` | - | Point groups: include and exclude rules, surface points, order |
| `POINTS` | `IMPORTPOINTS`, `COGOPOINTS` | - | Import COGO points from a point file: the format is detected and previewed, the coordinate system is checked, and the duplicate policy is yours |
| `POINTTABLE` | `PTTABLE` | - | Edit the points of a point set in a table: sort, filter and edit any cell |
| `PROFILEVIEW` | `PVIEW` | - | Draw a profile view in the model: grid, ground and design lines and bands |
| `SAMPLELINES` | `SAMPLELINEGROUP`, `SL` | - | Sample line group: stations, swath widths and the surfaces cut at each line |
| `SECTIONVIEWS` | `SECVIEWS` | - | Draw the section views of a sample line group as an array in the model |
| `SPOT` | `SPOTELEVATION`, `SPOTELEV` | - | Read the elevation of a surface at an easting and northing |
| `SURFACEDEF` | `SURFACEDEFINITION`, `DEFINITION` | - | Edit the definition of a surface: operations, order, rebuild and the build report |
| `SURFACENEW` | `CREATESURFACE`, `NEWSURFACE` | - | Create a surface from a definition: a name, a role and operations |
| `SURFACESTYLE` | `CONTOURS`, `SURFSTYLE` | - | Surface style: triangles, border, contours, elevation and slope bands, slope arrows |
| `TRAVERSE` | `TRAV` | - | Enter a traverse and adjust it (compass or transit rule) with a misclosure report |
| `VOLUMES` | `VOLUMEDASHBOARD`, `CUTFILL` | - | Volumes between two surfaces or against a level, bounded by an optional polygon, with a stage-storage table |

### Design: 10 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `ALIGNCHECK` | `DESIGNCHECK`, `GEOMCHECK` | - | Check a designed alignment against a limits table you load (CSV or JSON): per element, with the source of each limit |
| `ALIGNEDIT` | `PLANEDIT`, `PIEDIT` | - | Edit a designed alignment in plan: drag PIs, insert a PI on a tangent, delete a PI; a move the engine refuses shows its reason |
| `ALIGNLABELS` | `CURVELABELS` | - | Show or hide the curve data labels on the plan |
| `CHECKALL` | `CHECKMODEL`, `CALCPACK` | Manage | Run every check on every element and open one report: a status per element, the calculation lines, filters, CSV and a calculation pack |
| `LANDXMLOUT` | `EXPORTLANDXML` | - | Export the designed alignment and its profile as LandXML |
| `OPTIONS` | `ALIGNOPTIONS`, `OPTIONSET` | - | Alignment options: add the current design as an option, compare the options on figures worked out from the model, score them with your own weights |
| `PITABLE` | `PIS`, `CURVETABLE` | - | PI table of the designed alignment: station, coordinates, deflection, radius, spirals, tangent and curve length, with the feasible radius range |
| `PROFILEDESIGN` | `PROFILEEDIT`, `PVI` | - | Profile editor of the designed alignment: ground line, PVIs to click and drag, the PVI table, vertical exaggeration and Fit to ground |
| `VISIBILITY` | `SIGHTLINE`, `SIGNVISIBILITY` | - | Sign visibility along the alignment: enter the inputs for the selected sign or all signs, run, and see the check lines with citations, the plan envelope and the sight-line profile |
| `VISIBILITYRUN` | `VISRUN`, `RUNVISIBILITY` | - | Run the stored visibility inputs of the selected sign, or of every placed sign when none is selected, and open the results |

### Structures: 4 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `CANTILEVER` | none | - | Add a cantilever sign gantry at a station, with placeholder dimensions to edit |
| `GANTRY` | `PORTAL` | - | Add a portal sign gantry at a station, with placeholder dimensions to edit |
| `HANG` | `GANTRYSIGN` | - | Hang the selected sign on a gantry |
| `UNHANG` | none | - | Take the selected sign off its gantry |

### Coordination: 8 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `BCFIN` | `IMPORTBCF` | - | Import issues from a BCF 2.1 file and show what could not be carried |
| `BCFOUT` | `EXPORTBCF` | - | Export the issues as BCF 2.1 (not tested in other BCF tools; BCF 3.0 is not supported) |
| `CLASH` | `CLASHRUN`, `CLASHDETECT` | - | Run every clash test on the model and open the results |
| `CLASHCSV` | none | - | Export the clash results as CSV |
| `CLASHES` | `CLASHRESULTS` | - | Open the clash results: filter, review, comment, assign, zoom to a clash |
| `CLASHMARKERS` | none | - | Show or hide the clash markers in the 3D view |
| `CLASHTESTS` | `CLASHSETUP` | - | Add, edit and delete clash tests: what is checked against what, hard or clearance, with your own tolerance |
| `ISSUES` | `ISSUELIST`, `TOPICS` | - | Open the issues: raise them from clashes or by hand, comment, assign, keep viewpoints from the 3D camera with a snapshot |

### 4D: 19 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `BASELINE` | none | - | Keep the current schedule as the baseline |
| `CALENDAR` | `WORKDAYS` | - | Edit the programme calendar: working weekdays and the days off you enter |
| `CRITICAL` | `CRITICALBASIS`, `LONGESTPATH` | - | Choose what makes an activity critical: total float of zero or less, or the longest path, with the engine's definition |
| `FOURD` | `DATE4D` | Construction | Show the model at a chosen date: not started, in progress, complete, removed |
| `GANTT` | none | - | Show the Gantt chart: critical path, baseline bars and the data date |
| `LINK4D` | `LINKACTIVITY` | - | Link model elements to activities, by selection or by rule |
| `LINKREPORT` | none | - | List elements no activity reaches and activities with no elements |
| `MSPIN` | `IMPORTMSP` | - | Import a programme from MS Project XML and show what the file could not carry |
| `MSPOUT` | `EXPORTMSP` | - | Export the programme as MS Project XML |
| `PLAY4D` | `PLAYBACK`, `ANIMATE4D` | - | Play the programme in the 3D site view: play, pause, speed and a scrub bar, with the date shown large |
| `PROGCSV` | `PROGRAMMEREPORTS` | - | Export the activity table, variance and S-curve as CSV |
| `PROGRAMME` | `ACTIVITIES`, `PROG` | - | Edit the construction programme: activities, durations, constraints and dependencies |
| `PROGRESS` | `DATADATE` | - | Enter progress and the data date, then reschedule |
| `SCHEDULE` | `CPM` | - | Run the critical path method and show dates, float, the critical path and any problems |
| `TIMELOCATION` | `TLCHART`, `TIMECHAINAGE` | - | Time-location chart of the programme along an alignment: location ranges in stations, direction of working, dwell, critical marking, data date and baseline |
| `VARIANCE` | none | - | Show how the programme differs from the baseline |
| `WBS` | `SUMMARYTASKS`, `ROLLUPWBS` | - | Edit the work breakdown structure: summary tasks, which summary each activity sits under, and the roll-up on the basis you choose |
| `XERIN` | `IMPORTXER`, `P6IN` | - | Import a programme from a Primavera P6 XER file and show what the file could not carry (files have not been tested in P6) |
| `XEROUT` | `EXPORTXER`, `P6OUT` | - | Export the programme as a Primavera P6 XER file and show what the file does not carry (not tested in P6) |

### 5D: 7 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `BILL` | `BOQSETUP` | - | Edit the bill of quantities and its mapping rules; see measured quantities, unmapped and ambiguous lines |
| `CASHFLOW` | `SCURVE` | - | Phase the estimate in time (dates from the programme if you like): monthly table and S-curve |
| `CERTIFICATE` | `IPC`, `INTERIM` | Construction | Work out an interim payment certificate from your own payment terms |
| `COSTCSV` | `COSTREPORTS` | - | Export every cost report as CSV into a folder |
| `CURRENCY` | `EXCHANGE` | - | Set the primary currency label, an optional secondary currency and dated exchange rates |
| `ESTIMATE` | `PRICE` | - | Show the priced bill, additions, audit trail and revision comparison |
| `RATES` | `RATELIBRARY` | - | Enter or load rates (with currency, source note and date); lists unpriced items |

### Construction: 7 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `EVM` | `EARNEDVALUE` | - | Earned value at a data date: budget, actual costs, the EAC formula you choose, the figures, the tables and the S-curve |
| `EVMCSV` | `CONSTRUCTIONCSV` | - | Export every earned value table as CSV into a folder |
| `LOOKAHEAD` | `DELAYS`, `DELAYLIST` | - | Activities due to start or finish in the next days you enter, and the delay list |
| `ROLLUP` | `PROGRESSUPDATE` | - | Roll the element records up to the activities, see the inferred actual dates, then apply the update to the programme and reschedule |
| `SITELOG` | `SITERECORDS` | - | List the site records, filtered by element and date |
| `SITEREC` | `SITERECORD` | - | Record percent complete or a measured quantity at a date for the selected elements, or by category, layer or mark pattern |
| `SITEVIEW` | `PROGRESSCOLOUR` | - | Colour the 3D view by recorded percent complete at the 4D date, or back to the programme state |

### Output: 13 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `DXFOUT` | `EXPORT` | - | Export the selection, or everything, as DXF |
| `IFCOUT` | `IFC` | - | Export the model as IFC 4.3 |
| `QUANTITIES` | `BOQ` | - | Export sign-face quantities (CSV and summary) |
| `QUIT` | `EXIT` | - | Close AfroBIM |
| `REPORTPACK` | `CALCREPORT`, `REPORTS` | - | Compose the calculation report pack (one A4 PDF, plain text and CSV): cover, contents, design basis, every check and the summaries |
| `SHEETDXF` | `SHEETSDXF` | - | Export the sheet set as DXF, one file per sheet in a folder |
| `SHEETPDF` | `PLOTSHEETS` | - | Export the whole sheet set as one PDF, a page per sheet |
| `SHEETPREVIEW` | `PREVIEWSHEETS` | - | Compose the sheet set from the live model and page through the sheets, with the composer's warnings |
| `SHEETS` | `FABRICATION` | - | Export fabrication sheets (DXF and PDF) and the schedule PDF |
| `SHEETSET` | `DRAWINGSHEETS`, `SHEETMANAGER` | - | Open the sheet set manager: make sheet sets and add plan, profile, section, schedule, detail and notes sheets |
| `SIGNSCHEDULEOUT` | `SIGNSCHEDOUT` | - | Export the signs of the model as a sign schedule CSV |
| `SIGNSCHEDULETEMPLATE` | `SIGNSCHEDBLANK` | - | Write a blank sign schedule template (CSV) |
| `TITLEBLOCK` | `TBLOCK` | - | Edit the title block layout of the sheet set: fields shown, rows and cells, load or save it as JSON |

### Manage: 10 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `APPEARANCE` | `LOOK`, `SCALE`, `INTERFACE` | View | Interface look (dark glass, light glass, solid) and scale |
| `CANVAS` | none | View | Switch the model space between dark and light |
| `CRS` | `COORDSYS` | - | The project coordinate system: set or change it, and see what kind it is, its datums and area of use. Nothing is ever moved |
| `LAYER` | `LA`, `LAYERS` | - | Layer manager: create, rename, delete, colour, line weight, on, lock, current |
| `PROJECTINFO` | `PROJECT` | - | Edit the project information that title blocks print: project, client, contract number, revisions and other fields |
| `PROJECTSETTINGS` | `PSET` | - | Project settings: the speed band rule that meets a km/h limit with the UK mph thresholds |
| `THEME` | none | View | Switch between the light and dark themes |
| `UGANDA` | `UG`, `UGTSM` | - | Use the Uganda Traffic Signs Manual |
| `UK` | `UKTSM` | - | Use the UK Traffic Signs Manual |
| `UNITS` | `UN` | - | Display units: length, decimals, stations, areas, volumes and angles (stored values stay metric) |

### View: 6 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `OUTLINED` | `WIREFRAME` | - | Show elements as outlines |
| `REAL` | none | - | Show elements in full colour |
| `TOOLSPACE` | `PROSPECTOR` | - | Show or hide the toolspace: the Prospector tree of the civil objects and the Settings tab |
| `ZOOMIN` | `ZI` | - | Zoom in about the centre of the view |
| `ZOOMOUT` | `ZO` | - | Zoom out about the centre of the view |
| `ZOOMPREVIOUS` | `ZP` | - | Return to the previous view |

### Help: 3 commands

| Command | Aliases | Also on | Help text |
|---|---|---|---|
| `ABOUT` | none | - | About AfroBIM |
| `HELP` | `?` | - | List every command |
| `PALETTE` | `COMMANDS`, `FIND` | - | Search every command by name, alias or description (Ctrl+K) |


## 10. Standards and editions

Each row gives a document, the edition the code uses, whether the README says it is current, superseded or secondary, and what the code takes from it. The "Status" column repeats the warnings the crate READMEs give. Where a document was not held, the row says so, and the code leaves the value to the engineer.

| Area | Document | Edition held | Status (as the READMEs state) | Taken from it |
|---|---|---|---|---|
| Signs, UK | DfT Traffic Signs Manual Chapter 7 "The Design of Traffic Signs"; TSRGD 2016 Schedules 2, 3, 9, 10 and 12 (rule pack `UK-TSM-2019`) | Chapter 7 (2019) | The pack carries Open Government Licence v3.0 material, which may need an attribution notice (installing.md). No superseded warning in the sources held. | Sign layout, the sign catalogue, sizes, colours, lettering and the destination symbols. Golden-tested against the C# engine. |
| Signs, Uganda | Uganda Traffic Signs Manual, Volume 1 and Volume 2 (Appendix 1 and Appendix 3), July 2004 (rule pack `UG-TSM-2004`) | 2004 | The pack's own edition. Embedded signs under this pack are not ported. | Uganda sign rules, catalogue and the 241 golden fixtures. |
| Signs, UK | TSRGD 2016 (SI 2016/362) | 2016 | Read for the diagrams the markings chapter refers to and does not dimension. Amendments after 2016 are not applied. | Diagram numbers and Schedule references in the markings catalogue. |
| Markings, UK | Traffic Signs Manual Chapter 5, Road Markings | **2018, current** | **The 2003 edition is superseded and was not used.** Downloaded from GOV.UK and checked against the owner's two-PDF copy of the same edition, which is not in the repository. | Line types, widths, patterns, speed tables, transverse, arrow, hatching, yellow and stud markings. |
| Markings, Uganda | Uganda Traffic Signs Manual, Volume 1 section E and Volume 2 Appendix 3 section 7 | July 2004 | Sign pages carry no page numbers; cited by sign number and PDF page. | The Uganda lane, edge, double and transverse markings, and the lane arrows as blocks. |
| Drainage, MoWT | MoWT *Road Design Manual Volume 2: Drainage Design* | January 2010 (PDF produced May 2011; fetched 2026-11-15) | Obtained from a third-party copy. | Recurrence intervals, rational method, time of concentration, Manning, permissible velocities, culvert outlet control, headwater limit of 1.5 D, minimum culvert diameter of 600 mm. |
| Drainage, UK | DMRB CG 501 *Design of highway drainage systems*; CD 521 *Hydraulic design of road edge surface water channels and outlets*; CD 522 *Drainage of runoff from natural catchments*; CD 529 *Design of outfall and culvert details* | v2.2.0, v1.2.0, v0.1.0, v1.1.0 (all September 2025) | Current versions as obtained. **Under this basis there is no rainfall or runoff method and no culvert hydraulics.** CIRIA C786 and CD 532 are not held. | Criteria, roughness, Manning's equation, outlet and culvert details. |
| Culverts, US | FHWA *Hydraulic Design of Highway Culverts*, HDS-5, third edition (FHWA-HIF-12-026) | April 2012 | A US federal publication, used for inlet control equations under the MoWT basis. | Inlet control equations (HDS-5 Table A.1 inlet edges and the flow zones). |
| Drainage, South Africa | SANRAL *Drainage Manual*, sixth edition, with the *Drainage Manual Application Guide* and *Addendum 1* | September 2013 (Addendum 29 May 2013) | Public SANRAL downloads. Addendum items on pages 8-1 and 8-7 were not found in the print, so they are not relied on. | Rational method (Tables 3.7 and 3.8), time of concentration, culvert check (Table 7.2 and Equation 7.4). The Standard Design Flood, unit hydrograph, SCS-SA and the empirical and statistical flood methods are not done. |
| Gantry headroom | DMRB CD 127 *Cross-sections and headrooms* | Version 1.0.1, 29 July 2021 | Served without a superseded mark. The currency was judged from that and the DMRB index of 15 October 2023; confirm it. | Headroom (Table 4.1), sag compensation (Table 4.3), structure free zone, set-back for restraint (CD 377 clause 2.37). |
| Gantry requirements | DMRB CD 365 *Portal and cantilever signs/signals gantries* | Revision 1, March 2020 | **Watermarked SUPERSEDED.** The current version was not obtained; a third-party index lists a Version 1.1.0 (September 2025) that was not verified. Confirm every clause. | Clauses 3.19, 3.21, 3.22, 3.23 and the deflection and support rules listed in 5.9. |
| Restraint, requirements | DMRB CD 377 *Requirements for road restraint systems* | Version 5.0.1, 27 February 2026 (with the National Application Annexes attached) | Current as obtained. The RRRAP is outside the tool. | Minimum containment, set-back, terminals, taper, gaps, transitions, scope thresholds (50 mph, 5,000 AADT). |
| Restraint, earlier copy | DMRB CD 377 Revision 4, January 2021 | n/a | **Watermarked SUPERSEDED.** Read only; no value is taken from it. | Nothing. |
| Restraint, performance | BS EN 1317-2:2010 | The owner's copy | The owner should confirm it is still the current edition. | Impact tests, containment levels, impact severity, working width and vehicle intrusion tables. |
| Restraint, terminals | DD ENV 1317-4:2002 | Draft for Development, the English version of ENV 1317-4:2001 | Not a British Standard. CD 377 5.0.1 still cites it; replace it when a superseding document is available. | Terminal classes P1 to P4 (Table 1, clause 5.1). |
| Restraint, cross-check | TII DN-REQ-03034 *Safety Barriers* (Ireland), November 2015 with Amendment 1 | n/a | Cross-check only. Its tables agree with the owner's copy of EN 1317-2. | Not used in any calculation. |
| Restraint, MoWT | MoWT *Geometric Design Manual* Volume 1, Section 12.3 | January 2010 | Guidance of a different kind ("can only give a brief introduction"). Secondary reference only. | Optional NOTE lines only (`include_mowt_reference`). |
| Restraint, not held | PD CEN/TS 1317-7 (terminal classes T80/3, T110, T110/1); PD 6688-1-7; BS EN 1317-1, -3, -5; AASHTO MASH 2016 | n/a | Not held (PD CEN/TS 1317-7 and PD 6688-1-7), or held but not coded from (the others). | Named only. |
| Lighting, requirements | DMRB TD 501 *Road lighting design* | **Version 0.1.0, September 2025** | Current as obtained. It supersedes Revision 0 (March 2020), which the site watermarks SUPERSEDED. | Lighting class, maintenance, unlit gaps, conflict areas, the outputs and their requirements. |
| Lighting, appraisal | DMRB TA 501 *Road lighting appraisal* | Revision 0, March 2020 | The ILP says it was updated in 2025, but the search returned Revision 0. | The need for lighting (clause 1.1). |
| Lighting, speed | DMRB CD 109 *Highway link design* | Version 1.1.0, 30 October 2026 | Current as obtained. | Stopping sight distance, Table 2.10 (not interpolated). |
| Lighting, clearances (superseded) | BS 5489-1:2003 with Corrigendum No. 1 | 2003 | **Superseded.** Replaced in 2013 and again in 2020. Its ME, CE and S classes are not the M, C and P classes. Every report line that uses it says "superseded edition". | Column clearances (Table 2), vertical clearance, bracket projection, Tables B.1 to B.3. |
| Lighting, class values (secondary) | Stockmar, *Handbook about Interpretation of EN 13201* (LiTG, INTERREG Central Europe CE 452, deliverable D.T4.1.4, October 2017) | 2017 | **Secondary source.** It interprets EN 13201-2:2015, EN 13201-5:2015 and CEN/TR 13201-1:2014. BS EN 13201-2, -3 and -5 were not supplied. The owner should confirm each value. | The M, C and P class tables (Tables 5.1 to 5.3) and the class selection weights. |
| Lighting, MoWT | MoWT *Road Design Manual* Volume 1, Section 12.8 | January 2010 | Secondary reference only. | Reference lines only. |
| Lighting, secondary (Irish) | TII DN-LHT-03038, *Design of Road Lighting for the Strategic Motorway and All Purpose Trunk Road Network* | February 2012 | Secondary. It is the NRA addendum to TD 34/07 (2007), which TD 501 has replaced. | Paragraphs 2.9 and related lines, as secondary references. |
| Lighting, photometry | EN 13032-1 (public preview) and the IES LM-63 and EULUMDAT file formats | n/a | The photometry is not an EN 13201-3 calculation and is not validated against DIALux, Relux or AGi32. | Reading of luminaire files, flux and peak checks, the C-gamma system, and the grid calculation. |
| Pavement, MoWT | MoWT *Road Design Manual* Volume 3, Part I, Flexible Pavements | January 2010 (fetched from a third-party copy) | It adopts the SATCC *Draft Code of Practice for the Design of Road Pavements* (1998, reprinted 2001). The assumption that it governs is the crate author's. | Traffic, pavement classes, the catalogue and the structure (Part I). |
| Pavement, DMRB traffic | DMRB CD 224 *Traffic assessment* | Revision 0, March 2020 | **Superseded** by Version 0.1.0 (2025). The current revision was not obtained. | Growth factors, wear factors, the heaviest-lane percentage and the design traffic equations. |
| Pavement, DMRB thickness | DMRB CD 226 *Design for new pavement construction* | Version 0.1.0, 29 November 2021 | **Superseded** by Version 0.2.0 (2025). The current revision was not obtained. The method gives no complete structure. | Foundation classes and the asphalt over an HBGM base equation. |
| Pavement, AASHTO | AASHTO *Guide for Design of Pavement Structures* (1993), as reproduced in FHWA NHI-05-037 Appendix C and lecture notes | 1993 (the Guide itself was not read) | Reproduced, not read at source. | The design equation, solved for the structural number, and the layered check. |
| Pavement, South Africa | SANRAL *South African Pavement Engineering Manual* (SAPEM), Chapter 10 Pavement Design | Second edition, 2014 (a third-party flip-book copy) | Read. Reproduces draft TRH4 (1996) values. **The TRH4 catalogue could not be read.** TRH4 is a 1996 draft, and every calculation says so. | Design traffic in E80, traffic classes, road categories and reliability, climatic class and subgrade action. No structure. |
| Pavement, not obtained | TRH4 (1996), TRH14 (1985), SAPEM Chapter 9 | n/a | Not obtained. | Material codes are not defined. |
| Supports, wind and foundations | IHE *Sign Structures Guide* 2021 | 2021 | Used under the UK method. Two printed figures do not follow from the guide's own expressions; the module follows the expressions. | Wind force, factors, planted and spread foundation checks, Tables 5.1 to 5.3. Appendix C examples reproduced in tests. |
| Supports, wind code | BS EN 12899-1:2007 with the UK National Annex | 2007 | Used through the IHE guide. | Force coefficients (Table NA.2) and the wind procedure. |
| Supports, member | BS EN 1993-1-1 | n/a | Classification and resistance of the steel tube. Class 4 is refused. | Member capacity. |
| Supports, ground | BS EN 1997-1, Design Approach 1 | n/a | With the guide's assumed bearing resistances. | Spread foundation geotechnical checks. |
| Supports, planting | PD 6547 or CD 354 Revision 1 | n/a | Not held in the repository. The method is selected by the engineer. | Planted foundation method. |
| Supports, planting depth | BS EN 40-2 Table 7 | n/a | **Not held.** The engineer enters the minimum planting depth. | Nothing: an input. |
| Supports, Uganda wind | MoWT *Road Design Manual* Volume 4: Bridge Design, Section 5.3, Table 2 | January 2010 | A reading of two documents that do not refer to each other. The Figure 2 wind map is blank in the PDF. | Basic wind pressure for Uganda: 0.81 kN/m² open terrain, 0.37 kN/m² built-up, up to 5 m. |
| Alignments | LandXML 1.2 (schema `LandXML-1.2.xsd`) | 1.2 | Checked against the schema and AfroBIM's importer. **Not tested against a Civil 3D export.** | Import and export of alignments, profiles, station equations, cross-fall at full superelevation. |
| Design checks, Uganda MoWT | MoWT *Road Design Manual Volume I: Geometric Design* | January 2010 (third-party copy hosted by the Uganda Association of Consulting Engineers, not compared with a Ministry original) | The edition is the one the document states. Withheld cells are listed in section 11. The manual is partly guidance and partly standard. | The built-in basis `mowt_2010` for `ALIGNCHECK` and the design checks; superelevation tables. |
| Design, AASHTO | AASHTO *A Policy on Geometric Design* (Green Book) | n/a | No citable, retrievable copy of the tables was found. | Nothing. Values the user enters carry their own edition in `#source`. |
| Visibility, UK | Traffic Signs Manual Chapter 1 (1982), Chapter 2 x-height extract, Chapter 3 (2008), Chapter 4 (2013), Chapter 7 (2013) | Editions differ by chapter and are stated per value | Each value cites its edition and page. | Clear visibility distances, reading, siting, mounting height, set-back, eye lane and overhead lines. |
| Visibility, Uganda | Uganda Traffic Signs Manual, Volume 1 (July 2004) and Volume 2 Appendix 3 | 2004 | The 1.05 m eye height is stated in the manual for other criteria. | Visibility distance, second sign spacing, siting, letter sizes, mounting and clearance. |
| Coordination, BCF | BCF 2.1 (buildingSMART BCF-XML) | 2.1 | Checked against the schemas and read back by AfroBIM. Not opened in any other BCF tool. BCF 3.0 is refused. | Issues exchange. |
| IFC import | IFC2X3, IFC4 and IFC4X3 attribute tables | IfcOpenShell 0.9.0.post1 used as the comparison | Compared with IfcOpenShell on test files. Not opened in a commercial BIM tool. | Reference models. |
| IFC export | IFC 4.3 (schema IFC4X3_ADD2) | IfcOpenShell 0.9.0.post1 used to check | The buildingSMART Validation Service has **not** been run, because it asks for a signed-in account. | The export of signs, supports, alignments and road modules. |
| 4D, MS Project | MS Project XML (MSPDI) | n/a | Not tested in MS Project. | Import and export of the programme. |
| 4D, Primavera P6 | Primavera XER: Oracle *XER Import/Export Data Map Guide (Project)* (2023); the open-source readers `xerparser` 0.13.9 and `PyP6XER` 2.0; MPXJ `ConstraintTypeHelper`; published sample files | n/a | **Files have not been opened in P6.** The sample files were read, not P6. | Column names, codes, constraints, calendars and date numbering. |
| 4D, critical path | None: the critical path method is the crate's own definition | n/a | The definitions are in the schedule README. | Forward and backward passes, float, criticality on total float or longest path. |
| Earned value, construction | None held: PMI *Practice Standard for Earned Value Management* and PMBOK were not available | n/a | The EAC and TCPI formulae are the ones the progress README writes. If your contract defines a quantity differently, check it first. | Budget at completion, planned value, earned value, EAC, TCPI. |
| 5D, cost | None: no rate, percentage, standard bill or contract clause is built in | n/a | The MoWT General Specifications numbering was not offered; the interim payment encodes no FIDIC clause. | Nothing. |
| Speed band | The thresholds of each check: CD 377 (50 mph), TSM Chapter 5 (40, 50, 60, 70 mph), TSM visibility tables (20 to 60 mph) | As above | The rule for a km/h limit is the project's setting. The crate holds no rule of its own. | How a km/h limit maps to a mph band. |
| DXF and sheets | DXF R12 (ASCII) and PDF output, Helvetica metrics for text | n/a | The DXF writer matches the C# writer byte for byte on the reference files. | Output formats only. |
| Fonts | The Transport fonts, `fonts/tranh___.TTF` and `fonts/tranm___.TTF` | n/a | Third-party files. The project README says not to redistribute them without a licence that allows it. | Sign lettering. |


## 11. Known slips in the source documents

These are errors, ambiguities or conflicts the crate authors found in the documents they read. AfroBIM either follows the value as printed, withholds the cell, or flags the point in a calculation line. The list is short; each crate README has the full record.

**MoWT Road Design Manual Volume 2, Drainage Design (2010)**

- Table 3.2 column headings read "PIa, PIb", "PIII" twice and no "PII"; the row limits leave exactly 2 m in neither row.
- Table 9.1's "roughness coefficient" (25 to 60) has no unit. It is read as a Strickler coefficient, so Manning's n is always entered.
- Equations 4.2 and 4.3 disagree with Table 4-2 by a factor of 3.28. The equations are not implemented.
- Tables 4.5 and 4.6 are both headed "Group I" with different constants, and one Nairobi 5-year value is above its 10-year value. Intensity is an input.
- Section 9.5.1 has a printed step that reads "(dc + D/2)" for "(dc + D)/2", a headwater read from a chart (0.85 m) where the equation gives 0.80 m, and a depth accepted before the trial converged.
- Gravity is printed as 9.8, 9.81 and 19.63 (2 × 9.815). Each is used where printed.

**HDS-5 (FHWA, 2012)**

- Section A.3.1 prints a result of 0.55 for a row where its own columns give 0.97.
- The example cites Chart 34, which is in the discontinued tables of Table A.2, not the charts Table A.1 names.
- Equation A.2 has no slope term, while the legend and Section 3.3.4 give one. The crate follows the printed form.

**SANRAL Drainage Manual (2013)**

- Table 7.2 gives no unit for S0. `(S0/0.4)^0.05` means 40 % or 0.4 % depending on the unit, a difference of about 20 % at a 0.15 % slope. The crate refuses to choose: the slope unit is a required argument.
- Table 7.2 is not continuous at H1/D = 1.2.
- H1 is a depth in SDM 8.3.2 and an energy level in Chapters 7 and 10. The crate treats it as the energy level.
- SAG Example 7.1 reads H1 = 2.4 m from Figure 7.4, where the table gives about 2.15 m for the same flow.
- SDM Equation 7.4 prints Kin for one velocity and Kout for the other; the example uses the barrel velocity for both.

**DMRB CD 521 and CG 501 (2025)**

- CD 521 Appendix B cites the wrong clauses or tables for its grass coefficients and minimum pipe velocities (B5, B6).
- The documents do not classify a side drain. The engineer chooses which criteria apply.

**MoWT Road Design Manual Volume 1, Geometric Design (2010)**

- Withheld cells, because the printed value contradicts another table of the manual: S01 (Table 4-4, Class Ib flat, with a hidden second set of figures in the text layer), S02 (crest K at 100 km/h printed 80 against 62), S03 (passing K 307 against 387), S04 (Class II mountainous gradients), S05 (Class C flat and rolling K values) and S19 (the 5 % downgrade column of Table 6.20).
- Others used as printed or as the formula gives: S06 (58 printed against 57.5), S10 (the lane width is 3.65 m in the text and 3.5 m in the notes; 3.5 m is used), S13 (A² = R − L printed against R × L in Figure 6-13; R × L is used), S14 (a 20° deflection with a 23° working), S23 (d3 printed for d4), S24 (a worked minimum radius with no answer printed). The full list of 25 items is in the design README.

**MoWT Volume 3, Part I, Flexible Pavements (2010)**

- Equation 1 is printed without brackets; the bracketed form is used, which reproduces Appendix D.
- Table 2.5 prints T2 as "0.3-07"; Table 2.4 puts 4.5 m, 6.0 m and 2 000 vehicles in two rows or none; Table 3.1 gives whole numbers only.
- Chart keys D2, W2, D5 and W5 give a cemented subbase strength of 1.5 to 3 MPa; Table 5.1 gives 0.75 to 1.5 MPa. A warning states the conflict.
- Appendix D places lime-treated subbase at class T4; Table 5.3 limits it to T2.
- Table 5.2 gives layer coefficients but no required structural number.

**SAPEM Chapter 10 and draft TRH4 (1996)**

- Table 22's printed ET (18 748 635) uses an AADE and a growth rate rounded in the example. Carrying them unrounded gives 18 752 360 with the same conclusion.
- Figure 20's terms total 2.09; unrounded they total 2.0875. p. 32 prints 2.4 for (100/80)⁴ = 2.44.
- Table 2 prints a range "< ES0.03 - ES3", read as ES0.003 to ES3.
- The chapter's TRH4 values were accepted without sight of TRH4, which was not readable.

**DMRB CD 127 and CD 365 (gantries)**

- CD 127 does not say whether to interpolate Table 4.3. The crate takes the smaller tabulated radius.
- CD 127 cl. 4.2 says "greater than", cl. 4.9 "equal to or greater than". The crate passes a headroom equal to the requirement.
- CD 365 r1 cl. 3.19 refers to the deflection allowance in Section 6, which gives limits, not an allowance. The "maximum height" of cl. 3.23 is not defined.

**DMRB TD 501 and CD 377 (lighting and restraint)**

- TD 501 cl. 3.15 prints "RG0 or RG01", read as RG0 or RG1. Cl. 6.53 measures mounting height above adjacent ground; cl. 1.4 above the road surface. One height is held.
- CD 377 steps at 50 mph (80.47 km/h). A posted 80 km/h is 49.7 mph and falls in the lower band. The report flags this for the road authority.
- "50 mph or more" (CD 377) and "50 mph or less" (CD 127 Note 1) overlap at exactly 50 mph; both apply.
- The MoWT Geometric Design Manual section 12.3 gives beams of "4130 m" (mm is meant), post spacing of 1900 and 3800 mm in the text and 1905 mm in its figures, and a flare "of at least 1 in 10" without saying which way. None is used in a calculation.

**BS 5489-1:2003 (superseded)**

- Table 2 gives clearance to the face of the column; the set-back is to the axis, so the face clearance is smaller by half the column width.
- Table B.2 gives "<15 000" and ">15 000" for strategic routes; exactly 15 000 has no entry, and the report asks.
- Table B.3 omits the a, b and c suffixes; footnotes b and c of Table B.2 and note 2 of Table B.4 are not applied.

**Stockmar, Handbook about Interpretation of EN 13201 (2017)**

- Its table numbers do not follow its chapter numbers, and it labels EN 13201-2:2015 as "Part 5" in its introduction.
- It words the C-class threshold increment as informative and recommends the lower-speed column; the report asks which column applies.
- The P3 semi-cylindrical value is printed "1,5" and read as 1.5.

**Traffic Signs Manual Chapter 5, Road Markings (2018)**

- Diagram "1049B (S9-67)" in paragraph 2.11.1 against S9-6-7 everywhere else.
- Paragraph 11.2.2 refers to "diagram 1023 (S9-6-4)", where the Regulations and paragraph 6.3.6 give 1023A.
- Figure 2-9 says three deflection arrows are spaced at 2 and 3 seconds at 70 mph and shows 64 m and 96 m; at 70 mph those are 62.6 m and 93.9 m. Table 2-6 values are used as printed.
- Figure 2-8 dimensions "800 min / 8000 max" and "400 min" without saying which edges they measure between. Figure 2-17's "1000" is not used.

**Uganda Traffic Signs Manual (2004)**

- Vol. 1 E.4 has "M44 ... left turn lane" (probably "or"). E.6.2 is headed "Purpose and Use of Road Markings" but covers road studs.
- M04 does not say which side of the pair the broken line is on. M43 carries seven dimensions without saying which is the overall width. M32's 300 and 600 are not labelled mark and gap. M53's taper table has a symbol the extraction reads as `#`, probably "less than or equal".
- M21 and M23 do not say which edge a distance is to; the near edge is taken.

**Traffic Signs Manual Chapter 2 worked example (sign visibility)**

- The official extract takes 50 mph as 22 m/s (22.35 exact), so R is 1.6 % short; "22.8, say 23" rounds up where the speed was rounded down. With exact values the example gives 186.82 mm, not 185 mm.
- The variant copy's iteration 1 prints 112.09 for 112.10; iteration 4 prints S = 4.96 but multiplies 4.95; its final "195 mm" rounds a requirement of 195.88 down.

**EN 13032 and the IES and EULUMDAT formats**

- Not held as standard text. The format readings are from the sources the lighting README cites. Test files are real luminaire files, read against their stated light output ratios.


## 12. What has not been verified

Stated plainly, so nothing here is read as more than it is.

- No file written by AfroBIM has been opened in Civil 3D, AutoCAD, a commercial BIM viewer, a BCF tool, MS Project or Primavera P6. The IFC export was checked with IfcOpenShell, the BCF output against the BCF 2.1 schemas, and the exchange files by reading them back.
- Photometry has not been validated against DIALux, Relux or AGi32, none of which was available. It is not an EN 13201-3 calculation: its grid and observer rules, threshold increment, longitudinal and wet uniformity are not held or computed.
- Mouse-driven editing (grips, insert, delete, picking, profile drag, the plan editor) has been exercised only through its code paths and the functions the pointer events call. A person has not driven it with a mouse. Windows have been looked at as headless screenshots made without a pointer.
- On 2026-11-15 the installer workflow (`.github/workflows/installers.yml`) built the macOS `.dmg` (universal) and the Windows `.msi` from `main`. The Windows job installed and uninstalled the `.msi` on the build machine. The owner installed the `.dmg` on an Apple silicon Mac, where the app started and the traffic signs module loaded. The `.msi` has not been run on a real Windows desktop, and the installed app has not been tried on an Intel Mac or on another macOS version. Both installers are unsigned with a placeholder icon (installing.md).
- The Windows executable icon and version information (`build.rs`, `embed-resource`) were written without a Windows machine and are unchecked until the workflow next runs. Nothing else in the app has been built or run on Windows; the checks and screenshots here are from Linux debug builds. Release builds were not run for the visibility crate, by rule.
- Dropping a file on the window, and the Finder limit described in installing.md, were checked as the function that sorts files by extension and the Linux window; the drop itself was not driven.
- The near-plane clipping of the 3D view, the press-drag move with object snap and the status-bar layout were checked through their functions and headless screenshots, not with a pointer.
- The LandXML import and the alignment export have not been tested against a Civil 3D export, beyond the one alignment file noted in section 6.8. The survey surface import is tested only on hand-built files. The civil coordinate functions are tested as functions, not with a pointer.
- The viewpoint snapshots, the reference-model drawing, the visibility overlay and the underlay drawing were looked at in headless screenshots only.
- Several standards are held only as superseded, secondary or third-party copies, or not held at all. Section 10 lists each one. The report pack repeats those warnings in its design basis.


## 13. Planned, not built

**On the ribbon.** One tool is shown as a disabled "Planned" button: **Make an imported alignment editable**, on the Design tab, Exchange panel. The alignment engine holds an imported alignment as elements (lines, arcs and spirals), not as PIs with radii, and no conversion gives back a faithful PI design. The card says so.

**From the round 5 plan (2026-11-16)**, not built: pipe networks, grading objects, intersections and roundabout wizards, survey database and traverse adjustment beyond the compass and transit rules, point clouds, data shortcuts, reprojection between coordinate systems, code signing.

**From the round 4 plan (2026-11-15).**

- Excluded by decision: structural design of gantries and lighting columns. AfroBIM checks geometry and requirements only.
- Not in this round: code signing and licensing for a product for sale; imperial units; EN 13201-3 photometry; BCF 3.0; IFC boolean geometry.
- Still needed from the owner, as the plan lists it: a working GitHub Actions run (the start condition for installer work); current DMRB CD 224, CD 226 and CD 365; BS 5489-1:2020; BS EN 13201-2 and -3; PD 6547 Table 2; BS EN 40-2 Table 7; a MoWT wind map; a sample of an existing sign schedule to test the column mapping against; and a trial of the exported IFC, sheet PDF, LandXML, MS Project and P6 files in the real software. The MoWT Volume 1 manual is now held (the design README).

**From the roadmap (2026-11-14).**

- S2, site records for inspections, photographs and non-conformances linked to elements. Site records exist (8.13), but inspections and non-conformances are not in the code, and photographs are paths only.
- Carried over from the roadmap: the South African pavement basis stops at SAPEM, because the TRH4 catalogue was not readable; the SANRAL drainage basis is built; the superseded DMRB revisions in use are to be replaced when the current PDFs are supplied.

**Not done, from the crate READMEs.** Each chapter in section 8 ends with its own Limits list, and those lists name what is not done in each engine. The most material are: resources, productivities and levelling in 4D; forecast S-curves and claims in construction; rigid pavement, overlay and mechanistic pavement design; box culverts, headwalls, scour and subsurface drainage; the restraint parapet, crash cushions and pedestrian restraint; the photometry checks of EN 13201-3; the Uganda curved marking arrows.


## 14. Troubleshooting

**The signs module is missing: the Signs tab is disabled and the status bar reads "Traffic signs unavailable".**

The status bar gives the reason after the colon. The app looks for the rule pack and the fonts, and reports which is missing.

- `rule pack packs/UK-TSM-2019.json was not found`: the UK pack is not where the app looks. Run the app from inside the checkout, put `packs/` next to the executable, or set `AFROBIM_PACK` to the JSON file.
- `rule pack packs/UG-TSM-2004/pack.xml was not found`: the Uganda pack is missing. Run from the checkout, or set `AFROBIM_PACK_UG` to the pack's folder or to its `pack.xml`.
- `the fonts folder was not found (set AFROBIM_FONTS to the folder holding the Transport fonts)`: set `AFROBIM_FONTS` to the folder holding `tranh___.TTF` and `tranm___.TTF`.
- `Could not load <standard>`: the standard named in `--standard` or chosen in Tools → Design standard did not load. The reason follows.

The lookup order is the same for each file: the environment variable, if it names something that exists; then the folder next to the executable; on macOS, the bundle's `Contents/Resources`; then every folder above the executable, and every folder above the working directory, for the relative path. A checkout run from inside `afrobim` finds `packs/` and `fonts/` one folder up. A macOS build with its data in `Contents/Resources` is found without a launcher (the `--launcher` option of the packaging script is optional, and only for a build that predates this lookup).

**A station is ambiguous under station equations.**

On an alignment with station equations, a displayed station can occur in more than one stationing region, for example where the stations overlap. The message is refused until you pick the region. In the dialog, set the **Stationing region** to the number of the region the station is in (1 to the number of regions). Enter 0 only when the station occurs once. The dialog shows the converted internal station beside the field (for example `1+550.000, internal station 1650.000 m`), so you can check it. The Stationing region control is shown only when the alignment has station equations.

**A check reads "Input needed".**

The model-wide check (`CHECKALL`) names the dialog to open in the row for each element that needs an input. Open that dialog on the element and fill the fields it names. The common ones are:

- a drain or culvert: `CHECK` (design flow and basis, and the culvert inputs);
- a sign's supports: `DESIGN` (the ground and wind inputs);
- a sign's visibility: `VISIBILITY` (the inputs, with the eye height accepted);
- a lighting installation: `CHECK` (the lighting inputs, and the maintained photometric results of a calculation made elsewhere);
- a barrier run: `CHECK` (the speed limit);
- a gantry: the deflection in Properties;
- an alignment: `ALIGNCHECK` (a limits file, or the built-in MoWT basis).

A field left empty stops the checks that need it. The inputs are kept on the element, so `CHECKALL` repeats them after you run it again.

**An underlay appears far from the model.**

The 2D canvas is the sign sheet layout, while alignments and their survey coordinates sit in site coordinates. A DXF drawn in survey coordinates therefore appears far from the signs. Open the underlay's window (`UNDERLAY`) and either set the **insertion offset** and rotation to bring the drawing into the model, or use **Zoom to it** to find it. Check the file's units too: the window shows the file's units and the millimetres per unit, which can be overridden. A DWG file is refused; convert it to DXF first.

**An analysis is slow.**

- These run on a worker thread, with a progress window and a Cancel button: the clash run, the report pack, the sign visibility run (`VISIBILITY` and `VISIBILITYRUN`, for one sign or the road, including the scene build), the model-wide check `CHECKALL` (including its visibility scene), the corridor builds a clash run's scene needs, composing a sheet set for the preview, the PDF or the DXF (a set with cross-section sheets builds its corridor), a photometry run, the rebuild of corridor surfaces for the 3D view after an edit, and the build of a corridor for the cross-section window. Cancel discards the result. The result is applied only when the document is unchanged; otherwise it is discarded and the status line says so (a visibility or photometry run, and the settings a photometry run stores, are applied only then; a corridor whose surfaces were discarded is built again from the edited data, and one you cancelled waits for your next edit). The model stays editable while a job runs, but an edit discards the result; the 3D view shows a corridor without its surfaces until they are built.
- Still on the interface thread, so the window can stop responding while they work: the export commands, and headless runs (`--export-set`, `--export-pack` and the like). A corridor rebuild on a large model is slow, because the corridors are rebuilt from their definitions.
- A clash test with a large selection on both sides takes longer. Narrow the selections by category and layer.
- A large surface (100 000 points, or a long pavement layer) takes longer in clash tests and in the 3D view.

**Other messages.**

- `Unknown command "name". Type ? for the list.`: the typed name is not a command or an alias. Type `?`.
- `Not checked: ...` on the status line: the check cannot run; the reason follows.
- A ribbon button is disabled: either the tool is "Planned" (section 13), or it needs the sign module (the status bar says why), or it needs an element of a kind that is not selected.

**A civil object or import does not line up with the survey.**

Check the project coordinate system first (section 4.1, the Settings tab of the toolspace). Objects keep the coordinates they came with, and nothing is reprojected. A file in another system will not line up until it is converted in a survey tool. The import dialog's check (section 4.5) says whether the file's system differs from the project's. If the check reports undeclared coordinates, check the source of the file before you import it.

**SECTIONS does not show the civil corridor.**

`SECTIONS` reads the older corridor element (`CORRIDOR`). A corridor made with `CORRIDORMODEL` is shown by its sample lines and `SECTIONVIEWS` (section 6.5).


## 15. Notes for the maintainer

The round 4 polish corrected the READMEs that disagreed with the code: the workspace README (alignment design limits, culvert inlet control, lighting check inputs, units, background work, object snap), the READMEs of `afrobim-visibility`, `afrobim-design`, `afrobim-lighting`, `afrobim-markings` and the comment in `afrobim-drainage` about the drain's gradient; the three crates that had no README (`afrobim-signs`, `afrobim-draw`, `afrobim-app`) now have one; and the wording about `layout.env` in `installing.md` was made plain. What remains below is still true of the code as built. Where the manual and a README disagree again, the manual follows the code.

- **Installer launcher.** `installing.md` now describes the macOS `--launcher` option as optional. The workflow input `macos_launcher` and the flag of `build-dmg.sh` remain, as the code does.
- **Roadmap link.** The workspace README points to `docs/plan/2026-11-13-afrobim-platform.md`. That file exists, and so does the roadmap of 2026-11-14 this manual uses.
- **Station conventions.** Every road dialog takes the station shown on the drawings and converts it through `station_input.rs`. The pavement design's "make a pavement" step and the cross-section window have no station field of their own.
- **Shortcut labels.** Each menu item shows the text `shortcut()` in `ribbon.rs` holds (for example Delete for Erase and Ctrl+K for the palette). An item with no shortcut shows its command name at the right. The manual follows the same rule.
- **Command names for sheets.** `SHEETS` with its alias `FABRICATION` is the fabrication sheets, one per sign. The drawing sheets use `SHEETSET`, `SHEETPREVIEW`, `TITLEBLOCK`, `PROJECTINFO`, `SHEETPDF` and `SHEETDXF`.
- **Windows.** Every window is made by `win::new` in `win.rs`, which keeps it inside the central area recorded by the canvas each frame. New windows should be made the same way.
- **Ribbon and menus.** The ribbon has eighteen tabs and the menu bar ten menus: File, Edit, View, Insert, Civil, Modify, Analyse, Tools, Window and Help. The menus are not a separate command set. Every item runs a command from the command table. The Signs tab is built from the sign templates of the active standard.
- **Command table.** `commands.rs` holds 205 commands. The reference table in section 9 holds all 205, grouped by ribbon tab. Every one is on a ribbon tab, or is a sign template that the Signs tab and the Insert menu show. The earlier edition of this manual said 169, which predated the civil commands.
- **Speed band.** `projset.rs` shows the rule and its examples. The rule's thresholds belong to each check, as the workspace README says; the rule holds none.
- **Corridor objects.** The corridor crate's README says its new corridor objects are not a kernel document element, and are meant to be held by the caller. The application stores the civil corridor model as a civil element, with its data serialised, so the README describes the crate, not the application.
- **Coordinate system wiring.** The project coordinate system is stored with the project settings (`project_crs`). A file without the key reads as having no system. The import checks (section 4.5) only read; nothing is moved.
- **Appearance and workspace.** The look, the scale and the workspace are kept in the preferences. A new workspace is one more entry in `WORKSPACES` in `workspace.rs`, and it is listed in the switcher automatically.
