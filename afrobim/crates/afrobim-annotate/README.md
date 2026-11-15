# afrobim-annotate

The annotation engine for AfroBIM: text, dimensions, leaders, measuring, display units, named
styles and layer helpers. It is a library; the application wires the commands (TEXT, DIM, LEADER,
MEASURE, layer manager, units dialog) to it.

Annotations are **ordinary document elements**, one element each, with a `Module` implementation, so
the kernel's selection, move, copy, erase and file save work on them without knowing what they are.
Everything they draw is first reduced to one description (`AnnGeometry`: strokes, fills and text
labels) that feeds the on-screen drawing, the DXF export and the sheet output, so the three agree.

No size here is a drafting-standard value. Every size is a neutral **default** that the user edits,
and each is labelled so where it is defined. Nothing in this crate claims conformity with a
standard. No DXF written by this workspace has been opened in AutoCAD; it has been read back by
ezdxf (including `audit()`) and by this workspace's own reader.

## Model in one page

- Model space is millimetres, Y up. Areas are mm2, volumes mm3, angles are degrees anticlockwise
  from +X. `DisplayUnits` decides how these are shown and typed (metric only).
- Sizes in styles are **paper millimetres**; the annotation scale (the denominator, 500 for 1:500)
  turns them into model millimetres. Text printed at 2.5 mm stays 2.5 mm on paper at any scale.
  The scale default is 1; set it first, as you would set an annotation scale in CAD.
- An annotation element stores its definition points **relative to `element.origin`**. That is why
  `Document::translate`, copy and paste move it correctly. `origin`, `width` and `height` are the
  lower-left corner and size of everything it draws, text included. After you change an annotation,
  call `write` (or `normalize`) so they are recomputed; `Generated.drawing` then has the lower-left
  corner on the origin, as the kernel contract requires.
- An element keeps a **copy of the style it was made with**. Editing a named style does not silently
  change drawings; `settings::restyle(&mut doc)` applies changed styles to the annotations that use
  them.
- Values that need the rest of the document (a station on an alignment, the mark of another element)
  are looked up by `refresh(&mut doc)` and cached in the element, so `generate` needs no document.
  Call `refresh` after a rename or erase, an alignment edit, or a moved station dimension point.

## Wiring (copy-ready)

```rust
use afrobim_annotate as ann;
use ann::{settings, DimData, DimKind, LinearOrient, TextData, Justify};
use afrobim_draw::Pt2;

// 1. At start: register the modules with the app's registry.
ann::register(&mut app.modules);

// 2. Settings (units, annotation scale, styles) live in the document.
let mut s = settings::load(&app.doc);
s.annotation_scale = 500.0;                      // 1:500
s.units.length_decimals = 2;
settings::store(&mut app.doc, &s);

// 3. TEXT command: points are model millimetres.
let mut t = TextData::new("Existing kerb\nto be removed", Pt2::new(10_000.0, 5_000.0), &s.current_text(), s.annotation_scale);
t.justify = Justify::MiddleCentre;
t.rotation_deg = 15.0;
t.wrap_width = Some(30_000.0);                   // wrap to 30 m
t.frame_offset = Some(0.5);                      // frame, 0.5 text heights out
let id = ann::add_text(&mut app.doc, t, "NOTES")?;

// 4. DIM command: aligned dimension between two picks, line through a third pick.
let d = DimData::new(
    DimKind::Linear { p1, p2, line_pt, orient: LinearOrient::Aligned },
    &s.current_dim(),                            // style with the annotation scale applied
);
let dim = ann::add_dimension(&mut app.doc, d, "DIMS")?;   // Err(reason) if it cannot be drawn

// 5. Grips: the definition points, and moving one.
let mut a = ann::read_absolute(app.doc.find(dim).unwrap())?;   // points in model space
let grips = a.grips();                           // Vec<Pt2>
a.set_grip(1, new_pick);
ann::replace_absolute(&mut app.doc, dim, a)?;    // recomputes value, bounds, text

// 6. After renames, erases or alignment edits:
let report = ann::refresh(&mut app.doc);         // report.problems lists unresolved references
```

Other kinds, all `DimKind`: `Chain { mode: Continued | Baseline, pts, line_pt, orient }`,
`Angular { vertex, p1, p2, arc_pt }`, `AngularLines { a1, a2, b1, b2, arc_pt }`,
`Radius { centre, radius, at }`, `Diameter { .. }`, `ArcLength { centre, radius, start_deg,
end_deg, offset }`, `StationOffset { point, alignment, text_pt }`, `Level { point, value_m, source,
text_pt }`. `DimData::overrides[i] = Some("<> TYP".into())` overrides the text of value `i`
(`<>` stands for the measured text); the measured value stays in `DimData::measured`.

Leaders: `LeaderData::new(pts, LeaderText::Literal("R61".into()), &style)`, or
`LeaderText::Mark { element: id.0, cached }` to show another element's mark (name); `refresh` keeps
the text following the element.

## Text

Layout rules, stated so they can be checked (see `text.rs`): height is the capital height;
lines are `height * line_spacing` apart baseline to baseline; the block is `height + (n - 1) * pitch`
tall; the insertion point is the justification corner of the block (Top is the cap line of the
first line, Bottom the baseline of the last, Middle halfway; Left, Centre, Right the block's left
edge, middle and right edge); the block rotates about the insertion point. Widths come from the
Helvetica metrics already used by `afrobim-sheets`, stretched by the width factor. A frame is the
block grown by `frame_offset * height`; a mask is the same rectangle filled (white; not exported to DXF).

## Dimensions

