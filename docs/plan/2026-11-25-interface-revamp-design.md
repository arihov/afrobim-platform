# AfroBIM: interface review and redesign proposal (2026-11-25)

Status: review and proposal for the owner. No code was changed. The brief: a modern look, Civil 3D-like density (many tools visible at once, small icons with labels, nothing hidden), a better workspace switcher, and the AfroBIM design system (white canvas, near-black ink `#0C120E`, forest green `#1F5130`, signal lime `#A5E82F` used sparingly, Archivo for display, IBM Plex Sans for body, IBM Plex Mono for labels) with a derived dark theme.

## 1. What the interface is today

Read from `ribbon.rs` (641 lines), `menus.rs` (619), `workspace.rs`, `theme.rs` (559), `ui_kit.rs` (1,031), `glyphs.rs` (257), `icons.rs`, `statusbar.rs`, `palette.rs` and the panel layout in `app.rs` (`TopBottomPanel::top("toolbar")` at line 1893, `status` at 1908, `SidePanel::left("tools")` at 2040, `SidePanel::right("properties")` at 2093, `command` at 2278).

### 1.1 Layout, top to bottom

| Layer | What is there now |
|---|---|
| Menu bar | File, Edit, View, Insert, Civil, Modify, Analyse, Tools, Window, Help (`menus.rs`). Every item is a command from the command table, so the menu duplicates the ribbon. |
| Ribbon tab row | The **workspace button** (tinted, far left), then the tab names as plain 12.5 pt labels with a 2 px underline. 18 tabs in the "All" workspace: Home, Edit, Insert, Annotate, Modify, Signs, Roads, Civil, Design, Structures, Coordination, 4D, 5D, Construction, Output, Manage, View, Help. |
| Ribbon body | Panels of buttons. `BIG` is 62 x 62 px (26 px icon, label under it in 11 px, wrapped to 2 or 3 lines); small buttons are 20 px high with a 14 px icon. A `ScrollArea::horizontal` carries what does not fit. Counting the source, the ribbon has about 285 buttons, of which about 230 (80 per cent) are large; small buttons appear only on Annotate, Civil and Structures. |
| Left panel | Toolspace: Prospector, Survey, Settings tabs, and the sign and modify palettes. |
| Canvas | Model space, with the basemap and overlays. |
| Right panel | Properties. |
| Bottom | Command line, then status bar with the segmented read-outs (`statusbar.rs`). |
| Overlays | Command palette (Ctrl+K), hover preview cards, dialogs built on `ui_kit::Dialog`. |

### 1.2 Tokens in code

* Three looks: Dark glass (default), Light glass, Solid (dark or light). Glass means translucent panel fills and a bright top-edge highlight.
* Dark base `#0F141C` with blue accent `#2F6FDB`; light base `#EEF1F6`, accent `#1D5FD0`. No AfroBIM green anywhere.
* Radii 6 (controls), 12 (windows), 8 (menus). Window shadow blur 30, offset 10.
* Type: Inter (variable file, OFL) at 11.5, 12.5, 13.5 and 16 px. Icons: Phosphor Regular via `egui-phosphor`.
* Spacing: 4, 8, 12, 16 px.
* A second icon set of 151 hand-drawn `Icon` values (`icons.rs`) remains for sign templates and planned tools.

### 1.3 What works well (keep)

1. One command table (`commands.rs`) drives the ribbon, menus, palette and command line. A test already fails if a ribbon button names a missing command, and another if a command has no way to be reached.
2. Hover preview cards that say what a command does, what to type and the shortcut.
3. Honest "planned" buttons.
4. A token struct (`Tokens`) with one source for colours, so a re-skin is a change in one file, not many.
5. The Phosphor icon system with one lookup table (`glyphs.rs`).
6. The interface scale setting (0.85 to 1.75), which matters for small laptop screens.

### 1.4 What is wrong, measured

