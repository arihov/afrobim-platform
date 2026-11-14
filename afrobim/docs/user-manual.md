# AfroBIM user manual

This manual describes the AfroBIM desktop application and the crates it uses, as they stand in the `afrobim/` workspace (workspace version 0.1.0), updated for the Coordination, 4D and 5D tabs, the station convention of the road dialogs and the street lighting check inputs. Every feature, value and clause here comes from the source files. Where two sources disagree, the code wins, and the disagreement is listed in "Notes for the maintainer" at the end.

## Contents

1. What AfroBIM is
2. Installing and running from source
3. The screen
4. Working with commands
5. Chapters by area: traffic signs, supports, alignments, road markings, drainage, gantries, exports, Python and MCP, the other road modules, stations and the lighting check inputs, Coordination, 4D and 5D
6. Built but not yet in the app
7. Command reference
8. Planned, not built yet
9. Standards and editions
10. Notes for the maintainer

## 1. What AfroBIM is

1. AfroBIM is a desktop application for roads and infrastructure, written in Rust.
2. It lays out UK and Uganda traffic signs, places them on road alignments, and adds posts, foundations, road markings, drains, culverts and sign gantries.
3. It exports DXF drawings, IFC 4.3 models, quantity schedules and fabrication sheets.
4. A Python package and an MCP server let scripts and AI assistants build the same documents.
5. The C# SignCore application remains the reference for signs.

**What AfroBIM is not.** AfroBIM produces calculations and models for an engineer to check. It does not produce certified designs. A passed check does not show that a sign, post, gantry, drain or pavement is adequate. Where a value is not in a held source, AfroBIM leaves it as an input for the engineer. It does not assume one.

**Status.** Signs, supports, alignments, road markings, drainage, gantries and exports are built into the app. Four further modules are built as crates but are not yet in the app (section 6).

## 2. Installing and running from source

### Requirements

- Rust 1.85 or later.
- The traffic signs module needs the rule pack `packs/UK-TSM-2019.json` and the Transport fonts in `fonts/`.
- The fonts are third-party files and are not part of AfroBIM. Do not redistribute them without a licence that allows it.

### Run the app

1. Open a terminal in the `afrobim` folder of the checkout.
2. Run `cargo run -p afrobim-app`. The README suggests `cargo run --release -p afrobim-app` for a faster build.
3. To start with sample signs, add `--demo`: `cargo run -p afrobim-app -- --demo`.

Tests run with `cargo test` from the `afrobim` folder. They cover the kernel, the sign engine (golden tests) and the app.

### Finding the rule pack and fonts

The app looks for `packs/UK-TSM-2019.json` and `fonts/` from the checkout. If it cannot find them, set `AFROBIM_PACK` to the JSON file and `AFROBIM_FONTS` to the fonts folder. Without them the Signs tab and the sign buttons are disabled, and the status bar says why.

### Platforms

The codebase builds for Windows, macOS and Linux. The desktop shell has been checked on Linux with headless screenshots. It has not yet been run on Windows or macOS.

### Command-line options

These options come from `main.rs`. Any option that is not listed is ignored. A file name given without `--` opens that document.

| Option | Effect |
|---|---|
| `file.afrobim` | Opens this document at start-up. |
| `--demo` | Starts with sample signs. |
| `--design` | With `--demo`, sizes the demo supports for wind (MoWT open terrain) and shows the first calculation. |
| `--tab NAME` | Starts on this ribbon tab, for example `Roads`. The name is not case sensitive. |
| `--preview COMMAND` | Shows the preview card of the first ribbon button that runs this command. Used to check the card without a pointer. |
| `--3d` | Starts in the 3D view. |
| `--standard ID` | Starts with this design standard, for example `UG-TSM-2004`. |
| `--landxml FILE` | Imports the alignments of this LandXML file at start-up. With `--demo`, the demo signs that have supports are placed along the first alignment. |
| `--focus MARK` | Frames the 3D view on the element with this mark. |
| `--export-ifc FILE` | Writes the starting model as IFC to this file, then exits. |
| `--export-sheets FOLDER` | Writes quantities and fabrication sheets for the starting model into this folder, then exits. |
| `--run COMMAND` | Runs this command once at start-up, on the element named by `--focus` when there is one, to look at a dialog or window without a pointer. Several commands may be given, separated by commas. |
| `--date YYYY-MM-DD` | Shows the 3D view at this date of the programme (the 4D date). |
| `--subtab N` | The sub-tab (from 0) the Bill and Estimate windows open on. With `--demo` and `--run CERTIFICATE`, 1 also fills the certificate from 50 percent complete (a demo value). |
| `--zoom-clash N` | Zooms to the Nth clash on record (from 0) in the 3D view. |
| `--screenshot FILE.png` | Renders a few frames, saves the window to this file and exits. This is how the interface is checked on a machine without a desktop. |

## 3. The screen

From top to bottom, the window holds the menu bar, the ribbon, and three panels with the drawing area between them.

### Menu bar

The menus repeat the commands on the ribbon.

- **File**: New, Open, Save, Save as, Import LandXML alignments, Export DXF, Export IFC 4.3, Export quantities (BoQ), Export fabrication sheets, Quit.
- **Edit**: Undo, Redo, Copy, Paste, Erase, Select all.
- **View**: zoom commands, Grid, Snap, Ortho, 2D model space or 3D view, Real or Outlined display, dark or light model space and theme.
- **Insert**: the sign templates for the active standard, and Standard signs.
- **Annotate**: Show or hide element marks.
- **Format**: the design standard (UK or Uganda). The active one has a dot beside it.
- **Modify**: Place the sign on an alignment, Add supports to the sign, Design the sign's supports, Remove the sign's supports, Move, Copy, Erase, Arrange.
- **Manage**: the design standard, theme and model space colour.
- **Help**: Command list, About AfroBIM.

### Ribbon

The ribbon has fifteen tabs: Home, Edit, Insert, Annotate, Modify, Signs, Roads, Structures, Coordination, 4D, 5D, Output, Manage, View and Help. Each tab is a row of panels. Each panel holds icon buttons. Every button runs a command from the same command table as the menus and the command line. Hover over a button to see its preview card (section 4).

Only the built buttons are listed here. Buttons for tools that are not built yet are disabled, and section 8 lists them.

- **Home**: File (New, Open, Save, Save as, Undo, Redo); Signs (Direction sign, Standard signs); Modify (Move, Copy, Erase, Arrange); Supports (Add supports, Design supports); Alignment (Place by station); Navigate (Zoom extents, Zoom window, 2D / 3D); Drafting aids (Grid, Snap, Ortho).
- **Edit**: History (Undo, Redo); Clipboard (Copy, Paste); Selection (Select all, Erase).
- **Insert**: Signs (Standard signs); Import (LandXML alignments, AfroBIM document).
- **Annotate**: Marks (Element marks).
- **Modify**: Modify (Move, Copy, Erase, Arrange); Supports (Add supports, Design supports, Remove supports); Alignment (Place by station); Drafting aids.
- **Signs**: sign buttons for the active standard, in two panels (direction signs; map-type and temporary); Catalogue (Standard signs); Design standard (UK TSM, Uganda TSM); Fabrication (Quantities, Fabrication sheets). The sign buttons are disabled when the rule pack is not loaded.
- **Roads**: Alignment (Import LandXML, Place by station); Markings (Road markings); Drainage (Side drain, Pipe culvert, Check); Lighting (Street lighting, Light column); Restraint (Road restraint); Pavement (Pavement, Pavement design); Earthworks (Survey surface, Corridor, Sections).
- **Structures**: Supports (Add supports, Design supports, Remove supports); Gantries (Portal gantry, Cantilever gantry, Hang sign, Take sign off, Check).
- **Coordination**: Tests (Clash tests); Run (Run tests, Results); Display (Clash markers); Report (Export clashes).
- **4D**: Programme (Activities, Calendar, Schedule); Chart (Gantt chart); Baseline (Set baseline, Variance); Progress (Progress); Links (Link elements, Link report); Date (4D date); MS Project (Import, Export, Reports).
- **5D**: Bill (Bill and rules); Rates (Rate library, Currency); Estimate (Estimate); Time (Cash flow); Payment (Interim certificate); Export (Cost reports).
- **Output**: Export (DXF, IFC 4.3, Quantities, Fabrication sheets); Document (Save, Save as, Quit).
- **Manage**: Design standard (UK TSM, Uganda TSM); Appearance (Theme, Canvas colour).
- **View**: Zoom (Extents, Window, In, Out); Views (2D / 3D); Display (Real, Outlined, Element marks); Drafting aids; Appearance (Theme, Canvas colour).
- **Help**: Help (Command list, About AfroBIM).

### Tool Sets panel (left)

The left panel is headed TOOL SETS. It holds:

- the traffic sign buttons for the active standard, and Standard signs;
- Modify buttons (Move, Copy, Erase, Arrange) and Zoom buttons (Extents, Window). Hover over a button to see its command.

Below these, under MODEL, is the model browser. It lists every element, grouped by module with a count in each heading. Click a name to select that element.

### Properties panel (right, top)

The Properties panel shows the selected element. It shows the last element picked when several are selected.

- **Mark**: the element's name. You can edit it.
- **Category** and **Layer**. Choose a layer from the list.
- **Position X** and **Position Y** in millimetres. Drag them or type them.
- **Size**: width by height in millimetres. It is read only.
- **Definition**: the module's own fields. For a sign, these are the sign fields (section 5.1). For a post, foundation, drain, culvert or gantry, these are its dimensions and inputs, labelled from the stored data.
- **Derived**: values the module calculates. These cannot be edited.
- **Checks**: notes from the module, coloured by severity. Red is an error, amber a warning, grey information.
- **Property set**: free key and value pairs, such as Material. Remove a pair with the x button. Add a new pair with the box and the Add button.

When a support's dimensions are placeholders, the Properties panel says so in amber. The note goes when the dimensions are edited.

### Layers panel (right, below Properties)

The LAYERS list shows each layer with three controls. The tick sets visibility. Lock stops elements on the layer being selected. Locked layers cannot be selected. The Show element marks box at the bottom shows or hides the mark under each element.

### Command line (bottom)

The bottom panel shows the last three lines of the command history, a prompt and an input box. The prompt reads `Command:`, or it shows what the running command needs next, for example a base point. The hint in the box reads `? commands`, `Enter repeats` and `Esc cancels`.

### Status bar (bottom)

From left to right, the status bar shows:

- the cursor position in millimetres;
- the GRID, SNAP and ORTHO toggles (F7, F9 and F8);
- the 2D and 3D switch;
- the Real and Outlined display choice;
- the latest status message;
- on the right: the module status message (for example, why the sign tools are unavailable), the scale as "1 px = x mm", and the selection (No selection, the name and size of one element, or the number of elements selected).

### 2D model space (centre)

Model space is the drawing area. The drawing is in millimetres, with Y up.

