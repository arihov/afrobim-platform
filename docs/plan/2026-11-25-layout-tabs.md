# Layout tabs (paper space) and viewports

Status: built on branch `claude/layout-tabs` (from `claude/satellite-basemap`); the second round (plot matches the screen, twist, MoWT title block, per-layout fields, plot border, snap zoom, Ctrl-drag copy) is on `claude/layout-tabs-2`, described under "Second round" below. This note records the design, the data model, the commands, how it is tested and what is not done. It moves item 4 of the interface revamp design ("paper-space layouts: roadmap, later") from roadmap to built.

## What the user sees

* **A tab strip along the bottom of the canvas**, above the command line: a fixed `Model` tab, one tab per layout, then a `+`. Click switches space. Ctrl+PageDown and Ctrl+PageUp cycle Model, layout 1, layout 2 and round again. When the tabs do not fit, arrows at both ends scroll them and the current tab is kept in view.
* **Right click on a tab:** New layout, From template (A0 to A4, landscape or portrait, each with its size in mm), Rename, Delete (asks first), Duplicate, Move left, Move right, Page setup, Plot / Export. Double click renames in place. Right click on `+` or on `Model` offers the two creation entries. Delete and rename are undoable.
* **A layout** draws the paper at true size in millimetres on a grey ground with a shadow, the printable area dashed, the frame and the AfroBIM title block (from `afrobim-sheets`, filled from Project information), anything drawn on the paper, and the viewports. Mouse wheel zooms the paper about the pointer, the middle button pans it, a middle double click fits it.
* **Viewports** are windows onto model space. Each keeps its own centre (a model point) and drawing scale 1:n. They show the same drawing as the Model tab: the model painter itself runs with the viewport's transform and clip, so basemap tiles, images, DXF underlays, GIS overlays, civil objects and annotation all appear.
* **Status bar** on a layout tab: `PAPER` and `MODEL` toggle, the scale of the selected or active viewport as a drop down of the standard list, a lock button. The position read-out shows paper millimetres in paper space.
* **Properties** shows the selected viewport (scale list, custom scale, lock, display, title, label, frozen layers) when a viewport is selected in paper space.

## Architecture

Three layers, none of them a second sheet engine.

| Layer | File | Holds |
|---|---|---|
| Data and arithmetic | `afrobim-kernel/src/layout.rs` | `Layout`, `Viewport`, `PaperShape`, `LayoutList`, paper to model transform, clip polygon test, grips, presets, scale list |
| Plot | `afrobim-sheets/src/layout_plot.rs` | `compose_layout` into the existing `ComposedSheet`, then the existing `write_pdf` and `write_dxf`; ISO paper from `PaperSize`; scale from `Scale` |
| Application | `afrobim-app/src/layout_ui.rs`, `layout_canvas.rs`, `layout_cmds.rs` | tab strip, windows, commands, paper painting and input, model space through a viewport |

The kernel cannot depend on the sheets crate (the sheets crate depends on the kernel), so the layout stores plain millimetres and a scale denominator. The sheets crate converts: `viewport_scale(vp)` is `Scale(vp.scale_den)`, and the plot and the screen both take paper millimetres per model millimetre from `Scale::paper_mm_per_m() / 1000`. A test checks that `Viewport::paper_mm_per_model_m` and `Scale::paper_mm_per_m` agree at every standard scale, and that 1:500 is 2 mm per metre.

### Data model (saved in the document)

```text
Document.layouts : LayoutList { items: [Layout], next_id }
Layout   { id, name, size ("A3"), landscape, width_mm, height_mm,
           margins [left,right,top,bottom], title_block_mm,
           viewports [Viewport], shapes [PaperShape], new_viewport_frozen [layer] }
Viewport { id, rect [x0,y0,x1,y1] (paper mm), clip: polygon (paper mm) or none,
           centre (model mm), scale_den (1:n), locked, on, frozen [layer],
           title, show_label }
PaperShape = Poly { id, points, closed } | Circle { id, centre, radius }
```

* Paper units are millimetres, Y up, lower left corner on the origin. Model units are millimetres, as everywhere in the kernel.
* `paper = paper_centre + (model - centre) / n`. A boundary stretch keeps the contents where they are on the paper (the centre is adjusted); a move carries the contents along; a clip does not move the contents.
* In the `.afrobim` file the `layouts` key is written only when there is at least one layout and is read with a default, so files from before this change load with no layouts and files without layouts are byte for byte what they were. The format version is unchanged: an older program reading a newer file ignores the key.
* Names are unique, case-insensitive, never empty and never `Model`. Ids are never reused within a document.

### Undo and redo