| # | Problem | Evidence |
|---|---|---|
| 1 | **Density is low.** Nearly every tool is a 62 px square, so a ribbon holds about 16 tools per 1,000 px of width. The Civil tab has 15 panels and 78 buttons (45 large, 33 small), roughly 4,000 px of ribbon on a 1,920 px screen: more than half is behind a horizontal scroll, which is "hidden" in the sense the owner means. | `BIG = 62 x 62`; `ScrollArea::horizontal` in `ribbon.rs:560` |
| 2 | **Too many tabs, with overlapping names.** Edit and Modify; Civil, Roads and Design; Home holds seven panels (File, Signs, Modify, Supports, Alignment, Navigate, Drafting aids) that belong to different jobs. Edit's whole content (undo, redo, copy, paste, select all, erase) is already on the menu bar. | `TABS` array |
| 3 | **Workspace switching is a hidden filter and looks like a stray button.** It is a tinted menu button at the left of the tab row, indistinguishable from a tab at a glance. It opens a 300 px list of 13 entries, each with a separate wrapped blurb. Choosing one **removes tabs and panels** from the ribbon and can change which side panels show (`show_left`, `show_right`, `toolspace_tab`), so the whole screen rearranges. The file's own doc admits "every command still runs from the palette": tools are hidden, not absent. | `workspace.rs:64-210`, `switcher` |
| 4 | **Overlapping workspace names.** "Civil", "Survey, Clouds and Data", "Survey and Terrain", "Road Design", "Pipes, Grading and Junctions" overlap in content, so a user cannot predict where a tool is. | `WORKSPACES` |
| 5 | **Two navigation systems on top of each other.** A menu bar of 10 menus and an 18-tab ribbon both list the same commands, using about 50 px of height before any tool appears. No quick-access toolbar. | `menus.rs`, `ribbon.rs` |
| 6 | **Look does not match the brand.** Blue accent and Inter instead of forest, lime, Archivo and Plex. Glass (translucent) panels reduce legibility, cost GPU and make the app look like a different product from afrobim.com. | `theme.rs:150-195` |
| 7 | **Labels are crammed.** A word longer than the button forces the button wider; others wrap to three lines under the icon. | `button()` in `ribbon.rs:432` |
| 8 | **No contextual tabs.** Selecting a sign, alignment or (soon) a drafting object does not bring its tools forward. | no code for it |
| 9 | **No ribbon minimise.** On a 1,366 x 768 laptop the tab row, panels (80 px), menu bar and status bar leave little canvas. | `app.rs:1893` |
| 10 | **Status bar toggles are not visible as toggles.** Grid, snap and ortho are small buttons in the Home tab (Drafting aids) rather than a status bar of states, as in AutoCAD and Civil 3D. | `DRAFTING` panel |

## 2. Redesign overview

The proposal in one paragraph: replace the menu bar and the two-row tab strip with a slim **app bar** (menu, quick access toolbar, command search, workspace, theme); rebuild the ribbon around a **three-row grid of small icon-and-label buttons**, with large buttons only for a panel's main tool and **split buttons** for families; **cut the 18 tabs to 12** and put the remainder behind a visible "More" menu so no tool is hidden; turn the **workspaces into 7 presets** that reorder and emphasise tabs and panels but never remove them; re-skin everything with **brand tokens** in a light and a dark theme, without glass.

### 2.1 Screen anatomy

```
 +--------------------------------------------------------------------------------------------------+
 | APP BAR      32 px   logo, menu, quick access, command search, workspace, theme, help            |
 +--------------------------------------------------------------------------------------------------+
 | TAB STRIP    28 px   Home  Draw  Modify ...  | discipline tabs |  More v      [ ^ minimise ]      |
 +--------------------------------------------------------------------------------------------------+
 | RIBBON       96 px   panels of 3 rows of 24 px buttons, 16 px panel title row                    |
 +---------+---------------------------------------------------------------------+------------------+
 | TOOL-   |                                                                     | PROPERTIES       |
 | SPACE   |   CANVAS (white in light theme, ink in dark)                        | (two-column grid)|
 | 240 px  |                                                                     | 300 px           |
 +---------+---------------------------------------------------------------------+------------------+
 | COMMAND LINE   26 px  (Plex Mono)                                                                |
 +--------------------------------------------------------------------------------------------------+
 | STATUS BAR     24 px  coordinates | SNAP GRID ORTHO POLAR OSNAP DYN LWT | units, scale | messages |
 +--------------------------------------------------------------------------------------------------+
```