- Click an element to select it. Click more elements to add them to the selection. Shift and click removes one.
- Drag on empty space to make a window or a crossing. Drag from left to right to take elements that lie wholly inside. Drag from right to left to take elements that the box touches.
- Move by dragging a selected element or one of its grips, or type `M`.
- Copy with `CO`: a base point, then a point for each copy, then Enter. Ctrl+C and Ctrl+V also work.
- Erase with the Delete key or `E`.
- Zoom with the mouse wheel, which zooms about the cursor. Middle-click and drag pans. `Z` zooms to the extents and `ZW` zooms to a window.
- Undo with Ctrl+Z and redo with Ctrl+Y.
- F7 shows the grid. F9 snaps picked points to a 50 mm grid. F8 holds moves to the X or Y axis.

### 3D site view (centre, when 3D is on)

Switch to the 3D view with View → 2D / 3D, the `3D` command or the 3D switch in the status bar. Drag to orbit. Shift-drag or middle-drag pans. The wheel zooms. Double-click fits the view. The 3D view is for looking only. Editing stays in 2D.

The site view shows:

- alignment centrelines, drawn at the profile's level where the alignment has a profile;
- signs at their stations, facing their traffic, on their posts and foundations;
- other elements with solids and a site placement, such as posts, foundations, drains, culverts and gantries, in their site positions.

Road marking lines are drawn at the alignment's profile level.

## 4. Working with commands

**Typing a command.** Click the command line, or type over the drawing. The command line takes the keyboard when nothing else has focus. Type a command name or one of its aliases and press Enter. Case does not matter, and spaces around the name are ignored. An unknown name gives the message `Unknown command "name". Type ? for the list.`

**Listing commands.** Type `?` and press Enter, or use Help → Command list. The list window also shows how to type points (see below).

**Enter repeats.** Press Enter on an empty command line to repeat the last command.

**Esc cancels.** Esc cancels the command in progress. When no command runs, Esc clears the selection. Esc in a Properties field belongs to that field, unless a command is running.

**Points.** A command that waits for a point takes:

- `x,y` for an absolute point;
- `@dx,dy` for a relative point;
- `@distance<angle` for a polar point.

All values are in millimetres.

**Preview cards.** Hover over a ribbon button to see its card. The card shows the button's icon and label, what the command does, the command name and its aliases, the shortcut if there is one, and the line `Click, or type the command and press Enter. Esc cancels.` A button for a tool that is not built yet is disabled, and its card says so.

**Shortcuts.** The shortcuts are F7 (grid), F8 (ortho), F9 (snap), Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+Shift+S, Ctrl+Z, Ctrl+Y, Ctrl+C, Ctrl+V, Ctrl+A and Delete. Section 7 gives the full command table.

## 5. Chapters by area

Each area has the same parts: what it does, how to use it, what comes out, the standard and edition it follows, and its limits. The limits are copied from the crate READMEs and the status table in the workspace README.

### 5.1 Traffic signs

**What it does.** Places traffic signs in the model and lays each one out to the rule pack. The engine returns a size and diagnostics, and each diagnostic cites its clause. The UK pack has the direction, map-type and temporary signs, and the standard sign catalogue. The Uganda pack has its own catalogue. The UK pack is the default.

**How to use it.**

1. Choose the design standard. Use Format → UK or Uganda, the `UK` or `UGANDA` command, or the Design standard buttons on the Manage or Signs tab. Changing the standard lays every sign out again. A sign that the new standard cannot lay out keeps its place and shows the reason. The list of sign buttons changes with the standard.
2. Insert a sign. Use a button on the Signs tab or in Tool Sets, the Insert menu, or type one of these commands: `STACK`, `DIRECTION`, `CONFIRM`, `PLACE`, `LANE`, `DIVERSION`, `MAP`, `TEE`, `ROUNDABOUT`. The sign hangs from the cursor. Click to place it.
3. Insert a standard sign. Use Standard signs (`LIBRARY`). The window has a Find box that filters on the key or name. Entries are grouped by class. Click an entry to insert it, then click in the model to place it.
4. Edit the sign. Select it and use Properties. The fields are:
   - **Type**: the sign kind. It is read only.
   - **Variant**: Entry or Exit. This appears only on place identification signs.
   - **Design speed**: km/h, from 10 to 130.
   - **Approach class**: Primary, Secondary or Local.
   - **Background**: Green, Blue, White, Brown or Yellow.
   - **Destinations**: a list of destinations, each with a name, a direction and route fields. Use Add destination to add one. The Python API lists the destination fields as name, direction (ahead, left or right), distance, route number, route status (motorway, primary or non-primary), order, symbol and number status.
   - **Embedded signs**: warning triangles and regulatory roundels carried on a direction or map-type sign. The editor offers at most two.
   - **Standard signs without destinations** show Diagram or symbol (for example `504.1` or `670:30`), Plate legend (lines separated by `|`) or, for supplementary plates, Plate type and Legend, and a Temporary tick that gives a yellow ground.
5. Read the Checks section of Properties. It gives the engine's diagnostics.
6. Move or erase the sign with the commands in section 4.

**What comes out.** A sign element on the Signs layer, with its drawing, its width and height, its diagnostics and its definition. The sign then appears in DXF, IFC, quantity and fabrication output (section 5.7).

**Standard and edition.**

- UK: the rule pack `UK-TSM-2019`. The app labels it "UK Traffic Signs Manual (Chapter 7, TSRGD 2016)". The catalogue has the 24 TSRGD warning and regulatory diagrams with their plates, supplementary plates, information signs and the 66 destination symbols.
- Uganda: the folder pack `UG-TSM-2004`, labelled "Uganda Traffic Signs Manual (2004)". It is switched with Format → Uganda, or the `UGANDA` command.

**What was checked.** 444 signs were compared with the C# engine primitive by primitive. The worst deviation is about 3e-14 sw. The DXF output matches the C# writer byte for byte on the reference files. The quantities are byte-identical to the C# output. The Uganda pack has 16 sign kinds, 105 catalogue entries and 241 golden fixtures.

**Limits.**

- Embedded signs under the Uganda pack are not ported.
- Warning triangles and regulatory roundels embedded on direction and map-type signs are drawn as the standard frame only, as in the C# engine. The inner symbol is not drawn.
- Schedule CSV and XML import and export are not ported in the app. The MCP server reads schedule CSVs. Use the C# CLI for schedules.

### 5.2 Supports and support design

**What it does.** Adds posts and foundations to a sign (`SUPPORTS`), sizes them for wind (`DESIGN`) and removes them (`NOSUPPORTS`). It measures posts and foundations by their geometry. The design module sizes a steel CHS post and a planted or spread foundation for wind, and shows the calculation.

**How to use it.**

1. Add supports. Select one sign, then use Add supports (Home, Modify or Structures tab) or type `SUPPORTS`. The Supports dialog asks for:
   - **Mounting height (mm)**: from the ground to the sign's lower edge. It must be above zero. The box shows the hint "ground to the sign's lower edge".
   - **Number of posts**: 1 to 6. It starts at 2.
   - **Edge inset (mm)**: 0 to 5000. It starts at 150.

   Click Add supports. The posts and foundations are added with placeholder dimensions. Set the real dimensions in Properties.
2. Design the supports. Select the sign, then use Design supports or type `DESIGN`. The dialog asks for:
   - **Mounting height (mm)**, **Number of posts** (1 to 6) and **Edge inset (mm)**, as above.
   - **Basic wind pressure**: MoWT Vol. 4 Table 2 for open terrain, or for a built-up area; or **Entered** in kN/m², from 0.05 to 5.0.
   - **Foundation**: Planted, one per post; or Spread, one under the sign.

   For a planted foundation:
   - **Method**: PD 6547 (the concrete surround resists) or CD 354 rev 1 (the post alone resists).
   - **Foundation diameter (mm)**: 100 to 2000.
   - **Ground factor G (kN/m² per m)**: 1 to 2000.
   - **Minimum planting depth (mm)**: 100 to 3000.

   For a spread foundation:
   - **Assumed ground (Table 5.2)**: Poor, Average or Good.
   - **Breadth along the sign (mm)**: 300 to 10000.
   - **Thickness (mm)**: 200 to 3000.
   - **Top below ground (mm)**: 0 to 1000.

   Click Design. A calculation window opens. Lines that contain FAILS, or that start with Check, are shown in amber. The Copy button copies the whole calculation.
3. Remove supports with Remove supports (`NOSUPPORTS`).

A sign that hangs on a gantry takes no posts. The command says so and asks you to take the sign off the gantry first (`UNHANG`).

**What comes out.** Post elements (category `post`) and foundation elements (category `foundation`), each hosted by its sign. The quantities file gets posts, foundations and totals (section 5.7). The design gives a calculation window that you can copy. IFC export writes the posts as `IfcMember` and the foundations as `IfcFooting` (section 5.7).

**Standard and edition.**

- Method: IHE *Sign Structures Guide* 2021, applying BS EN 12899-1:2007 and its UK National Annex. Wind force is `Fw = cf · wb · Aref`.
- Factors: γF = 1.35 (ULS), 1.0 (SLS) and class PAF1; γm = 1.05 for steel; γf3 = 1.0.
- Member capacity: BS EN 1993-1-1 classification, plastic (class 1 and 2) or elastic (class 3) resistance. Class 4 is refused.
- Planted foundation: PD 6547 or CD 354 rev 1. The minimum planting depth comes from BS EN 40-2 Table 7, which is not held in the repository. The engineer enters it.
- Spread foundation: BS EN 1997-1, Design Approach 1, with the guide's assumed bearing resistances (Tables 5.2 and 5.3).
- Uganda wind: MoWT *Road Design Manual Volume 4: Bridge Design* (January 2010), Section 5.3, Table 2, with values of 0.81 kN/m² for open terrain and 0.37 kN/m² for built-up areas, up to 5 m. The module uses it as the sign's basic wind pressure. This is a reading of two documents that do not refer to each other.
- Verification: the tests reproduce Examples 1 and 2 of the guide's Appendix C. Two printed figures in the guide do not follow from its own expressions: 2.4.6 prints 28.17 mm where the expression gives 28.37 mm, and 2.5.5 prints 56.23 × 1.50 = 83.34 where the product is 84.34. The module follows the expressions and applies γG = 1.35, which the guide's Table 5.1 gives.

**Limits.** These are copied from the supports README and the workspace README.