The undo history now stores a `DocState { elements, layouts }` instead of the element list (`Document::state`, `restore_state`). Every layout and viewport change goes through `schedule_commit`, so each is one undo step after the usual 0.3 s settle, together with element edits. Layer visibility flags and the selection stay outside the history as before. A paper pan or zoom is not document data and is not an undo step. Pan and zoom *inside* a viewport change the viewport's centre and scale, so they are (debounced) undo steps.

### Painting and model space through a viewport

`canvas()` in `app.rs` was split: the body that draws and handles the model is now `canvas_model_pass(ctx, response, painter, rect, nav)`. The Model tab calls it with the whole canvas. A layout calls it with the viewport's rectangle after `with_model_view` has put the viewport's transform in `self.view`, turned the grid off, chosen the light canvas, hidden the layers frozen in that viewport, and (on return) put all of it back. So there is one model draw path, one set of tools, snaps and grips, all working in viewport coordinates through `self.view`. A locked viewport passes `nav = false`: the model pass ignores wheel and middle button and the layout code pans or zooms the paper instead. Inactive viewports are painted with `paint_scene` and the same wrapper; a polygon clip is applied by painting the paper colour over the part of the bounding box outside the polygon.

Basemap tiles are synchronised for each viewport's view just before it is painted, so each viewport gets the tiles for its own window.

Maximise (`VPMAX`) runs the same pass over the whole canvas with a temporary view that fits the viewport's model window; `VPMIN` discards it, so the viewport is exactly as it was.

### Plot

`PLOT` opens a window: current layout or all layouts, PDF or DXF. `compose_layout` builds a `ComposedSheet` of the paper size: frame, title block (scale field taken from the viewports, or `AS SHOWN` when they differ), paper shapes, and for each displayed viewport the module drawings of the elements on non-hidden, non-frozen layers, mapped by an exact affine map (`(origin - centre) * k + paper_centre`, differences taken first so far-away coordinates keep their precision) and cut to the boundary (box, then polygon). Annotation (text, dimensions, leaders) is added through a callback from the application. The PDF page size equals the paper size (A3 landscape is 1190.55 x 841.89 pt), and at 1:500 a 50 m line is 100 mm on the sheet. Export of all layouts writes one multi-page PDF. DXF R12 is in paper millimetres, side by side for several layouts.

## Commands

All are in the one table (`commands.rs`), so ribbon, palette and the command line share them. Ribbon: View tab, panels Layouts, Viewports, Viewport settings, Paper drawing; Output tab, panel Plot.

| Command | Aliases | Does |
|---|---|---|
| `LAYOUT` | LAYOUTS, LO | Manager window. With words: `LAYOUT NEW name`, `SET name`, `RENAME name`, `DELETE [name]`, `COPY`, `LIST` |
| `LAYOUTNEW` `LAYOUTRENAME` `LAYOUTDELETE` `LAYOUTCOPY` `MODEL` | | the tab operations; `MODEL` returns to the Model tab |
| `MVIEW` | MV, VPORTS | rectangular viewport, two corners |
| `MVIEWPOLY` | MVP | polygonal viewport, Enter or right click finishes |
| `MVIEWOBJECT` | MVO | viewport from a closed polyline or a circle drawn on the layout |
| `MVIEWFIT` | MVF | fills the printable area (less the title block) |
| `MVIEW1` to `MVIEW4` | VP1 to VP4 | 1, 2, 3 or 4 viewport arrangement |
| `MSPACE` `PSPACE` | MS, PS | enter model space through the selected viewport; return to paper |
| `VPSCALE` | VPS | scale window; `VPSCALE 1:500` on the command line |
| `VPLOCK` `VPDISPLAY` | | lock display; display on or off |
| `VPMAX` `VPMIN` | | maximise to the canvas; restore |
| `VPLAYER` | | freeze or thaw layers in the viewport; layers frozen in new viewports |
| `VPCLIP` `VPUNCLIP` | | reshape the boundary to a picked polygon; remove it |
| `VPCOPY` `VPDELETE` `VPPROPS` | | copy (10 mm up and right), delete, properties window |
| `PRECT` `PCIRCLE` `PPOLY` | | draw on the paper, as boundaries for `MVIEWOBJECT` |
| `PAGESETUP` | | paper size, orientation, margins, title block |
| `PLOT` | PRINT | the plot window |

Interaction: click selects a viewport; drag inside it moves it (contents with it); handles on the selected viewport resize by corner or edge (rectangle) or move a vertex (polygon); Delete removes it; double click inside enters model space (the border goes thick); double click outside, or `PSPACE`, returns. Scale list: 1:1, 1:2, 1:5, 1:10, 1:20, 1:50, 1:100, 1:200, 1:250, 1:500, 1:1000, 1:1250, 1:2000, 1:2500, 1:5000, 1:10000, and any custom 1:n. A locked viewport shows a lock on its border and refuses scale changes.

## Tests