Fixed chrome is 32 + 28 + 96 + 26 + 24 = 206 px. The current chrome is about 20 (menu) + 24 (tab row) + 82 (ribbon) + command and status, so height is **about the same**; the gain is in width, not height. Ribbon minimise (below) gives a 1,366 x 768 laptop back 96 px.

### 2.2 The new ribbon, mocked

```
 AfroBIM [=]  [New][Open][Save][Undo][Redo][Plot]v   [ Search commands..   Ctrl+K ]    [ Road design v ] [Dark] [?]
 Home   Draw   Modify   Insert   Annotate  | Survey  Alignments  Corridors  Drainage  Output  Manage  View | More v
       =======
+----------------------+----------------------+----------------------+----------------------+----------------------+
|[L] Line        [v]   |[+] Move  [R] Rotate  |[T] Text         [v]  |[=] Layer Manager     |[?] Distance      [v] |
|[~] Polyline    [v]   |[C] Copy  [M] Mirror  |[D] Dimension    [v]  |[x] Isolate  [*] Frz  |[i] ID point          |
|[o] Circle      [v]   |[O] Offset [T] Trim   |[>] Leader       [v]  |[L] Match layer       |[g] List              |
|[a] Arc         [v]   |[F] Fillet [E] Extnd  |[#] Hatch        [v]  |[c] ByLayer colour [v]|[q] Quick select      |
|DRAW            [>]   |MODIFY           [>]  |ANNOTATE         [>]  |LAYERS & PROPS   [>]  |UTILITIES       [>]   |
+----------------------+----------------------+----------------------+----------------------+----------------------+
```

Reading the mock: the tab row shows the core tabs, then a divider, then the discipline tabs of the current workspace, then **More v** listing every other tab (so nothing is removed). The mock shows 4 rows to fit more names; the final grid has 3. A trailing `[v]` is a split-button arrow; `[>]` in a title row opens the panel's own dialog or its extended row. `[Dark]` is the theme toggle and `[?]` is Help.

### 2.3 Button types and sizes

| Type | Size (at scale 1.0) | Used for |
|---|---|---|
| Large button | 48 w (min) x 72 h; 32 px icon; label up to 2 lines of 11 px under it, max 12 characters a line | The one or two main tools of a panel (Line, Alignment, Surface, Sheets) |
| Small button (icon and label) | 24 h, width = 6 + 16 icon + 6 + label + 6; 16 px icon, 11.5 px label | Everything else; stacked three high |
| Icon-only small button | 24 x 24; 16 px icon | Toggles and clusters with a clear icon (Undo, Redo, Zoom) |
| Split button | Large or small, with a 14 px wide arrow segment on the right | A family: Circle (centre-radius, 2P, 3P, TTR), Alignment (by layout, from polyline, from LandXML). The face runs the last used or the default option, and the choice is remembered in the preferences file. |
| Drop-down | 24 h, field style | Layer, colour, linetype, lineweight, units, style |
| Stack | Three small buttons or two rows of one | Tight related sets (Move, Copy, Rotate) |
| Toggle | Small button with a filled background when on | Drafting aids; Ortho, Snap, Grid |

Rules:

* Ribbon height is **three** small-button rows, so every panel is 96 px tall including a 16 px title row. A large button spans the three rows.
* Aim for **at least 80 per cent of commands as small buttons**. A large button costs about 62 px of width per tool (16 tools per 1,000 px). A column of three small buttons costs about 112 px (27 tools per 1,000 px), and the typical mix of one large to four small gives about 22. Small buttons alone are not enough: the 78 Civil buttons would still need about 3,300 px as one tab, which is why section 2.4 splits Civil into three tabs of about 1,100 px each. After the split every tab is expected to fit a 1,920 px window with no scrolling, and a 1,366 px window with at most two panels collapsed into pop-ups. Check these figures with the real layout in item 4 of the change list; they are calculated from the button sizes, not measured.
* A panel never scrolls. When the window is too narrow, panels collapse **from the right** in priority order into a single icon button with a pop-up of the whole panel (the Office and Civil 3D method), so every tool is still one click away. Priority is a number on each panel; the Home, Draw, Modify and Annotate panels collapse last.
* Panel title: 10 px, Plex Mono, upper case, 0.04 em tracking, muted ink. A small arrow at the right of the title opens the panel's dialog (for example Layer Manager) or an extended row of rarely used tools.
* Ribbon minimise: double click a tab, or Ctrl+F1. Three states: full, tabs only, tabs with panels on hover. The state is remembered.
* Key tips: pressing Alt shows letter badges on tabs and buttons (accessibility and speed).
* Contextual tabs: when the selection is a sign, an alignment, a drafting object or a culvert, a tab with a lime underline appears after the last tab with that object's tools (for example "Polyline" with PEDIT, Join, Convert to feature line). It disappears when the selection changes.

### 2.4 Tabs: from 18 to 12, with a More menu

| New tab | Takes from the current tabs |
|---|---|
| Home | File and clipboard (from Edit), Drafting aids moved to the status bar, quick Modify, Navigate, Layers and properties, Utilities |
| Draw | New (the drafting design: `2026-11-25-drafting-tools-design.md`) |
| Modify | Edit's Selection panel plus the drafting Modify tools; the current Modify tab's discipline panels (Markings, Drainage, Lighting, Restraint, Pavement, Earthworks) move to their disciplines |
| Insert | as now |
| Annotate | as now, with Snap panel moved to the status bar |
| Survey and Terrain | Points, Surfaces, Survey database, Point clouds, Reprojection, Shortcuts |
| Alignments and Corridors | Alignments, Profiles, Sections, Corridors, Intersections, Design tab panels (Alignment, Profile, Checks, Options) |
| Drainage and Structures | Pipe networks, Grading, Drainage, Gantries, culverts, supports |
| Signs and Furniture | Signs, Markings, Lighting, Restraint, Pavement |
| Delivery | Coordination, 4D, 5D (with Green BIM), Construction |
| Output | Sheets, reports, exports, recovery |
| Manage | Project, appearance, preferences, Help (Help becomes an app bar button) |

**View** becomes a status bar and canvas navigation bar function (zoom, orbit, 2D or 3D, display style), with a View tab only for the less common items. Edit disappears (its content is in the quick access toolbar and Home). Help moves to the app bar. That gives 12 tabs. In the **More** menu the user always finds the tabs the current workspace has put aside, each marked with its workspace.

### 2.5 App bar

| Area | Content |
|---|---|
| Left | Logo mark, product name in Archivo, and the **application menu** button. It opens the old File, Edit, View and the other menus in one pop-up, so the menu bar no longer takes a row. Every item is still a command from the table. |
| Quick access | New, Open, Save, Undo, Redo, Plot or Export, with a drop-down to add more. 16 px icons, no labels. |
| Centre | Command search box (opens the existing palette, `Ctrl+K`), 320 px, placeholder "Search commands". |
| Right | Workspace switcher, theme toggle (light and dark), Help, Send feedback (see the launch plan), account or licence. |

## 3. Workspace switcher

### 3.1 Principles

