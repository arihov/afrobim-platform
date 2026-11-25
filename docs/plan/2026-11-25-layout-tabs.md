# Layout tabs (paper space) and viewports

Status: built on branch `claude/layout-tabs` (from `claude/satellite-basemap`). This note records the design, the data model, the commands, how it is tested and what is not done. It moves item 4 of the interface revamp design ("paper-space layouts: roadmap, later") from roadmap to built.

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

## Not done yet

* **Twist angle** (rotating the view in a viewport) is not stored or drawn. The data model would need an angle in `Viewport` and the transform a rotation; the plot already uses `Aff`, which can rotate.
* **Civil objects, point clouds, georeferenced images, DXF underlays, GIS overlays and basemap tiles are on the screen but not in the plot.** The plot draws what modules generate (elements) and annotation. The civil scene and the raster layers are painted by app code that has no sheet output; the plot window says so. Frozen layers do not hide annotation in the plot.
* **Concave clip polygons** cut strokes and text exactly, but a filled area that crosses a concave boundary is kept whole and the sheet warns. Convex clips cut fills exactly.
* **Overlapping polygon viewports:** the area outside a polygon is covered with paper colour inside its bounding box, which also hides a viewport underneath in that area.
* **Layer freeze and civil objects** on the screen: civil scenes are not hidden by a viewport freeze (they are by the layer's global visibility, as on the Model tab).
* **Viewport borders are not plotted** (as for a non-plotting viewport layer in CAD); a per-viewport "plot border" option is not there.
* **Title block:** there is no separate MoWT template in `titleblock.rs`; the layouts use `TitleBlockLayout::default()` filled from Project information. A MoWT-specific layout would be a second `TitleBlockLayout` value, chosen in Page setup.
* **Layout properties are not editable per layout** beyond page setup: a sheet number and revision field per layout (the title block shows the layout name as number and title).
* **Pan and zoom inside an unlocked viewport change its scale** to a custom value (as in CAD). A "snap to standard scale when zooming" option is not there.
* **Viewport grips for copy** are commands and context menu entries, not Ctrl-drag on a handle.
* No visual check on a real display was done; the tests are headless.