Values are always recomputed from the definition points. Generation rules are in the module docs of
`dim.rs`; in short:

- Linear: value is `|(p2 - p1) . u|` along the chosen direction; aligned uses the direction of `p1 -> p2`.
  Extension lines start `ext_offset` from the point and run `ext_extend` past the dimension line.
- **Where text goes.** With `L` the dimension line length, `w` text width, `g` text gap, `a` arrow size:
  arrows fit inside when `L >= 2a`; text fits inside when `w + 2g <= L` (plus `2a` for centred text).
  If the text does not fit it moves outside past the second end, on an extension of the line. If
  the arrows do not fit they are drawn outside, pointing in, with a tail. A note is added whenever
  something moved.
- Angular: the angle containing the arc point is dimensioned (so the same three points give 90 or
  270 degrees depending on where the arc is picked). Two-line angular picks the sector the same way.
- Radius/diameter: centre mark, arrow on the circle, text on the line or outside when it does not fit.
- Arc length: `r * sweep`, the dimension arc `offset` outside the arc.
- Station and offset: the point is in **site millimetres**; the alignment's metres are the point's
  millimetres / 1000. Stations are the displayed stations (through station equations). Offset
  is positive to the right looking up-station, written `R` or `L`.
- Level: the value is supplied by the caller (typed, or sampled from a surface); `LevelSource` records which.

## Measuring (no element)

```rust
use ann::measure;
let u = &settings::load(&doc).units;
let r = measure::distance(a, b, u);              // r.measurement is structured, r.text is one line
println!("{}", r.text);                          // Distance = 5.000 m, dX = 3.000 m, ...
measure::angle(vertex, p1, p2, u)?;              // interior angle, 0 to 180
measure::radius_three_points(a, b, c, u)?;
measure::polygon(&pts, u)?;                      // area and perimeter
measure::cumulative(&picked, u)?;                // legs and running total
measure::on_alignment(&alignment, point_site_mm, u)?;   // station, offset, level
```

Area is the shoelace sum `A = 1/2 * sum(x_i * y_(i+1) - x_(i+1) * y_i)` with Y up: **positive when the
vertices run anticlockwise, negative when clockwise**. The result reports `|A|`, the signed value and
the winding. A ring that crosses itself gives the net area and is flagged.

## Display units

`DisplayUnits { length: Millimetre | Metre, length_decimals, station_decimals, area: SquareMetre |
Hectare, area_decimals, volume_decimals, angle: Decimal | Dms | QuadrantBearing | Azimuth,
angle_decimals }`, stored in the document. Formatters: `length`, `length_u`, `station`, `area`,
`area_u`, `volume`, `volume_u`, `angle` (a sweep), `direction` (a direction, where bearings apply).
Parsers for the same formats: `parse_length` (bare number in the display unit, `m` or `mm` suffix),
`parse_station` (`1+250.500`; after the `+` exactly three integer digits), `parse_area`, `parse_volume`,
`parse_angle` (`45.5`, `45d30m15s`, `45°30'15"`), `parse_direction` (`N 45d30'00" E`; a bare number is
an azimuth when the display format is a bearing format). Each parser accepts what its formatter writes
(tested round trip). Bearings measure from north, clockwise; internal directions are anticlockwise
from east.

## Layers

The kernel models a layer as a name with `visible` and `locked`. This crate adds, without touching
the kernel (`layers.rs`): `create`, `rename` (moves the elements and properties), `delete` with a
stated rule (`OnDelete::RefuseUnlessEmpty`, or `OnDelete::MoveTo(layer)`), `set_colour`,
`set_lineweight`, `set_visible`, `set_locked`, `list` (what a layer manager shows, without the
hidden settings layer). Layer `0` and the settings layer cannot be renamed or deleted. Names may not
contain the characters DXF forbids (`" ; > \ ` < = : / ? *`, the set ezdxf 1.4.5 refuses). Colour and
line weight live in the settings element.

## Output

- `output::export_dxf(&doc, &mut DxfDocument)` explodes annotations to R12 entities: LINE, ARC,
  POLYLINE, SOLID (arrow heads), and one TEXT per text line with rotation, alignment and width
  factor. Layer colours become AutoCAD colour indices. Masks are not exported (R12 has no wipeout) and
  the report says so. A dimension is no longer a dimension in the file: that is a limit of R12.
- `output::sheet_drawing(&doc, offset, 1.0 / denominator, window)` returns an
  `afrobim_sheets::SheetDrawing` (primitives in paper millimetres and `SheetLabel`s), the form the sheets
  preview consumes, with no edit to that crate.
- `Generated.drawing` (what the canvas draws) includes the text as a **single-stroke preview**
  (`font.rs`, this crate's own 4 by 6 glyphs scaled into the Helvetica advance widths) because the
  draw crate has no text primitive. A front end that can draw real text should use
  `annotation_items(&doc)` (labels with anchors) and skip the preview, or accept the preview. Lines
  are hairlines (colour `None`, so the canvas ink colour); arrow heads are black fills.

## Limits

- Text is laid out with Helvetica metrics whatever font a style names; the font name is carried for
  an application to map. No kerning, no tabs, no inline formatting in created text.
- Dimension text is plain text; no tolerances, no stacked fractions, no alternate units.
- Station/offset dimensions assume site millimetres equal alignment metres times 1000.
- Wrapping breaks at spaces; a word wider than the width is cut between characters.
- Kernel gaps the app should know: layer `visible` and `locked` are not in the kernel's undo
  snapshot (elements are, so annotations, styles and layer colours are); the kernel picks by element
  bounds only, so a pick inside an annotation's box selects it even between its strokes.