- Inputs, not defaults: the ground factor G (only the guide's value for poor ground, 230, is held) and the BS EN 40-2 Table 7 minimum planting depth. Neither table is in this repository.
- The MoWT bridge-manual velocity pressure used as the basic wind pressure is a reading of two documents that do not refer to each other. The manual's Figure 2 wind map is blank in the published PDF, so no regional variation is available.
- Spread foundations are geotechnical checks only. The concrete, its reinforcement and the fixing of the posts into the block are not designed. The assumed ground must be proved on site by the guide's inspection and DCP testing.
- Not covered: foundation concrete and reinforcement, slopes, wind funnelling and orography, wind on the posts, passively safe supports, eccentric signs, fixings, the sign plate, sections other than CHS, and AASHTO sections or other steel shapes.
- `COMMON_CHS_MM` is a convenience list of common nominal sizes, not a statement of what any supplier stocks.
- The model holds no steel grades, concrete classes, clearances, rates or costs. Material, finish and concrete are free text that AfroBIM never interprets.
- Caps, base plates, brackets, clips and fixings are not modelled or measured. Reinforcement, backfill and disposal are not measured. Concrete is not reduced for a post cast directly into the foundation, only for an explicit socket.
- The excavation is a plain vertical-sided prism with the working space given (default 0). There is no batter, bulking or overbreak.
- The output is a calculation for an engineer to check. It is not a certified design.

### 5.3 Alignments

**What it does.** Imports road centrelines from LandXML 1.2 files. An alignment holds tangents, circular curves and clothoid spirals; a vertical profile with PVIs and parabolic, asymmetric or circular curves; station equations; and stored superelevation. Signs are placed on an alignment by station, offset and side. The sign takes the profile's centreline level.

**How to use it.**

1. Import alignments. Use File → Import LandXML alignments, the `LANDXML` command, or the LandXML alignments button on the Insert tab or the Import LandXML button on the Roads tab. Choose an `.xml` file. The importer's notes go to the command history.
2. Place a sign. Select one sign, then use Place by station (`STATION`). The dialog asks for:
   - **Alignment**: choose from the alignments in the model.
   - **Station (m)**: the station as shown on the drawings. It is read through the station equations.
   - **Stationing region**: shown only when the alignment has station equations. Enter 1 to the number of regions, or 0 when the station occurs only once.
   - **Offset from centreline (m)**: a distance from 0 to 200. It is always positive.
   - **Side, looking up-station**: Left or Right.

   The dialog starts from the sign's own station and offset when it has them, and from 0 m and 3 m when it does not. Click Place. The status line gives the sign's mark, station, offset, side and, where the alignment has a profile, the centreline level. The sign's supports move with it.

The sign faces the traffic on its own side of the road. AfroBIM assumes traffic keeps left.

**What comes out.** An alignment element for each imported alignment. It is drawn in plan with station ticks, but without station labels. In the 3D view its centreline is drawn at the profile level. A placed sign gets a site placement (position, rotation, station and offset), and its supports are moved with it.

**Standard and edition.** LandXML 1.2. Element and attribute names were checked against `LandXML-1.2.xsd`. Units are converted to metres. The sign placement rules are a port of the C# `SignPlacement` and `LocationReport`, which match the C# on 4,256 cases. No design standard is applied to the geometry. AfroBIM computes no superelevation rates.

**Limits.**

- None of the LandXML import has been tested against a file exported by Civil 3D. The test files were written by hand to the LandXML 1.2 schema.
- It is not known yet what Civil 3D writes. The open questions are which stations a profile uses when station equations exist, what `CircCurve@length` holds, and whether Civil 3D writes `Superelevation` in this form.
- Only the first `ProfAlign` of a profile is read. The others are named in a note. `ProfSurf` (existing ground) is not read.
- Superelevation is stored data only. No rate is computed from speed or radius. The crate holds no design tables and gives no runoff or runout lengths. No normal crown is assumed. Only what the data states is returned.
- The LandXML 1.2 schema gives six stations and one full rate. The full rate applies at the `FullSuperSta` and `RunoffSta` stations, as a plane that falls to the inside of the curve. The other four stations are stored with no cross-fall.
- The profile has no design checks: no K value, sight distance or limiting grade.
- Station equations: a gap is refused. An overlap is ambiguous, so a displayed station that occurs twice is refused with the list of candidates.
- Placing by station: the cross-fall adjustment is an approximation that holds only within the carriageway. The app does not use it. The sign goes at the centreline level.
- The plan drawing has no station labels. Its ticks are at internal stations, and it ignores the profile, superelevation and station equations.
- Only clothoid spirals are supported. Other spiral types (Bloss, cubic parabola, sinusoid) are not. LandXML `IrregularLine` and `Chain` are not supported.
- LandXML is not written.
- Cross-sections, cant and existing-ground profiles are not supported. There is no ground level away from the centreline.
- An alignment whose horizontal geometry is unsupported is left out with a note. The rest of the file is still imported.
- Superelevation design is not covered: no rates are computed, and LandXML states the cross-fall only at full superelevation.

### 5.4 Road markings

**What it does.** Lays longitudinal road marking lines along an alignment to the Traffic Signs Manual, Chapter 5 (2018). It covers centre lines, lane lines, warning lines, edge lines and double white lines. It draws them in plan, shows them in the 3D view, checks them against the manual's tables and measures them.

**How to use it.**

1. Open the dialog with Roads → Markings → Road markings, or type `MARKINGS`. An alignment must already exist. Otherwise the status line says to import one first.
2. The dialog opens with the first catalogue line, a speed limit of 30 mph, an offset of 0 m, and the alignment's full length. It asks for:
   - **Alignment**: the alignment to follow.
   - **Line type**: the catalogue entry. Each entry shows its name, its diagram and its speed band.
   - **Pattern**: read only. It gives the mark and gap and the manual's citation for the chosen line type.
   - **Speed limit (mph)**: 5 to 70, with its km/h equivalent beside it.
   - **Width (mm)**: From the table, or one of the prescribed widths for the line type.
   - **Start station (m)** and **End station (m)**: internal stations.
   - **Offset from the alignment (m)**: 0 to 50, with Left or Right. The offset is to the centre of the line, or to the middle of a double line.
   - **Material**: free text. The box shows "not stated" until you type.
3. Click Lay the line. The dialog stays open so that you can lay several lines. Click Close when you finish. The first four notes go to the command history, and the status line says how many notes there are.
4. For quantities, use Export quantities. It writes `markings.csv` and `markings.summary.csv` beside the file you choose.

The manual gives the speed limits in mph. Mapping a km/h limit to a band is the engineer's decision.

**What comes out.** A marking element on the Road markings layer, with its plan line and its 3D line. Its quantities are the overall length, the number of whole marks, the number of part marks, the painted length and the painted area. The schedule has one row per marking and a total.

**Standard and edition.** Traffic Signs Manual, Chapter 5: Road Markings, 2018 (the current edition). The 2003 edition is superseded and was not used. TSRGD 2016 was not read directly for this crate. Each catalogue entry carries its figure, table and paragraph:

| Type (id) | Diagram (TSRGD) | Speed | Mark (mm) | Gap (mm) | Widths prescribed (mm) | Citation |
|---|---|---|---|---|---|---|
| Centre line (`1008`) | 1008 (S11-4-6) | 40 or less | 2000 | 4000 | 100, 150 | Figure 2-1, Table 2-1, 2.2.1 |
| Centre line (`1008.1`) | 1008.1 (S11-4-7) | over 40 | 3000 | 6000 | 100, 150 | Figure 2-1, Table 2-1, 2.2.1 |
| Lane line (`1005`) | 1005 (S11-4-4) | 40 or less | 1000 | 5000 | 100, 150 | Figure 2-3, Table 2-2 |
| Lane line (`1005.1`) | 1005.1 (S11-4-5) | over 40 | 2000 | 7000 | 100, 150 | Figure 2-3, Table 2-2 |
| Warning line (`1004`) | 1004 (S11-4-2) | 40 or less | 4000 | 2000 | 100, 150 | Figure 2-4, Table 2-3, 2.4.3, 2.7.1 |
| Warning line (`1004.1`) | 1004.1 (S11-4-3) | over 40 | 6000 | 3000 | 100, 150 | Figure 2-4, Table 2-3, 2.4.3, 2.7.1 |
| Edge line, short broken (`1009A`) | 1009A (S11-4-8) | any | 600 | 300 | 100, 150, 200 | Figure 2-6, Table 2-5, 2.5.3 |
| Edge line, broken (`1010`) | 1010 (S11-4-10) | any | 1000 | 1000 | 100, 150, 200, 250, 300 | Figure 2-6, Table 2-5, 2.5.4 |
| Edge line, continuous (`1012.1`) | 1012.1 (S11-4-11) | any | continuous | n/a | 100, 150, 200 | Figure 2-6, Table 2-5, 2.5.6 |
| Double white lines, both continuous (`1013.1A`) | 1013.1 (S9-6-23) | any | continuous and continuous | n/a | each line 100 minimum, 150 normal | Figure 3-1 version A, 3.2.1 |
| Double white lines, one broken (`1013.1D-L`, `1013.1D-R`) | 1013.1 (S9-6-23) | any | broken line 1000 | 5000 | each line 100 minimum, 150 normal | Figure 3-1 version D, 3.2.1 |

A double line is two strands, normally 175 mm apart, with a minimum of 90 mm. Both lines and the gap must not exceed 900 mm overall. All these types are white. `-L` and `-R` say which line is broken, looking up-station.

**Limits.** These are copied from the markings README and the workspace README.

- Horizontal geometry only. There is no profile, cross-fall or superelevation. Lengths are plan lengths.
- A marking has one line type, one width and one offset from end to end. Tapers, changes of width and lines that move across the carriageway need separate elements, or are not possible.
- At an angle point, where the elements do not meet tangentially, an offset line is not continuous. The strip bridges the joint. Smooth alignments are not affected.
- The catalogue checks a marking against the speed limit and the tables. It does not decide whether a marking is warranted. The visibility distances V and W of Table 2-4, the double white line design procedure of 3.9 and junction layouts are the engineer's judgement.
- The laying tolerances of Table 12-2 are not applied or checked.
- TSRGD 2016 was not read directly.
- Not covered: stop and give-way lines, arrows, worded markings, hatching, yellow lines, road studs, box junctions, and the Uganda manual's markings.
- Not done: raised profile edge lines (diagrams 1012.2 and 1012.3) and diagram 1009B; double white lines version B (hatched, up to 1200 mm) and the change-over version C; diagram 1013.5; splayed and inclined double lines; deflection arrows; transverse markings (Chapter 3); arrows, lane destinations and worded markings (sections 9 and 11); hatching and chevrons (diagrams 1040 to 1042); tapers (Tables 2-6 and 2-7); yellow lines and other waiting and loading restrictions; red routes; road studs; yellow box junctions, bar markings and traffic calming markings; cycle, bus and tram markings; junction and roundabout layouts; the Uganda manual's markings; rates and costs.

**Decisions the code makes, and what they mean for you.**

- The pattern starts at the start station with a full mark. The remainder at the end is cut square and kept as a part mark. With the End anchor, the pattern is laid backwards from the end station, so a line can finish on a full mark.
- Lane lines are drawn at 100 mm unless you choose 150 mm. At 70 mph an information note cites clause 2.3.5, which asks for 150 mm on dual carriageways and motorways.
- Tables 2-1, 2-3 and 2-5 give different widths by road layout. The marking has no road-layout field, so you choose the width. With no width set, the first row is drawn and a warning lists the others.
- Diagram 1009A has no width by speed. You must choose 100, 150 or 200 mm.
- Table 2-5 has columns for 40 or less, 50 or 60, and 70 mph. A limit between those values, such as 45 or 65 mph, has no column and raises a warning. 80 km/h is 49.7 mph and 100 km/h is 62.1 mph, and neither falls in a column. How km/h limits map to the UK bands is the engineer's decision.
- Diagrams 1008 and 1004 over 40 mph are allowed only under the notes to Table 2-1 and Table 2-3. The code catalogues them as exceptions and gives an information note.
- Table 2-3 has no three-lane row. Clause 2.4.7 says the 150 mm line is used. Choose 150 mm by hand.
- The minimum number of marks in Table 2-3 is given for each approach. The marking does not know if it is an approach, so a shortfall is an information note and not an error.
- Markings are white. Yellow on any type here raises a warning.
- A double line's offset is the middle of the pair. Clause 3.2.2 allows a second arrangement, which you get by setting the offset.
- The Northern Ireland rule (2.1.2, Appendix A) is not modelled.

### 5.5 Drainage

**What it does.** Adds side drains along an alignment and pipe culverts at a station. It measures their excavation, lining, pipe lengths and gradients, and it checks a side drain's capacity for a design flow that you enter, by Manning's equation. Two design bases are available: the MoWT Road Design Manual Volume 2 (2010) and the UK DMRB.

**How to use it.**

1. Add a drain or culvert. Use the Roads tab, Drainage panel, or type `DRAIN` (side drain) or `CULVERT` (pipe culvert). An alignment must exist. The Add dialog asks for:
   - **Alignment**.
   - **Start station (m)** and **End station (m)** for a side drain. A culvert or gantry asks for one **Station (m)**.
   - **Offset from the alignment (m)**, with Left or Right. This is asked for a side drain only.

   The stations are internal stations. The element is added with placeholder dimensions that are not design values. Select it, set the dimensions in Properties, then run `CHECK`.
2. Check a side drain. Select the drain and run `CHECK`, or use the Check button on the Roads tab. The dialog asks for:
   - **Design flow (m³/s)**: 0.001 to 500. It starts at 0.1.
   - **Basis**: MoWT Road Design Manual Vol. 2 (2010), or UK DMRB (CG 501, CD 521).

   The design flow is the engineer's. AfroBIM holds no rainfall data. The drain's gradient, lining and roughness come from Properties. Click Check. The calculation window shows the lines.
3. For quantities, use Export quantities. It writes `drains.csv`, `culverts.csv` and `gantries.csv` beside the file you choose, if the model has these elements.

`CHECK` in the app runs the side drain capacity check and the gantry checks. It does not run the culvert outlet-control calculation or the rational method, although the crate holds them.

**What comes out.**

- A drain as a closed trough swept along the route, with its lining. For an unlined drain, a 50 mm display skin is drawn, and it is in no quantity. The solid is level. The drainage README gives the reason as "the alignment has no vertical geometry yet". See Notes for the maintainer.
- A culvert as hollow pipes that fall from the inlet to the outlet.
- Quantities: drain length, excavated area and volume, lining area and volume, and culvert pipe lengths per barrel and in total.
- A calculation window for each check.

**Standard and edition.**

| Basis | Documents |
|---|---|
| MoWT | *Road Design Manual Volume 2: Drainage Design*, January 2010 (the DDM). Recurrence intervals (Table 3.2), the rational method (Section 4.4.2, Equation 4.10), Manning (Example 4-1, Section 9.5.1, Appendix 10.4), permissible velocities (Table 9.1), culvert outlet control (Section 9.5.1 and Equations 9.9 and 9.11), the headwater limit of 1.5 D (Section 9.3) and the minimum culvert diameter of 600 mm (Section 9.10). |
| UK DMRB | CG 501 v2.2.0 (September 2025), *Design of highway drainage systems*; CD 521 v1.2.0 (September 2025), *Hydraulic design of road edge surface water channels and outlets* (roughness, Table 5.18.1; Manning's equation, Equation 5.25); CD 522 v0.1.0 (September 2025), *Drainage of runoff from natural catchments*; CD 529 v1.1.0 (September 2025), *Design of outfall and culvert details* (minimum culvert diameter 450 mm, clause 3.5.1). |

Every number from these documents carries its section, table or clause beside it in the code. The worked examples in the DDM and CD 521 are reproduced in the tests.

**Limits.** These are copied from the drainage README and the workspace README.

- Uniform flow only. There is no backwater from culverts or outfalls, no drawdown, and no bends.
- Rainfall intensity is always an input, under both bases.
- Manning's n of a drain is always an input.
- The gradient of a drain is entered. Which way it falls is not modelled.
- A culvert's outlet-control headwater is half a culvert check. Inlet control exists in the DDM only as nomographs and is not implemented. The higher of the two governs, and every result says so.
- Layout notes cover only what the chosen basis' documents state. No note does not mean that no rule applies.
- Not in this cut: box and slab culverts and bridges; inlet-control nomographs, flow over the road, performance curves and the capacity tables (DDM Appendices 9.3 to 9.5, 9.19 to 9.21); headwalls, wingwalls, aprons and bedding; energy dissipators and scour; subsurface drainage; kerbs, gutters, inlets, storm sewers and gullies; DMRB outlets, gratings, weirs and combined channel and pipe systems; catchment delineation; the SCS, unit hydrograph and TRRL flood models; DDM Table 4.1 formulae Nos. 1 to 6; detention; costs.
- Under the DMRB basis there is no rainfall or runoff method and no culvert hydraulics. UK rainfall methods are not carried to other countries. CD 529 refers culvert hydraulics wholly to CIRIA C786, which is not held. CD 521 is written for shallow channels at the carriageway edge. Its roughness values for a deeper side drain are the crate's reading, not CD 521's. CD 532, the DMRB document for ditches, is not held.
- No South African value is held yet. A South African basis can be added beside the two.
- Not covered in the workspace status table: culvert inlet control, box culverts, headwalls, scour and subsurface drainage.

**Ambiguities in the documents.** The crate flags these and does not resolve them silently. Three matter most for use:

- The DDM's Table 9.1 "roughness coefficient" (25 to 60) has no unit. The crate reads the values as Strickler coefficients, so Manning's n is always entered.
- The DDM's velocity equations (4.2 and 4.3) disagree with its Table 4-2 by a factor of 3.28. They are not implemented.
- The DDM's Table 4.8 rainfall multipliers are never applied automatically. They are an explicit input.

### 5.6 Gantries

**What it does.** Models portal and cantilever sign gantries that carry signs over the carriageway. It sets them out on an alignment, hangs signs on them, measures them and runs headroom and listed-requirement checks.

**How to use it.**

1. Add a gantry. Use the Structures tab, Gantries panel, Portal gantry or Cantilever gantry, or type `GANTRY` or `CANTILEVER`. The dialog asks for an **Alignment** and a **Station (m)**, and it adds the gantry with placeholder dimensions. Set the dimensions in Properties.
2. Hang a sign. Select one sign, then use Hang sign (`HANG`). The dialog asks for:
   - **Gantry**: choose from the gantries in the model.
   - **Position**: either an **Offset from the centreline (m)** or an **Over lane number** from 1 to 12. Lanes are the ones entered on the gantry, numbered from 1 at the left.
   - **Faces traffic travelling**: Up-station or Down-station. This is asked when the position is an offset.
   - **Lower edge above the boom underside (mm)**. A negative value hangs the sign below the boom.

   Click Hang.
3. Take a sign off with Take sign off (`UNHANG`).
4. Check a gantry. Select it and run `CHECK` (Structures tab, Check). The calculation window gives the headroom and listed requirements. Enter the serviceability deflection in Properties first. Without it the headroom line reads INCOMPLETE.

**What comes out.** A gantry element with its legs, boom and foundation blocks as schematic solids, and its sheet elevation. Hosted signs follow the gantry when it moves. Quantities go to `gantries.csv`: the span or reach, leg and boom lengths, support height, sign area, and foundation concrete. Steel mass is given only when every member has a mass per metre entered.

**Standard and edition.**

| Document | Revision | Status |
|---|---|---|
| DMRB CD 127, *Cross-sections and headrooms* | Version 1.0.1, published 29 July 2021 | Headroom values (Table 4.1), sag compensation (Table 4.3) and the structure free zone. Its currency was judged only from the absence of a superseded mark and the DMRB index of 15 October 2023. Confirm it. |
| DMRB CD 365, *Portal and cantilever signs/signals gantries* | Revision 1, published March 2020 | **Watermarked SUPERSEDED.** The current version was not obtained. A third-party index lists Version 1.1.0 (September 2025) as its successor. That was not verified. Every clause here must be confirmed against the current version. |
| DMRB CD 377, *Requirements for road restraint systems* | Revision 4, published January 2021 | **Watermarked SUPERSEDED.** Read only. No value was taken from it. |

The checks cover:

- Headroom: the lowest point of boom, carried signs and entered equipment, less the highest surface under the gantry and the entered deflection, measured at right angles to the surface, against Table 4.1 plus the sag compensation (CD 127 clauses 4.1, 4.2, 4.3, 4.9 and 4.11; CD 365 Revision 1 clause 3.19). The report reads OK, FAILS or INCOMPLETE.
- The distance from the edge of the paved width to each leg's near and far face (CD 127 clauses 4.5 and 4.6). It is reported without a pass or fail.
- For a dual carriageway with a non-passively-safe portal, no leg between the carriageways unless the TAA agrees (CD 365 Revision 1 clause 3.21). The report reads OK or FAILS.
- The beam ends at the same level for a portal (clause 3.22). This is OK by construction.
- A clear distance to an over-bridge of at least twice the maximum height, unless wind interaction is in the design (clause 3.23).
- Listed for the engineer, not checked: Technical Approval, Eurocode design and fatigue, type selection, working life, vehicle restraint to CD 377, clearances to restraint systems, set-back of verge supports, maintenance, permanent access, foundation reserves, and deformation limits and pre-camber. These read REQUIREMENT.

**Limits.** These are copied from the gantry README, and the first sentence is the README's own warning.

- This is not a structural design. Passing the requirement checks does not show that the gantry, its connections or its foundations are structurally adequate. A gantry is a highway structure. It needs Eurocode analysis, fatigue assessment and technical approval. Legs, boom and foundations here are schematic envelopes of entered dimensions.
- Straight gantry line through one station. Levels come from the centreline profile only. Cross-fall and verge levels are not modelled. The road line is level, and the engineer enters the surface rise and each leg's base level.
- One boom level. No pre-camber, and no inclined or stepped booms.
- Two legs or one. No twin portals, double cantilevers or legs in pairs.
- The elevation carries no text or dimension figures. Values are in the module's derived values and the report.
- Sign faces are bounding rectangles.
- Not checked, because the documents held give no value that can be applied: a limit on span or outreach (CD 365 Revision 1 states none, and NOTE 3 to its Section 10 remarks only that dynamic effects become significant around 20 m spans for conventional steel gantries); a minimum set-back of legs from the carriageway (CD 365 says "should be set back" with no distance, and the numbers are in CD 377, of which only a superseded revision was obtained); and the need for and containment of vehicle restraint (CD 377, by risk assessment).
- Not done in this cut: structural analysis and member design; wind, collision and other actions; deflections; fatigue; foundation design; passive safety assessment; variable message signs and signal equipment and their loads; access walkways, ladders and handrails; lighting of signs; electrical works, cabling and earthing; vehicle restraint design; costs and rates.
- The app does not offer twin portals, double cantilevers or combinations.

### 5.7 Exports

Exports are on the Output tab, in the File menu, and by command. Each export asks for a file or a folder.

**DXF (`DXFOUT`, File → Export DXF).** Writes one DXF file (R12, millimetres) with the drawings of the selected elements. If nothing is selected, it writes every element. Each drawing sits at its element's sheet origin. Elements with no geometry are left out, and the status line says how many. Sign drawings are on one layer per sign colour. The DXF writer matches the C# writer byte for byte on the reference files.

**IFC 4.3 (`IFCOUT`, File → Export IFC 4.3).** Writes the whole model as an IFC 4.3 file (STEP, schema IFC4X3_ADD2). The default name is `model.ifc`. The project name is the saved document's file name. Re-exporting the same saved document keeps each element's identity (GlobalId). The first five export notes go to the command history.

- Signs become `IfcSign` with their coloured artwork, property sets and quantities.
- Posts become `IfcMember` (POST). Foundations become `IfcFooting` (PAD_FOOTING). A sign with its supports becomes an `IfcElementAssembly` (SIGNALASSEMBLY).
- Elements placed on site stand at their site position inside an `IfcRoad`.
- Alignments become `IfcAlignment` with horizontal segments and a polyline axis.

*Standard and edition:* IFC 4.3 (IFC4X3_ADD2). Checked with IfcOpenShell 0.9.0 at test time. The buildingSMART rules named in the export are followed, but the buildingSMART Validation Service has not been run.

*Limits, from the IFC README:*

- Not exported: linear placement (`IfcLinearPlacement`), alignment curve geometry (`IfcCompositeCurve`, `IfcGradientCurve`), vertical profiles and cant.
- Georeferencing: no map conversion or coordinate reference system, and no `IfcRoadPart` breakdown.
- Fixings, clamps, base plates, reinforcement, the socket as a void, excavation and backfill.
- Material grades, categories, layers and profile sets (`IfcMaterialProfileSet`). Sheeting classes, retro-reflectivity and mass. Colours are display colours for viewing, not specification values.
- Structural design: nothing about loads, checks or adequacy beyond the design basis text.
- Type objects (`IfcSignType`, `IfcMemberType`), classifications, layers as `IfcPresentationLayerAssignment` (the layer name is a property), owner history, and the notes of `Generated`.
- The properties `Pset_SignCommon.Category` and `TactileMarking`, `Qto_SignBaseQuantities.Weight` and the other properties of `Pset_MemberCommon` and `Pset_FootingCommon` are not written, because the document has no values for them.
- The crate cannot import IFC.
- Not validated: the buildingSMART Validation Service. Alignments read from LandXML, and alignments with discontinuous joints, were not exported in any test against the toolkit. No viewer has opened these files. Whether an assembly shows as one object in a viewer is not checked.
- The socket of a foundation is not cut out of its solid. A note says so.

**Quantities (`QUANTITIES`, `BOQ`, File → Export quantities (BoQ)).** You choose the name of a CSV file. The default is `sign-quantities.csv`. The export writes these files beside it, each named by extending the chosen name:

- `<name>.csv`: sign-face quantities, one row per sign.
- `<name>.summary.csv`: sign-face totals by ground colour.
- `<name>.posts.csv`, `<name>.foundations.csv` and `<name>.supports.csv`: posts, foundations and totals, when the model has supports.
- `<name>.drains.csv`, `<name>.culverts.csv` and `<name>.gantries.csv`: when the model has those elements.
- `<name>.markings.csv` and `<name>.markings.summary.csv`: when the model has road markings.

The sign CSV and summary are byte-identical to the C# output. Quantities are measured from geometry. No module holds rates or costs, so there is no estimate.

*Limits:* there is no rate or cost in any export. Schedule CSV and XML import and export are not in the app (see section 5.1).

**Fabrication sheets (`SHEETS`, `FABRICATION`, File → Export fabrication sheets).** You choose a folder. The export writes an A3 sheet for each sign, as DXF and as PDF, and a schedule PDF. The status line says how many files were written for how many signs. If any sheet has a layout error, it says so, and says to check before cutting.

The DXF sheets are byte-identical to the C# output. The PDF sheets match the C# output object for object, apart from dates, ids, the producer name and compression.

**Command line export.** `--export-ifc` and `--export-sheets` in section 2 write the starting model in the same formats, then exit.

### 5.8 Python API and MCP server

#### Python package

The package `afrobim` gives the same sign engine, DXF and SVG export, and `.afrobim` documents from a script or a notebook. It runs the same Rust code as the desktop app.

**Install.** You need Python 3.9 or later and Rust 1.85 or later. From `afrobim/python`, create a virtual environment, run `pip install maturin`, then run `maturin develop --release`. To make a wheel, run `maturin build --release --out dist`. The package is built with maturin.

**Rule pack and fonts.** Neither is inside the wheel. `afrobim.SignEngine()` looks for `AFROBIM_PACK` and `AFROBIM_FONTS`, or for `packs/UK-TSM-2019.json` and `fonts/` in the working directory or any folder above it. You can also pass them: `afrobim.SignEngine(pack=..., fonts=...)`.

**Quick start.**

```python
import afrobim

engine = afrobim.SignEngine()
sign = afrobim.stack_sign("ADS-001", 60, [
    afrobim.destination("Kasese", "ahead"),
    afrobim.destination("Masheruka", "right", distance=12, route_number="A109", route="primary"),
])
layout = engine.layout(sign)
print(f"{layout.width_mm:.0f} x {layout.height_mm:.0f} mm")
layout.to_dxf("ADS-001.dxf")     # DXF R12 in millimetres
doc = afrobim.Document()
doc.add_sign(engine, sign)
doc.save("scheme.afrobim")       # opens in the desktop app
```

**Main objects.**

- `afrobim.destination(name, direction, distance, route_number, route)`, `afrobim.stack_sign(code, speed_kph, destinations)` and `afrobim.sign(kind, ...)` build a sign definition, which is a plain dict. `afrobim.normalise(definition)` fills in every field.
- `SignEngine(pack=None, fonts=None)` has `catalog()`, `definition(key, speed_kph)`, `layout(definition)` and `sign_size_mm(definition)`.
- `Layout` has `width_mm`, `height_mm`, `area_m2`, `diagnostics`, `primitives(units)`, `to_dxf(path)` and `to_svg(path)`.
- `Document` has `Document()`, `Document.load(path)`, `save(path)`, `add_sign(engine, definition, x, y, mark)`, `move`, `move_to`, `remove`, `set_property`, `layout`, `export_dxf(engine, path)`. It reads `elements`, `layers`, `extents` and `standard`.

**Errors.** `SignError` (a `ValueError`) for an invalid definition, a missing font character or an unreadable pack. `UnsupportedError` for a sign the engine does not lay out yet. `DocumentError` for a file that is not an AfroBIM document, or a newer one. `KeyError` for an unknown catalogue key or element id.

**Tests.** `pytest` in `afrobim/python`. The tests use the checkout's pack and fonts.

#### MCP server

`afrobim-mcp` is a Model Context Protocol server. It speaks JSON-RPC 2.0 over stdio, one message per line. It lets an AI assistant lay out and check signs, write DXF files and build `.afrobim` documents that the desktop app opens.

**Build.** From the `afrobim` folder, run `cargo build --release -p afrobim-mcp`. The binary is `afrobim/target/release/afrobim-mcp`.

**Register it in Claude Code.** From the repository root, run `claude mcp add afrobim -- afrobim/target/release/afrobim-mcp`. Or add an `afrobim` entry with `"type": "stdio"` and that command to a project `.mcp.json`. Check it with `/mcp`.

**Register it in Claude Desktop.** Claude Desktop does not read `.mcp.json`. Add the server to `claude_desktop_config.json` with an absolute path to the binary. Give `out_dir`, `csv_path` and document paths as absolute paths.

**Tools.** The first five tools are ported from the C# server with the same names, parameters and results. The document tools are new.

| Tool | Arguments | What it does |
|---|---|---|
| `list_packs` | none | Lists the rule packs, the default and whether the fonts were found. |
| `search_catalog` | `pack?`, `query?`, `series?` | Lists catalogue pages: key, family, series, scheme and description. |
| `layout_sign` | `pack?`, `key` or `schedule_row`, `speed_kph?` | Gives diagnostics with clauses, size, stroke width, x-height, primitive counts and an SVG preview. |
| `export_dxf` | `out_dir`, `pack?`, `signs` or `csv_path` | Writes one R12 DXF per sign, with each sign's diagnostics. |
| `validate_schedule` | `csv_path`, `pack?` | Gives per-row errors and warnings with clause citations. |
| `document_new` | `path`, `pack?`, `overwrite?` | Creates an empty document with the pack as its design standard. |
| `document_add_sign` | `path`, `key`, `schedule_row` or `definition`, `speed_kph?`, `mark?`, `x_mm?`, `y_mm?`, `properties?`, `pack?` | Lays out a sign, adds it to the document and saves it. |
| `document_list` | `path` | Reads the document: standard, layers, extents and every element. |
| `document_remove_element` | `path`, `id` or `mark` | Removes one element and saves. |
| `document_save` | `path`, `save_as?`, `pack?` | Lays every sign out again, refreshes stored sizes and saves. |
| `document_export_dxf` | `path`, `out_path`, `marks?`, `pack?` | Writes the document's signs to one R12 DXF at their model positions. |

The document tools are stateless. Each call loads the file, works on it and saves it. Document paths must end in `.afrobim`, and DXF paths in `.dxf`. `export_dxf` writes only inside the `out_dir` it is given. Input schemas are strict, so an undeclared argument is refused. A failed tool returns `isError: true` with the reason, and the server keeps running.

**Rule pack and fonts.** The server looks for them in this order: the `--pack` and `--fonts` arguments; then the `AFROBIM_PACK` and `AFROBIM_FONTS` environment variables; then `packs/UK-TSM-2019.json` and `fonts/` above the executable and then above the working directory. The fonts are third-party files and are not part of AfroBIM.

**Limits.** These are copied from the MCP README, "Differences that remain".

- `quantities` is not in the server yet. It needs the quantities engine, which is being ported in `afrobim-fabrication`.
- Only single-file JSON packs can be loaded today. A folder pack such as `UG-TSM-2004` is listed by `list_packs` with the reason it could not be loaded.
- The Uganda pack (`UG-TSM-2004`, an XML folder pack) is listed but cannot be loaded until the Rust engine reads it.
- Fonts come from `AFROBIM_FONTS` or `--fonts`, not `SIGNCORE_FONTS`. `list_packs` returns `fonts_dir: null` when no fonts folder was found.
- `initialize` reports the server as `afrobim` and advertises only the `tools` capability.
- A line that is not JSON gets a `-32700` error with a null id. A `tools/call` without a tool name is `-32602`.
- A negative `speed_kph` in a schedule row is refused when the row is read.

### 5.9 Street lighting, road restraint, pavement and corridors

These four modules were built as crates first and are now in the app, on the Roads tab. Each is added with a dialog in which every design value starts at zero, which means "not entered": nothing is defaulted, and the crate refuses a definition it cannot use and says why. Their limits are those listed in the workspace README.

- **Street lighting** (`LIGHTING`, `LIGHTCOLUMN`). An installation along an alignment (arrangement, spacing, carriageway width, set-back, the column and luminaire as entered) or a single column at a station and offset. Drawn as a schematic column, bracket and luminaire block at the entered mounting height and outreach, at the profile level. The column and luminaire schedule, summary and connected load are written by Export quantities. `CHECK` opens an input dialog (section 5.10) and then reports the requirements that can be applied exactly from DMRB TD 501 with TA 501, CD 109 and CD 377, line by line. The BS 5489-1:2003 values used are from a superseded edition and the report says so. The lighting class is a label you enter. There is no photometry of any kind, no column or foundation design and no electrical design.
- **Road restraint** (`BARRIER`). A safety barrier run along an alignment at an offset, with the containment level, working width class and impact severity level, post spacing and beam height entered as product data, and end treatments. Plan, schematic solids and quantities. `CHECK` asks for the speed limit and then runs the requirement checks of CD 377 and CD 127. It does not perform or replace the Road Restraint Risk Assessment Process; lengths and any higher containment level from that assessment are inputs. Not for certification.
- **Pavement** (`PAVEMENT`, `PAVEDESIGN`). A layer stack along an alignment: carriageway width, cross-fall, layers and optional shoulders; cross-section, 3D slabs at the profile level and layer quantities by plain geometry, with no rates, densities or bulking factors. `PAVEDESIGN` runs one of three methods you choose (they are not interchangeable): the Uganda MoWT Road Design Manual Volume 3 Part I, UK DMRB CD 224 and CD 226 (from superseded revisions, no complete structure), or AASHTO 1993 flexible. Where a method arrives at layers, a pavement can be made from them. The output is a calculation for an engineer to check, not a certified design.
- **Ground surfaces and corridors** (`SURFACE`, `CORRIDOR`, `SECTIONS`). A ground surface is read from a LandXML 1.2 TIN (hand-built test files only, not tested against a file exported by Civil 3D). A corridor carries a typical section (lane, optional shoulder, formation depth, side slopes, all entered, the same both sides) along an alignment with a vertical profile, cut and filled against the surface. It can be limited to a stretch of the alignment. Cross-sections are shown station by station in a window; the corridor surfaces appear in the 3D site view; an earthworks CSV by average end area is written by Export quantities. Every width, slope, depth and interval is yours, and no bulking or shrinkage factor is applied. Compare a real job against the package you trust before using the volumes for payment.

### 5.10 Stations in the road dialogs, and the lighting check inputs

**Stations.** Place by station has always read the station shown on the drawings, through the alignment's station equations. Every other road dialog now does the same: road markings, side drain, pipe culvert, portal and cantilever gantry, street lighting (start and end), light column, barrier run, pavement (start and end) and the optional stretch of a corridor. The crates take internal stations (measured along the alignment from its start), so the dialog converts what you type.

- On an alignment with no station equations the two are equal and the field is the plain number it always was.
- With station equations, each station field shows the displayed station and a region number beside it, as in Place by station. A station that occurs once may be left with region 0. A station that occurs in more than one region (an overlap, where the displayed station falls back) is refused with the places listed until you pick the region (1 to the number of regions). A station in a gap, or off the ends, is refused.
- Under the field the dialog shows the converted result, for example `1+550.000, internal station 1650.000 m`.
- Each dialog carries a note saying which station is meant.

**Lighting check inputs.** `CHECK` on a street lighting element opens a dialog. It collects what you have established elsewhere: whether the need for lighting was appraised (TA 501), design speed, stopping sight distance, unlit lengths to adjacent lit sections, the required set-back and its source, whether the columns are passively safe, speed limit and flow, the maintenance factor and its source, luminaire sealing, pollution category and cleaning interval, the lamp flux maintenance factor, the type of road and its traffic, the maintained photometric results of your own calculation (L_av, U_o, U_l, wet U_o, TI, EIR, E_av, U_o(E), E_h,min, E_v,min, E_sc,min, with where they come from, and whether facial recognition is necessary and whether the speed is low), and the class selection parameters of CEN/TR 13201-1 for an M, C or P class (a suggestion only). **Every field starts empty.** A field left empty is "not entered": the checks that need it say INPUT NEEDED and nothing is assumed. A value that is not a number is refused by name. AfroBIM computes no photometry. The entries are quoted in the report and are not stored in the document.

### 5.11 Coordination: clash detection

The Coordination tab uses the `afrobim-coordination` crate. A clash test checks one selection of elements against another.

1. **Scene.** Every element that has solids becomes a body in world millimetres, moved with exactly the transform the 3D view draws by (site placement and rotation for site elements, the sheet position for elements not placed on site, corridor and ground surface meshes as the view draws them). Sign faces are flat and have no solids, so they are not bodies; posts, foundations, drains, culverts, gantries, lighting, barriers, pavement, corridors, ground and markings are.
2. **Tests** (`CLASHTESTS`). Add, edit or delete a test: a name, the type, selection A and selection B by category and layer (nothing ticked means all), and for a hard test whether faces that only touch count, for a clearance test the tolerance in millimetres, and whether to ignore pairs where one element hosts the other (a post and its sign or foundation, on by default). **The tolerance is typed by you with no default, and the app offers none: it comes from your project requirements.** A hard test finds solids that overlap. A clearance test finds solids closer than the tolerance (touching and overlapping included).
3. **Run** (`CLASH`). Runs every test and merges the clashes into the stored results: a clash seen again keeps its status, comment and assignee; one that has gone becomes resolved; an approved pair is skipped by later runs.
4. **Results** (`CLASHES`). One row per clash with its id, test, the marks of both elements, kind, depth (hard, an estimate) or distance (clearance), status (new, active, reviewed, approved, resolved), assignee and comment. Filter by test and by status. Status, assignee and comment are edited in the table. **Zoom** frames the clash point in the 3D view and highlights the two elements in magenta. **Export CSV** (`CLASHCSV`) writes every clash.
5. **Markers** (`CLASHMARKERS`). A red ring and cross at each live clash (new, active or reviewed) in the 3D view, on or off.
6. **Saving.** The tests and results are one element of category `coordination` in the document, so they are saved, opened and undone with it.

Limits (from the crate): solids made of flat sides lie inside the true round ones, so distances to round posts are larger than true by at most the sagitta (0.24 mm at a 50 mm radius for the 32-sided posts), and hard overlaps shallower than that are missed; open meshes (ground, corridor surfaces) get intersection and distance tests only and no containment; a reversed winding breaks the touching versus overlapping decision for coplanar faces, and the site-solid meshes of lighting, restraint, pavement and corridors are not guaranteed to be wound outwards; hard depth is an estimate, not a minimum translation distance. Not done: BCF export, saved viewpoints, clashes against other disciplines' IFC models, 4D (time-based) clashes.

### 5.12 4D: the construction programme

The 4D tab uses the `afrobim-schedule` crate. Nothing is assumed: there are no default durations, productivities or holidays.

1. **Programme** (`PROGRAMME`). The first time, enter the programme name, the project start date, the calendar name and tick the working weekdays (none are ticked for you). Then the table of activities: id, WBS, name, duration in working days (0 makes a milestone, which can be a start or finish milestone), and a constraint (start no earlier than, finish no later than, or must start on, each with a date). Below it, the dependencies: predecessor, successor, type (FS, SS, FF or SF) and lag in working days of the successor's calendar (negative means overlap). A required finish date is optional. Edits are stored in the document and the programme is scheduled again at once.
2. **Calendar** (`CALENDAR`). The working weekdays and the days off you enter, per calendar. More calendars can be added and given to activities. No holiday is built in.
3. **Schedule** (`SCHEDULE`). The critical path method: early and late dates, total and free float in working days, the critical flag (float zero or less, not complete), status and percent complete, and the findings of the crate (open starts and ends, negative float, constraint conflicts) in plain words. A programme with errors (a logic loop, a dependency naming an unknown activity, a calendar with no working day) is not scheduled, and the errors are listed.
4. **Gantt chart** (`GANTT`). Drawn from the crate's drawing: critical bars red, others blue, finished grey, a black strip for progress, milestones as diamonds, dependency lines, thin grey baseline bars when there is a baseline, and the data date as a green line.
5. **Baseline** (`BASELINE`, `VARIANCE`). Keeps the dates, durations and float of the current schedule under a name you give. The variance table shows current minus baseline for start, finish, duration and float in working days, with added and removed activities, and the project finish variance.
6. **Progress** (`PROGRESS`). The data date, and per activity the percent complete and actual start and finish. Progress is as of the end of the data date; work done keeps its actual dates and the work left is scheduled from the next working day (retained logic). Actual dates after the data date are refused by the crate.
7. **Links** (`LINK4D`, `LINKREPORT`). Link elements to an activity with an action: construct (absent before, growing during, present after), demolish (present before, gone after) or temporary (present only while the activity runs). Link the current selection by element, or by rule: a category, a layer and a mark pattern (`*` and `?`), every field given must match; rules are read again each time, so new elements are picked up. The link report lists elements no link reaches, activities with no elements, links to elements that no longer exist and rules that match nothing.
8. **4D date** (`FOURD`). A date control: a slider, a date field and day and week steps. With "Colour the 3D view by this date" on, the 3D view shows each linked element in its state at the end of that day: not started (ghosted, or hidden if you choose), in progress (amber), complete (normal), demolished or temporary work that is over (removed). Elements no link reaches stay as drawn. This is a static state at a chosen date: animated playback is a later work package and is not built.
9. **MS Project** (`MSPIN`, `MSPOUT`). Import and export of MSPDI XML. The import report lists everything the file could not carry (summary and null tasks, other constraint types, elapsed lags, calendar work weeks and so on); nothing is dropped silently. Importing replaces the programme (undo restores it) and keeps links whose activity id survives. The export does not carry milestone kind, weights, element links, the baseline or the required finish.
10. **Reports** (`PROGCSV`). The activity table, the variance and the S-curve as CSV into a folder. The S-curve weighs each activity by its working days.

Not done: resources, productivities and levelling, Primavera P6 XER, time-location charts, longest-path criticality, summary tasks and WBS roll-up, multiple baselines.

### 5.13 5D: the estimate

The 5D tab uses the `afrobim-cost` crate. **No rate, price, exchange rate, percentage, item number or standard bill is built in.** Money and quantities are exact decimals. The primary currency starts as the label UGX with 0 decimals; that is a label, with no exchange rate or rate attached. The estimate is one element of category `estimate` in the document; payment terms, kept certificates and model snapshots are stored as properties of that element.

1. **Bill** (`BILL`).
   - *Structure.* Load a structure CSV (`kind,code,section,description,unit`), or start from numbered sections (you type how many; they are titled "Section n" for you to rename), or type sections and items. An item is measured from the model, or manual: a quantity you enter, with the reason it is not measured.
   - *Mapping rules.* A rule sends the quantity lines of one category (a trailing `*` matches a prefix, `*` alone all) that satisfy its conditions to an item: `key=value`, `key=a|b`, `key~text`, `key>=n`, `key<=n`, `key?`, separated by `;`. Rules are tried in order and a line goes to the first that fits. A line's unit must equal the item's unless you give a factor, your own conversion. Load or save a rules CSV. The list of quantity lines the model offers (category, unit, count, attribute names) is shown to help write rules.
   - *Measured quantities.* After measuring the model, each item with its quantity and the number of lines; click an item to see the elements that make it up (mark, line, rule, factor, quantity) and click an element to select it. Adjustments and manual quantities are listed.
   - *Unmapped and ambiguous.* Every line that no rule takes (never dropped) with the reason, and totals by category; lines that more than one rule fits, with the rule used.
   - *Double counting.* A warning where one element is measured by lines of different kinds (a post as a length and as a count, a sign as area and as a count): if both items are priced, the element may be paid for twice. Earthworks cut and fill are not flagged. Map one of the alternatives.
   - Quantities are only those the collectors list (signs, posts and foundations, markings, drainage, gantries, lighting, restraint, pavement, corridors' earthworks). Post mass uses the supports crate's nominal steel density.
2. **Rates** (`RATES`). The library is empty until you enter or load rates. Per item: the rate, its currency, the source note and the date. Load or save a rate CSV (`item_code,description,unit,rate,currency,source_note,date`). **An item with no rate is listed as unpriced and shown as unpriced everywhere; it is never priced at zero without saying so.**
3. **Currency** (`CURRENCY`). The primary currency code and decimals; an optional secondary currency; exchange rates you enter (1 FROM = RATE TO, each with a date and a source note), converting the other way by reciprocal, never through a third currency; and an optional as-of date (blank uses the latest rate). A secondary currency without an exchange rate stops the estimate with a message saying so.
4. **Estimate** (`ESTIMATE`). Tabs: the priced bill (items with quantity, rate and amount, section totals, a secondary-currency column, the bill total, additions and the grand total, with banners for unpriced items, unmapped lines and possible double counting); additions (preliminaries, contingency, provisional sums, dayworks, taxes: whatever you name, each a percentage of the bill total, the running total, a section or earlier additions, or a lump sum, applied in list order); the audit trail (every line, element, rule and factor behind each quantity); revisions (save a snapshot of the measured model, then compare it with the model now, or compare with another estimate in the document; per item quantity and amount old, new, change and percent). The estimate's name and revision label are yours.
5. **Cash flow** (`CASHFLOW`). A window (start and finish) per item, or the same dates for a whole section. Where a programme exists, "Take the dates from the linked activities" proposes for each item the earliest start and latest finish of the activities its elements are linked to; items with no linked element are listed and get no window; you accept or ignore the proposal. The amount of an item is spread evenly by calendar day, both ends counted, by month. A monthly table, the unphased remainder (what has no window) and an S-curve. Export as CSV.
6. **Interim payment certificate** (`CERTIFICATE`). **This module encodes no FIDIC clause and no other contract's terms: every percentage, cap, threshold and amount is your own contract data, and AfroBIM only does the arithmetic.** Enter payment rules (retention with an optional cap, advance recovery with its start threshold, a percentage of gross to date or of the period, a lump amount), the certificate label, the previous certificate (none for the first), and the quantities to date per item: typed, or taken from percent complete (from the linked activities where the programme has them, otherwise from the percent you type per item; an element with neither is 0 percent). The certificate shows gross value, each rule to date, previous and this period, the net, the warnings of the crate (quantity above the bill, or below the previous) and its lines. Keep it to use as the previous certificate of the next one; export as CSV.
7. **Cost reports** (`COSTCSV`). Writes every CSV the crate offers into a folder: quantity lines, bill structure, mapping rules, rates, the priced and unpriced bill, summary, audit trail, unmapped lines, the cash flow, the comparison and the certificate when they exist.

Not done: price adjustment and escalation, valuation of variations, dayworks sheets, claims and the final account, non-linear cash flow distributions, xlsx output.

**The demo.** `--demo` adds a made-up programme linked to the demo elements, a neutral bill with mapping rules and arbitrary rates, two clash tests and arbitrary payment terms. Every number is commented as an arbitrary demo value in `demo_glue.rs`, and the estimate is named "DEMO, not real rates". They are there to make the windows show something and are not rates, durations, tolerances or contract terms.

## 6. Built but not yet in the app

Every engine crate in the workspace is now wired into the desktop app, including street lighting, road restraint, pavement and corridors (section 5.9), clash detection (5.11), the programme (5.12) and the cost estimate (5.13). What the app still does not do is listed with each chapter. The one built feature outside the app is the schedule CSV and XML import of the C# CLI: the MCP server reads schedule CSVs and the app does not import them.

The roadmap (`docs/plan/2026-11-14-afrobim-bim-workflow-roadmap.md`) lists what comes next.

## 7. Command reference

Every command below is in the command table in `commands.rs`. Type the name or an alias on the command line. The shortcut column comes from the `shortcut()` function in `ribbon.rs`.

| Command | Aliases | Shortcut | What it does (help text from `commands.rs`) |
|---|---|---|---|
| `NEW` | none | `Ctrl+N` | Start a new document |
| `OPEN` | none | `Ctrl+O` | Open an AfroBIM document |
| `SAVE` | `QSAVE` | `Ctrl+S` | Save the document |
| `SAVEAS` | none | `Ctrl+Shift+S` | Save the document under a new name |
| `DXFOUT` | `EXPORT` | none | Export the selection, or everything, as DXF |
| `IFCOUT` | `IFC` | none | Export the model as IFC 4.3 |
| `QUANTITIES` | `BOQ` | none | Export sign-face quantities (CSV and summary) |
| `SHEETS` | `FABRICATION` | none | Export fabrication sheets (DXF and PDF) and the schedule PDF |
| `UNDO` | `U` | `Ctrl+Z` | Undo the last edit |
| `REDO` | none | `Ctrl+Y` | Redo the edit that was undone |
| `MOVE` | `M` | none | Move the selection: base point, second point |
| `COPY` | `CO`, `CP` | none | Copy the selection: base point, a point per copy |
| `ERASE` | `E`, `DELETE` | `Delete` | Erase the selection, or ask for one |
| `SELECTALL` | `ALL` | `Ctrl+A` | Select every element |
| `ZOOM` | `Z`, `ZE` | none | Zoom to show every element |
| `ZOOMWINDOW` | `ZW` | none | Zoom to a window picked with two corners |
| `GRID` | none | `F7` | Toggle the grid (F7) |
| `SNAP` | none | `F9` | Toggle grid snap for picked points (F9) |
| `ORTHO` | none | `F8` | Toggle ortho: moves held to X or Y (F8) |
| `REAL` | none | none | Show elements in full colour |
| `OUTLINED` | `WIREFRAME` | none | Show elements as outlines |
| `ARRANGE` | none | none | Arrange every element in rows |
| `STACK` | `SIGN` | none | Insert a stack-type direction sign |
| `DIRECTION` | `DS` | none | Insert a rectangular direction sign |
| `CONFIRM` | `RC` | none | Insert a route confirmatory sign |
| `PLACE` | none | none | Insert a place identification sign |
| `LANE` | none | none | Insert a lane pre-selection sign |
| `DIVERSION` | none | none | Insert a temporary diversion sign |
| `MAP` | `CROSSROADS` | none | Insert a map-type crossroads sign |
| `TEE` | none | none | Insert a map-type T-junction sign |
| `ROUNDABOUT` | `RBT` | none | Insert a map-type roundabout sign |
| `LIBRARY` | `INSERT`, `LIB` | none | Open the standard-sign library |
| `3D` | `ORBIT`, `PLAN` | none | Switch between the 2D model space and the 3D view |
| `UK` | `UKTSM` | none | Use the UK Traffic Signs Manual |
| `UGANDA` | `UG`, `UGTSM` | none | Use the Uganda Traffic Signs Manual |
| `LANDXML` | `ALIGNMENT` | none | Import horizontal alignments from a LandXML file |
| `STATION` | `CHAINAGE` | none | Place the selected sign on an alignment by station and offset |
| `MARKINGS` | `MARKING`, `ROADMARKING` | none | Lay a road marking line along an alignment (Traffic Signs Manual Chapter 5) |
| `DRAIN` | `SIDEDRAIN` | none | Add a side drain along an alignment, with placeholder dimensions to edit |
| `CULVERT` | none | none | Add a pipe culvert across an alignment at a station, with placeholder dimensions to edit |
| `GANTRY` | `PORTAL` | none | Add a portal sign gantry at a station, with placeholder dimensions to edit |
| `CANTILEVER` | none | none | Add a cantilever sign gantry at a station, with placeholder dimensions to edit |
| `CHECK` | `VERIFY` | none | Run the check of the selected gantry, drain, street lighting, barrier run or pavement (street lighting opens its input dialog; the pavement check is a design method) |
| `LIGHTING` | `LIGHTS`, `STREETLIGHTING` | none | Lay a street lighting installation along an alignment (spacing, arrangement and the column are entered) |
| `LIGHTCOLUMN` | `COLUMN` | none | Place one lighting column at a station and offset on an alignment |
| `BARRIER` | `RESTRAINT` | none | Add a safety barrier run along an alignment (containment, working width and impact severity are entered) |
| `PAVEMENT` | `PAVE` | none | Add a pavement along an alignment: width, cross-fall and a list of layers |
| `PAVEDESIGN` | `PAVEMENTDESIGN` | none | Design a pavement structure by the MoWT manual, UK DMRB or AASHTO 1993, and make a pavement from the layers |
| `SURFACE` | `TIN`, `TERRAIN` | none | Import ground surfaces (TIN) from a LandXML file |
| `CORRIDOR` | none | none | Build a corridor on an alignment and a surface: a typical section, cut and fill, earthworks |
| `SECTIONS` | `XSECTIONS` | none | Show the cross-sections of the selected corridor, station by station |
| `CLASHTESTS` | `CLASHSETUP` | none | Add, edit and delete clash tests: what is checked against what, hard or clearance, with your own tolerance |
| `CLASH` | `CLASHRUN`, `CLASHDETECT` | none | Run every clash test on the model and open the results |
| `CLASHES` | `CLASHRESULTS` | none | Open the clash results: filter, review, comment, assign, zoom to a clash |
| `CLASHMARKERS` | none | none | Show or hide the clash markers in the 3D view |
| `CLASHCSV` | none | none | Export the clash results as CSV |
| `PROGRAMME` | `ACTIVITIES`, `PROG` | none | Edit the construction programme: activities, durations, constraints and dependencies |
| `CALENDAR` | `WORKDAYS` | none | Edit the programme calendar: working weekdays and the days off you enter |
| `SCHEDULE` | `CPM` | none | Run the critical path method and show dates, float, the critical path and any problems |
| `GANTT` | none | none | Show the Gantt chart: critical path, baseline bars and the data date |
| `BASELINE` | none | none | Keep the current schedule as the baseline |
| `VARIANCE` | none | none | Show how the programme differs from the baseline |
| `PROGRESS` | `DATADATE` | none | Enter progress and the data date, then reschedule |
| `LINK4D` | `LINKACTIVITY` | none | Link model elements to activities, by selection or by rule |
| `LINKREPORT` | none | none | List elements no activity reaches and activities with no elements |
| `FOURD` | `DATE4D` | none | Show the model at a chosen date: not started, in progress, complete, removed |
| `MSPIN` | `IMPORTMSP` | none | Import a programme from MS Project XML and show what the file could not carry |
| `MSPOUT` | `EXPORTMSP` | none | Export the programme as MS Project XML |
| `PROGCSV` | `PROGRAMMEREPORTS` | none | Export the activity table, variance and S-curve as CSV |
| `BILL` | `BOQSETUP` | none | Edit the bill of quantities and its mapping rules; see measured quantities, unmapped and ambiguous lines |
| `RATES` | `RATELIBRARY` | none | Enter or load rates (with currency, source note and date); lists unpriced items |
| `CURRENCY` | `EXCHANGE` | none | Set the primary currency label, an optional secondary currency and dated exchange rates |
| `ESTIMATE` | `PRICE` | none | Show the priced bill, additions, audit trail and revision comparison |
| `CASHFLOW` | `SCURVE` | none | Phase the estimate in time (dates from the programme if you like): monthly table and S-curve |
| `CERTIFICATE` | `IPC`, `INTERIM` | none | Work out an interim payment certificate from your own payment terms |
| `COSTCSV` | `COSTREPORTS` | none | Export every cost report as CSV into a folder |
| `HANG` | `GANTRYSIGN` | none | Hang the selected sign on a gantry |
| `UNHANG` | none | none | Take the selected sign off its gantry |
| `SUPPORTS` | `POSTS` | none | Add posts and foundations to the selected sign |
| `DESIGN` | `SIZE` | none | Size the selected sign's posts and foundations for wind (UK method) |
| `NOSUPPORTS` | none | none | Remove the selected sign's posts and foundations |
| `COPYCLIP` | none | `Ctrl+C` | Copy the selection to the clipboard |
| `PASTECLIP` | `PASTE` | `Ctrl+V` | Paste the clipboard into the model space |
| `ZOOMIN` | `ZI` | none | Zoom in about the centre of the view |
| `ZOOMOUT` | `ZO` | none | Zoom out about the centre of the view |
| `MARKS` | `LABELS` | none | Show or hide the mark under each element |
| `CANVAS` | none | none | Switch the model space between dark and light |
| `THEME` | none | none | Switch between the light and dark themes |
| `HELP` | `?` | none | List every command |
| `ABOUT` | none | none | About AfroBIM |
| `QUIT` | `EXIT` | none | Close AfroBIM |

## 8. Planned, not built yet

These tools appear on the ribbon as disabled buttons. They are not built. They are listed here only. The text in the second column is from `ribbon.rs`.

| Ribbon tab and panel | Tool | What it will do |
|---|---|---|
| Insert, Import | Sign schedule | Import a sign schedule from CSV or XML. |
| Insert, Reference | DXF underlay | Attach a DXF drawing as a background reference. |
| Annotate, Annotation | Text | Place a note in model space. |
| Annotate, Annotation | Dimension | Dimension between two points. |
| Annotate, Annotation | Leader | A note with an arrow to a point. |
| Annotate, Annotation | Measure | Read the distance between two points. |
| Manage, Project | Project information | Project name, client, contract and revision for title blocks. |
| Manage, Project | Layer manager | Create, rename and colour layers. Layers can be shown and locked in the Layers panel today. |
| Manage, Project | Units | Display units and precision. |

## 9. Standards and editions

Each row gives a document, the edition the code uses, what it governs and any warning that the sources give about its currency. Rows marked "not in app" are for the crates in section 6.

| Area | Document and edition | Governs | Warnings and status |
|---|---|---|---|
| Traffic signs, UK | UK Traffic Signs Manual and TSRGD 2016, pack `UK-TSM-2019` | Sign layout, catalogue, diagnostics | Golden-tested against the C# engine. |
| Traffic signs, Uganda | Uganda Traffic Signs Manual (2004), pack `UG-TSM-2004` | Uganda sign rules and catalogue | Embedded signs under this pack are not ported. |
| Road markings | Traffic Signs Manual Chapter 5, 2018 | Line types, widths, patterns, quantities | **The 2003 edition is superseded and was not used.** TSRGD 2016 was not read directly. |
| Alignments | LandXML 1.2, schema `LandXML-1.2.xsd` | Import of alignments, profiles, station equations, superelevation | Not tested against any Civil 3D export. |
| Sign supports, wind | IHE *Sign Structures Guide* 2021, applying BS EN 12899-1:2007 and its UK National Annex | Post and planted foundation design, wind | Two printed figures in the guide do not follow from its expressions. The module follows the expressions. |
| Member capacity | BS EN 1993-1-1 | Classification and resistance of steel tube posts | Class 4 is refused. |
| Spread foundation | BS EN 1997-1, Design Approach 1 | Geotechnical checks | Bearing resistances are the guide's assumed values. |
| Planted foundation | PD 6547 or CD 354 rev 1 | Method, selected by the engineer | Not in the repository. |
| Planting depth | BS EN 40-2 Table 7 | Minimum planting depth | Not held. The engineer enters the value. |
| Uganda wind | MoWT *Road Design Manual* Volume 4: Bridge Design, January 2010, Section 5.3, Table 2 | Basic wind pressure for supports | A reading of two documents that do not refer to each other. The Figure 2 wind map is blank in the PDF. |
| Drainage, MoWT basis | MoWT *Road Design Manual* Volume 2: Drainage Design, January 2010 | Recurrence, rational method, Manning, culverts | Ambiguities and slips are flagged in the crate README. |
| Drainage, DMRB basis | CG 501 v2.2.0; CD 521 v1.2.0; CD 522 v0.1.0; CD 529 v1.1.0 (all September 2025) | Drain capacity, criteria, culvert details | No rainfall or culvert hydraulics under this basis. CIRIA C786 and CD 532 are not held. |
| Gantry headroom | DMRB CD 127 v1.0.1, published 29 July 2021 | Headroom and sag compensation | Currency judged from the absence of a superseded mark. Confirm it. |
| Gantry requirements | DMRB CD 365 Revision 1, March 2020 | Listed gantry requirements | **Watermarked SUPERSEDED.** The current version was not obtained. Confirm every clause. |
| Restraint, CD 377 (gantry README) | DMRB CD 377 Revision 4, January 2021 | Not used for any value | **Watermarked SUPERSEDED.** |
| Restraint, requirements (not in app) | DMRB CD 377 v5.0.1, 27 February 2026 (with CD 127 v1.0.1) | Containment, set-back, lengths, terminals | The version read. The RRRAP is outside the tool. |
| Restraint, performance (not in app) | BS EN 1317-2:2010 | Impact tests, containment levels, severity, working width, intrusion | The owner should confirm it is current. |
| Restraint, terminals (not in app) | DD ENV 1317-4:2002 | Terminal classes P1 to P4 | A Draft for Development, not a British Standard. **CD 377 5.0.1 still cites it, and it should be replaced** when a superseding document is available. |
| Restraint, cross-check (not in app) | TII DN-REQ-03034, November 2015 with Amendment 1; MoWT Volume 1, January 2010, Section 12.3 | Reference only | Not used for any calculation. |
| Lighting, requirements (not in app) | DMRB TD 501 v0.1.0, September 2025 | Governs lighting class, maintenance, unlit gaps, design outputs | Replaces Revision 0 (March 2020), which is **watermarked SUPERSEDED**. |
| Lighting, appraisal (not in app) | DMRB TA 501 Revision 0, March 2020 | Need for lighting | The ILP says it was updated in 2025. The search returned Revision 0. |
| Lighting, speed (not in app) | DMRB CD 109 v1.1.0, 30 October 2026 | Stopping sight distance, Table 2.10 | Not interpolated. |
| Lighting, clearances (not in app) | BS 5489-1:2003, with Corrigendum No. 1 | Column clearances, class tables, maintenance factors | **Superseded.** Replaced in 2013 and again in 2020. Confirm every value taken from it. |
| Lighting, secondary (not in app) | TII DN-LHT-03038, February 2012 | Reference only | Irish. It is the NRA addendum to TD 34/07 (2007), which TD 501 replaced. |
| Pavement, MoWT (not in app) | MoWT *Road Design Manual* Volume 3, Part I, January 2010 | Flexible pavement structures and traffic | Adopts the SATCC *Draft Code of Practice* (1998, reprinted 2001). |
| Pavement, traffic (not in app) | DMRB CD 224 Revision 0, March 2020 | Design traffic | **Superseded** by Version 0.1.0 (2025). The current revision was not obtained. |
| Pavement, thickness (not in app) | DMRB CD 226 Version 0.1.0, 29 November 2021 | Asphalt over an HBGM base | **Superseded** by Version 0.2.0 (2025). The current revision was not obtained. |
| Pavement, AASHTO (not in app) | AASHTO *Guide* (1993), as reproduced in FHWA NHI-05-037 (2006) and in lecture notes | Flexible pavement design equation | The 1993 Guide itself was not read. |
| Exports, IFC | IFC 4.3 (schema IFC4X3_ADD2) | Model export | Checked with IfcOpenShell 0.9.0. The buildingSMART Validation Service has not been run. |
| Corridor (not in app) | None | Geometry and measurement only | Every design value is entered by the engineer. |

## 10. Notes for the maintainer

- **Markings in 3D.** Marking placements have z = 0, but the marking solids themselves are generated at the alignment's profile level (`afrobim-markings` `plan_at_level`), so the lines follow the road profile in the 3D view.
- **Ribbon status.** The ribbon now has the Coordination, 4D and 5D tabs, and the Clash check item that was planned on the Structures tab has been removed because clash detection exists. This manual follows the ribbon.
- **Uganda pack in the MCP server.** The MCP README and the server's error hint say XML folder packs are not read by the AfroBIM engine yet. The desktop app does load `UG-TSM-2004`, through `afrobim-signs/src/xml_pack.rs` and the app's standards list. The MCP server still calls `RulePack::load` on the folder, so it refuses it. The hint's wording is out of date. The manual says the app loads the pack and the server does not.
- **Crate list.** The workspace README's layout now lists `afrobim-coordination`, `afrobim-schedule` and `afrobim-cost` with the other crates.
- **Roadmap link.** The workspace README points to `docs/plan/2026-11-13-afrobim-platform.md`. This manual uses `docs/plan/2026-11-14-afrobim-bim-workflow-roadmap.md`, as briefed. Both files exist. The README link may need updating.
- **Missing crate READMEs.** `afrobim-signs`, `afrobim-dxf`, `afrobim-fabrication`, `afrobim-kernel`, `afrobim-draw` and `afrobim-app` have no README. The sign and export limits here come from the workspace README's status table only.
- **Station conventions.** Every road dialog now takes the station shown on the drawings, with the region picker of Place by station, and converts it to the internal station (`station_input.rs`). The pavement design dialog's "make a pavement" step and the cross-section window have no station field of their own.
- **Shortcut labels.** Some menu items show key labels that `shortcut()` in `ribbon.rs` does not hold: Erase shows "Del" in the menu and "Delete" in `shortcut()`, and zoom extents shows "Z" in the menu. The command table follows `shortcut()`, as briefed.
- **Drain solid.** The drainage README says the drain's 3D solid is level "because the alignment has no vertical geometry yet". The alignment crate now reads vertical profiles and superelevation. The manual reports the drain as level, as that README states. The code should be checked against the current alignment data.