1. A workspace **chooses emphasis, not existence**: it orders tabs and panels, opens or closes side panels and picks the toolspace tab. It never removes a tool. Tabs outside the workspace remain in **More**.
2. Switching must not make the screen jump: it keeps the ribbon tab if that tab exists in the new workspace, and keeps side panel widths and the canvas view.
3. The switcher is one control in a fixed, consistent place.

### 3.2 Control

* Position: app bar, right side, before the theme toggle. Look: a plain outline button with a 16 px icon, the workspace name in Plex Sans 12.5 px and a chevron. **No tinted fill** (the current look reads as a tab or a warning). It takes the lime underline only while the pop-up is open.
* Pop-up: a 2 column grid of cards, 260 x 56 px each: 20 px icon, name in Plex Sans medium, one line (max 48 characters) of what is emphasised in 11 px muted. The current workspace has a forest left bar and a tick. Keyboard: arrow keys, Enter, Esc; Ctrl+Alt+W cycles.
* Remembers: the last workspace per project is saved with the document settings (`PROJECT_SETTINGS_CATEGORY`), and the last one used is the default for new documents.

### 3.3 Seven presets in place of thirteen

| New workspace | Replaces | Emphasised tabs (in order) |
|---|---|---|
| Everything (default) | All | the 12 tabs, nothing in More |
| Survey and Terrain | Survey, Clouds and Data; Survey and Terrain | Survey and Terrain, Insert, Output |
| Roads and Corridors | Civil; Road Design | Alignments and Corridors, Survey and Terrain, Output |
| Drainage and Structures | Drainage; Pipes, Grading and Junctions; Structures and Supports | Drainage and Structures, Alignments and Corridors, Output |
| Signs and Furniture | Signs and Markings; Lighting and Restraint | Signs and Furniture, Annotate, Output |
| Delivery | Coordination; Construction | Delivery, Output, Manage |
| Documentation | Documentation | Output, Annotate, Insert |

Home, Draw, Modify, Insert, Annotate and Manage are always visible in the first positions.

## 4. Tokens

All values map onto the existing `Tokens` struct (`theme.rs`) so the re-skin is one file plus the font install. Contrast figures are WCAG ratios computed for this proposal: body text and controls need 4.5:1, large text and icons 3:1.

### 4.1 Colour

| Token (existing field) | Light | Dark | Notes |
|---|---|---|---|
| `surface` (window clear colour, canvas ground) | `#FFFFFF` | `#0C120E` (the brand ink) | White canvas as in the design system; the dark canvas is the ink itself |
| `panel` (side panels, ribbon, bars) | `#F3F6F4` | `#141C17` | A faint green-grey, not blue-grey |
| `window` (dialogs, menus) | `#FFFFFF` | `#1B2520` | opaque, no glass |
| `raised` / `raised_hover` / `raised_active` | ink at 6 / 10 / 16 per cent | white at 6 / 10 / 16 per cent | |
| `field` | `#FFFFFF` | `#0C120E` | text boxes |
| `border` | `#D5DDD8` | `#2A3630` | 1 px |
| `highlight` | none | none | glass top line removed |
| `text` | `#0C120E` (18.9:1 on white) | `#E8EEE9` (14.8:1 on `#141C17`) | |
| `text_muted` | `#4A5A4F` (7.3:1 on white, 6.7:1 on `#F3F6F4`) | `#A9B6AD` (8.3:1 on `#141C17`) | |
| `accent` (primary button fill) | `#1F5130` forest (9.2:1 against white text) | `#2E7D4F` (5.1:1 against white text) | In the dark theme forest `#1F5130` is only 2.1:1 on ink, so it is **not** used as a line or text there |
| `text_on_accent` | `#FFFFFF` | `#FFFFFF` | |
| `accent_text` (links, active tab text, focus ring) | `#1F5130` (9.2:1) | `#6DBE8A` (7.8:1 on `#141C17`) | |
| `selection` (selected row, selected element tint) | forest at 14 per cent | `#6DBE8A` at 28 per cent | |
| `signal` (new token) | `#A5E82F` as a **fill with ink text** (12.8:1) | `#A5E82F` (11.7:1 on `#141C17`) | see 4.2 |
| `success` | `#2E7D4F` (5.1:1) | `#6DBE8A` | |
| `warning` | `#8A5A00` (5.9:1) | `#E8B04A` (8.9:1) | |
| `danger` | `#B3261E` (6.5:1) | `#FF9A90` (8.5:1) | |
| `shadow` | ink at 16 per cent | black at 50 per cent | blur 12, offset 4 (current: blur 30, offset 10) |
| CAD ink on canvas (`ink` in `canvas.rs`) | black | `#E8EEE9` | already switches on `dark` |