* `afrobim-kernel` (`layout.rs`, `document.rs`): paper to model round trip at every standard scale; 1:500 shows exactly 50 m across 100 mm; scale refused when locked or invalid; clip containment (an L shape, edges, a triangle); a clip, stretch or move keeps the contents where they belong; freeze and thaw and the new-viewport default; presets tile without overlap; viewports from polygons and circles; tab add, rename, delete, duplicate, reorder and the name rules; serde round trip of a full layout and loading of a file without the key.
* `afrobim-sheets` (`layout_plot.rs`): scale agrees with `Scale` at every standard scale; a 50 m line is 100 mm at 1:500 and centred on the view centre; freeze and display off remove it; polygon clip cuts it; the PDF of a layout with a 1:1000 viewport has the right `MediaBox` for A3, A1 and A4; multi-page PDF; DXF; labels; ISO templates.
* `afrobim-app` (`layout_tests.rs`, headless, using the existing `u4_dialogs_test` harness): tab lifecycle and cycling, 40 tabs scrolling, undo and redo of every kind of change, the canvas drawing in paper space, in model space through a viewport, maximised and with a polygon viewport (also on the civil demonstration), real double click events entering and leaving model space, the creation tools and VPCLIP, the lock, the plot to a file with the right page size, save and reopen. The existing command table, ribbon reachability and "every command opens" tests cover the new commands.

## Second round (branch `claude/layout-tabs-2`)

### The plot now matches the screen

For each displayed viewport the plot composes, bottom to top, as the screen paints: the **raster** (online map tiles, then the georeferenced images in their draw order), the **DXF underlays**, the **GIS overlays**, the **civil objects**, the module drawings, and **annotation**. The sheets crate calls back into the application twice per viewport (`layout_plot::Stage::Under` and `Stage::Over`); the application side is `afrobim-app/src/layout_out.rs`.