Rule: **lime is never text on white** (about 1.5:1) and never a large area.

### 4.2 Where lime is allowed ("used sparingly")

At most one lime element per screen region, from this closed list:

1. the underline of the active ribbon tab and of a contextual tab;
2. the active-command indicator in the command line (a 2 px bar and the command name);
3. the count badge of the current selection in the status bar;
4. an "update available" or "new" dot.

Primary buttons are forest, not lime.

### 4.3 Type

| Role | Family | Size | Weight | Used for |
|---|---|---|---|---|
| Display | Archivo | 16 / 20 | 600 | Product name in the app bar, dialog titles, the start screen |
| Body | IBM Plex Sans | 12.5 (body), 11.5 (small controls, ribbon labels), 13.5 (dialogs) | 400, 500 | Menus, buttons, properties, tree |
| Label and data | IBM Plex Mono | 10 (panel titles, caps, tracking 0.04 em), 12 (command line, coordinates, status read-outs, numeric fields) | 400, 500 | Panel titles, coordinates, station and chainage values, command line |

Notes:

* All three families are SIL Open Font Licence. Add their notices to `NOTICE` and ship their licence texts with the app (the release audit already requires this for Inter).
* egui (version 0.31 in the lockfile) selects a font by file, not by a weight axis. The current variable Inter file is used at its default weight only, so "bold" and "medium" need **separate static font files**, each registered as its own family (about 5 files: Archivo SemiBold, Plex Sans Regular and Medium, Plex Mono Regular and Medium; roughly 1 MB in the binary). This is the understanding of the egui font API from reading `install_fonts`; confirm it in a one-day spike before committing.
* Numbers in tables and read-outs use Plex Mono so columns line up.
* Keep `Inter` only as a fallback for glyphs the new fonts lack.

### 4.4 Shape, spacing, icons

| Token | Value |
|---|---|
| Spacing scale | 2, 4, 6, 8, 12, 16, 24 px (a 2 px half-step is needed for ribbon density) |
| Control radius | 4 px (current 6); window 8 (current 12); menu 6 (current 8) |
| Ribbon: gap between small buttons | 2 px vertical, 4 px horizontal |
| Ribbon: panel padding | 6 px left and right; 1 px divider between panels |
| Small button | 24 px high, 6 px side padding, 16 px icon, 6 px icon-to-label gap |
| Large button | min 48 x 72 px, 32 px icon, 4 px icon-to-label gap |
| Icon set | Phosphor Regular only. Sizes: 16 (buttons, tree, menu), 20 (quick access, cards), 32 (large buttons). One stroke weight; no mixing with hand-drawn icons except sign templates, which draw the sign |
| Icon colour | ink or `text` for neutral; forest (`accent_text`) for the main action of a panel; `danger` only for destructive commands; disabled at 40 per cent |
| Labels | Sentence case, 14 characters at most on a small button (truncate and show the tooltip), 2 lines of 12 at most on a large one; no trailing full stops |
| Minimum hit area | 24 x 24 px (a mouse-driven desktop application; scale 1.15 and above for tablets) |
| Focus ring | 2 px `accent_text`, 1 px offset |
| Motion | none in the ribbon; pop-ups fade in 80 ms |

### 4.5 Look and scale settings

* Replace the three looks by **Light** and **Dark** (and "Follow system" if egui exposes the OS theme). Remove glass. Keep the interface scale choices 0.85 to 1.75, and add 1.0 as the explicit default for 1,920 px and 0.9 suggested for 1,366 px screens.
* Existing preference keys (`dark-glass`, `light-glass`, `solid-dark`, `solid-light`) are mapped on load (the two dark ones to Dark, the two light ones to Light) so saved settings do not break.

## 5. Status bar and command line

| Element | Change |
|---|---|
| Toggles | SNAP, GRID, ORTHO, POLAR, OSNAP, DYN, LWT as 24 px toggle chips with Plex Mono 10 px caps and a forest fill when on; click toggles, right click opens its settings. F-keys as in AutoCAD (F7 grid, F8 ortho, F9 snap, F10 polar, F3 osnap, F12 dynamic input). |
| Coordinates | Plex Mono 12 px, project coordinates at true magnitude as now |
| Segments | keep the existing fit-and-drop logic in `statusbar.rs` |
| Selection badge | lime count badge when something is selected |
| Command line | Plex Mono, one history line plus the prompt, active command name with a lime bar |

## 6. Screen-by-screen change list

Effort is in working days of agent-assisted building with tests and a visual review. It is an estimate, to be revised after the first two items.

| # | Screen or file | Change | Effort | Tests |
|---|---|---|---|---|
| 1 | `theme.rs` tokens and `install_fonts` | New `Tokens::of` values (section 4.1), new `signal` token, spacing and radii, fonts (spike first), preference-key mapping, glass removed | 3 | A unit test that computes the contrast ratio of every text and background pair listed in 4.1 and fails below 4.5 (3 for icons); a test that old preference strings still load |
| 2 | `ui_kit.rs` (dialogs, forms, fields, tabs, segmented controls) | Restyle with the tokens: Archivo titles, Plex labels, 4 px radii, tighter rows (row height 26 to 28), label column in Plex Mono caps for units | 3 | Existing dialog tests; screenshot comparison for 5 representative dialogs in both themes |
| 3 | `ribbon.rs` data model | Replace `Item::{Big, Small, Planned}` with `Item { icon, label, command, size: Large or Small, split: Option<&[SplitItem]>, toggle }`; add panel priority; migrate the existing ~285 items into the new three-row layout | 4 | The existing tests ("every ribbon button names a real command", "every command can be reached") continue to pass; new test: no panel has more than 3 rows; every small label is 14 characters or fewer |
| 4 | `ribbon.rs` layout engine | Three-row grid placement, large buttons spanning rows, split buttons with remembered face, panel collapse into pop-up by priority, minimise states, key tips, contextual tabs | 8 | Layout unit tests at widths 1,366, 1,600 and 1,920: every command is reachable in at most two clicks; no horizontal scroll |
| 5 | App bar (new), `menus.rs` | Application menu pop-up that holds the old menus, quick access toolbar with customisation, search box wired to `palette.rs`, theme toggle, help and feedback buttons; remove the menu bar row | 4 | A test that every command in the old menus is in the new application menu |
| 6 | `workspace.rs` | Seven presets, "emphasise, never remove" semantics (ordering plus More), card pop-up, per-project memory, cycle shortcut | 3 | Replace `every_ribbon_panel_is_in_some_workspace_that_is_not_all` by: every panel is reachable in every workspace (in tab or in More); switching keeps the active tab when it exists |
| 7 | Tabs and panel content (`ribbon.rs`) | 18 to 12 tabs per section 2.4, including the Draw tab when the drafting set exists | 3 | The two existing reachability tests |
| 8 | Status bar (`statusbar.rs`, `app.rs:1908`) | Toggle chips, F-key bindings, selection badge | 2 | Existing status bar fitting tests; a test per toggle |
| 9 | Command line (`app.rs:2278`) | Plex Mono, active command indicator, prompt styling | 1 | Existing tests |
| 10 | Left toolspace (`app.rs:2040` and the toolspace modules) | Tree rows at 22 px, 16 px icons, tab strip restyled, collapse arrows | 2 | Screenshot comparison |
| 11 | Right properties panel (`app.rs:2093`) | Two-column grid (Plex Mono labels, Plex Sans values), collapsible groups with a 10 px caps header, unit suffixes, `*VARIES*` for multi-select | 3 | Existing properties tests; screenshot comparison; also the drafting properties from the drafting design |
| 12 | Canvas chrome (`canvas.rs`) | Light and dark ground colours from the tokens, selection and snap marker colours (snap marker in forest and lime), a small navigation bar at the canvas edge (zoom, extents, 2D or 3D) | 2 | Existing canvas tests; marker contrast |
| 13 | `glyphs.rs` and `icons.rs` | Audit every action has a Phosphor glyph at 16 and 32 px; replace the hand-drawn `Icon` values except sign templates; add the missing glyphs for drafting commands | 2 | The existing test that keeps the glyph table complete |
| 14 | Command palette (`palette.rs`) | Restyle, show shortcut and workspace, recent commands | 1 | Existing tests |
| 15 | Hover cards (`ribbon.rs` preview) | Two-stage tooltip: instant label and shortcut, preview card after 1.2 s; token styling | 1 | none beyond screenshots |
| 16 | Context menus (`menus.rs`) | Row height 24, 16 px icons, shortcut column in Plex Mono | 1 | Existing tests |
| 17 | Accessibility and scaling | Keyboard focus order through the ribbon, key tips, minimum hit areas, test at scale 0.85 and 1.75 and at 1,366 x 768 | 3 | A screenshot matrix run in CI on Linux: two themes x three widths |
| 18 | Documentation | User manual: new chapter on the interface; screenshots; a keyboard map | 2 | n/a |