* **One selection, two outputs.** The screen painters and the plot use the same functions to decide what shows: `UnderlayState::shown`, `ImageryState::shown`, `GisState::shown` with `Geo::features_in`, the `_in` methods of the civil `Scene` (`lines_in`, `fills_in`, `texts_in`, `points_in`), and `layout_plot::element_shows_in` for document layers. Nothing was copied: `civil_scene.rs`, `underlay.rs`, `imagery.rs` and `gis_overlay.rs` were changed so that painting calls them.
* **Frozen layers and layer visibility.** A layer frozen in a viewport hides the civil objects, underlays, images and overlays on that layer, on the screen and in the plot. Civil objects also follow the document's layer switch (they did not on the screen before). The layer that holds the placeholder of an underlay, image or overlay (`Underlays`) is switched off in the document by design, so for those the document switch does not count, only the viewport freeze does (`LayoutUi::frozen_now` carries the freeze into the painters).
* **Vectors** (underlays, GIS, civil) are mapped through `Viewport::to_paper` (scale and twist), cut to the viewport box when they are put on the sheet and to the viewport polygon by the existing merge. Colour with alpha is flattened onto white paper. Labels are placed highest priority first and a label that would overlap one already placed is left out, as on the screen. A civil line much smaller than the pen is left out.
* **Rasters** are resampled by `afrobim_sheets::raster::PaperRaster` into one upright RGBA raster for the viewport box at 150 dpi (at most 3200 pixels on a side, the resolution drops to fit), cut to the viewport polygon, and written to the PDF as an image with a soft mask. Images are decoded again from their files for the plot (the screen keeps only the GPU texture), through the same placement grid the screen uses, so a reprojected image lands where it does on screen. A twisted viewport turns them with the same transform.
* **Map tiles** go through the MAPCAPTURE code (`plan_capture`, `render`), so the tile limit and the cache behave as for a capture, and **respect `capture_allowed`**: with a provider that does not allow saving tiles (every built-in provider today) the tiles are left out, the plot says so as a warning on the sheet (the provider's own refusal text, "... allows its map to be displayed but not saved as an image ... Terms: ..."), the same text is shown in the Plot window, and it is in the note after the plot. A custom tile URL is the user's own source and is plotted, with its attribution on the sheet. A missing project coordinate system or a local grid also leaves the map out with the reason. Tests use a fake tile fetcher (`LayoutUi::plot_fetcher`).
* **DXF** has no raster: the DXF writer leaves rasters out and the plot note says how many (`skipped_rasters`).
* Annotation is turned with the view.

### Other items

* **Twist** (`Viewport::twist_deg`, serde default 0, not written when 0): the model is shown turned anticlockwise about the view centre. `Viewport::to_paper`/`to_model` take it; `ViewTransform` has a `twist_deg` too (`with_twist`), used for the screen transform of a viewport, so painting, picking, the cursor, snaps and the grid-free model tools work through it; `ViewTransform::world_window` is the box around all four screen corners, used where culling took two corners (it is also what a selection box in a twisted viewport covers). Boundary stretches keep the contents in place at any twist. Plot: the module drawings use `Aff::scale_rotate(k, twist)`, annotation, underlay labels, rasters and the north point turn with it. Command `VPROTATE` (aliases `VPTWIST`, `TWIST`): `VPROTATE 30` on the command line, or without an angle it opens Properties, which has a Twist field. A locked viewport keeps its view.
* **MoWT title block** (`TitleBlockLayout::mowt()`, 80 mm wide): revision table, north point space (the arrow follows the first viewport's twist), project, contract no., client (fixed text, Ministry of Works and Transport), consultant, contractor, drawing title, drawing no. with sheet no. of total, scale, date, drawn, checked, approved. New field keys `Consultant` and `Contractor` (and the project information has the two fields, with serde defaults). Chosen per layout in Page setup (`Layout::title_style`, serde default AfroBIM); the viewport area follows the width of the block. The block is data like any other `TitleBlockLayout`; it is a working layout with the names and order a MoWT drawing carries, not a copy of a form.
* **Sheet number, revision and drawing title per layout** (Page setup, `Layout::sheet_no`, `revision`, `drawing_title`, serde defaults, not written when empty). An empty field prints the layout name (number, title) or the project's latest revision as before.
* **Plot border** per viewport (`plot_border`, default off): plots the boundary (the polygon for a clipped viewport) on the `VIEWPORT` layer. Properties has the check box.
* **Snap to standard scale when zooming** (`snap_scale`, default on, Properties): the wheel in an unlocked active viewport steps through `STANDARD_SCALES` about the point under the pointer; with the option off the scale is custom, as in CAD. A custom scale moves to the next standard one in the direction of the zoom; the ends of the list hold.
* **Ctrl-drag of a viewport handle** copies the viewport and drags the copy (`Layout::copy_viewport_in_place`); the original stays.

### Tests added

* `afrobim-kernel`: twist round trip at 0, 30, 90 (and more) degrees, 90 degrees turns anticlockwise, normalisation, refusal when locked, stretch under a twist, model window of a twisted viewport, serde defaults of the new fields (and nothing written for an untouched viewport), standard scale stepping, copy in place, layout title field defaults and round trip; `ViewTransform` twist round trip, zoom and far pan, `world_window`.
* `afrobim-sheets`: `PaperRaster` (placement, orientation, turn, grid, opacity, polygon mask, seam not blended twice), twisted plot, plot border, sheet number/revision/title in the title block, MoWT block fields and sizes, the north point follows the twist, rasters in the PDF with an xref that holds and a soft mask only where needed, rasters not in the DXF, consultant and contractor.
* `afrobim-app` (`layout_tests.rs`): `plot_matches_screen` (underlay at scale, clipped, twisted, frozen, switched off; GIS; georeferenced image in the PDF and not in the DXF with the note; map tiles with a refusing provider and with a custom one; civil objects plotted, frozen and switched off, and the screen painting fewer shapes with the layer frozen in the viewport) and `viewport_options` (screen transform equals the kernel transform at 0, 30, 90 degrees; pick and cursor through a twisted view; VPROTATE; snap zoom maths and the wheel; Ctrl-drag copy; MoWT title block per layout and Page setup; plot border).

## Not done yet

* **Point clouds** are on the screen (a raster of the level-of-detail query) but not in the plot. The Plot window says so.
* **Referenced documents and reference markers** (`shortcuts_paint`) are not in the plot.
* **Plot colour of vectors:** underlays are plotted in dark grey at their opacity and GIS and civil in their screen colours (flattened onto white); there is no plot style table (CTB) yet. A civil text taller than 15 mm or shorter than 1.2 mm on the paper is left out.
* **Selection box in a twisted viewport** takes everything in the box around the turned rectangle (not only what is inside the turned one).
* **The wheel snap steps once per wheel event**, so a fast spin can pass several standard scales; there is no throttle.
* **Map tiles are fetched in the plot call** (disk cache first, then the network, on the UI thread, at most the capture tile limit): a plot over a large uncached area waits for them. A background plot with progress would be better.
* **Concave clip polygons** cut strokes and text exactly, but a filled area that crosses a concave boundary is kept whole and the sheet warns. Convex clips cut fills exactly. (Unchanged.)
* **Overlapping polygon viewports:** the area outside a polygon is covered with paper colour inside its bounding box, which also hides a viewport underneath in that area. (Unchanged.)
* **Layer freeze and annotation:** annotation does not follow the viewport freeze (its layers are not the document's layers). (Unchanged.)
* **Raster images in the DXF** are skipped (R12 has no raster); an AutoCAD DXF with IMAGE entities would need a newer DXF version.
* No visual check on a real display was done; the tests are headless.