Total: about **48 working days**. A smaller first release (items 1, 3, 4, 5, 6, 8, 17) is **about 27 days** and delivers the ribbon, the app bar, the switcher and the brand look; items 2, 7, 9 to 16 and 18 follow.

### Suggested release order

1. **R1 (about 9 days):** items 1, 2, 8, 9. The brand look, fonts and status bar: everything a user sees changes colour and type and no layout moves. Low risk, high visible effect, and it can ship in the alpha.
2. **R2 (about 22 days):** items 3, 4, 5, 6, 7. The ribbon, the app bar and the workspaces. This is the large change and should be behind a Preferences switch ("Classic ribbon") for one release so alpha users can compare and report.
3. **R3 (about 17 days):** items 10 to 18, plus the drafting tab when the drafting set lands.

## 7. Risks and checks

1. **egui limits.** No font weight axis (section 4.3); no built-in ribbon widget, so the layout engine (item 4) is custom code; pop-ups and hover behaviour need care with many small widgets. Mitigation: one-day spikes for fonts and for a 3-row layout at 1,366 px before the plan is committed.
2. **Performance.** Only the buttons of the visible tab (at most about 80) are painted each frame, as now, which is acceptable in immediate mode, but measure on the lowest laptop the alpha testers use.
3. **Regression of the reachability guarantees.** The two existing tests are the safety net; keep them running through the whole change.
4. **Alpha confusion.** Moving 18 tabs to 12 changes where people look. Keep the command palette and the "More" menu prominent, and print a one-page "where did it go" map in the release notes.
5. **Brand fonts.** Archivo and IBM Plex are OFL and may be bundled; check each file's licence text before shipping and record it in `NOTICE` (see the release audit).
6. **Dark theme tuning.** The dark values here are derived and checked for contrast; they have not been seen on a real screen. A visual pass by the owner on two displays is part of R1.
7. **Open decision for the owner:** keep a classic menu bar as an option (some users expect it) or drop it, and whether to keep the three glass looks as a legacy setting for one release.
