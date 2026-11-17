# afrobim-dxf

Two halves.

1. **Writer** (crate root): R12 (AC1009) ASCII DXF of AfroBIM drawings in millimetres. Unchanged:
   its output is byte-identical to the C# writer and `tests/golden.rs` is untouched. Additive
   methods only: `add_arc`, `add_solid` and `add_text_scaled` (TEXT with a width factor).
2. **Reader** (`afrobim_dxf::reader`): reads an ASCII DXF from AutoCAD or Civil 3D (R12 through the
   current release) as a **read-only underlay**, and produces a drawing for display and snap points.

No DXF written by this crate has been opened in AutoCAD. It has been read back by ezdxf and by the
reader below, and the reader has been checked against ezdxf on generated files and on real files
written by AutoCAD and other programs (see Verification).

## Reader API (copy-ready)

```rust
use afrobim_dxf::reader::{self, DisplayOptions, SnapOptions};

// Errors say what to do: binary DXF -> save as ASCII; DWG without ODA File Converter -> install it or convert to DXF.
let mut under = reader::read_file(std::path::Path::new("site.dxf"))?;

// What was and was not read.
println!("read {:?}", under.report.read);        // by DXF type
println!("NOT read {:?}", under.report.unread);  // by DXF type, with reason in the key
println!("warnings {:?}", under.report.warnings);

// Units come from the header ($INSUNITS); override when the file is wrong or silent.
if under.units.assumed { /* ask the user */ }
under.unit_scale_mm = 1000.0;                    // the file is in metres

// Layers: name, colour, frozen, off, locked. Entities refer to them by index.
for l in &under.layers { println!("{} aci={:?} visible={}", l.name, l.aci, l.visible()); }

// A Drawing for the canvas, in model millimetres, placed by an offset.
let view = under.to_drawing(&DisplayOptions {
    offset_mm: afrobim_draw::Pt2::new(0.0, 0.0),
    hidden_layers: vec!["TOPO-SPOT".into()],     // the layer manager's switches
    tolerance_mm: 5.0,                           // curve chord tolerance; choose from zoom
    ..Default::default()
});
// view.drawing: afrobim_draw::Drawing (hairlines, arcs, fills)
// view.labels: texts to draw (the draw crate has no text primitive)

// Object snap.
let snaps = under.snap_points(&DisplayOptions::default(), &SnapOptions::default());
for s in &snaps.points { /* s.p in mm, s.kind: End, Mid, Centre, Quadrant, Node, Intersection */ }
```

Call `to_drawing` again when a layer switch, the unit scale or the zoom-dependent tolerance changes;
it does not touch the file. `Underlay::flatten(tol)` returns the expanded geometry as polylines (used
by the tests, handy for hit testing).

## What is read

Group codes were taken from ezdxf 1.4.5 (`DXFATTRIBS` of each entity class, `boundary_paths.py`,
`const.py`, `units.py`, `tools/text.py`); the header of `reader/parse.rs` has the table. The Autodesk
DXF Reference pages on help.autodesk.com are rendered by script and could not be fetched here, so they
are **not** cited as verified.

| Read | Notes |
|---|---|
| LAYER table | name, colour (negative means off), frozen (flag 1), locked (flag 4), true colour |
| LINE, CIRCLE, ARC | mirrored extrusion (0,0,-1) handled exactly; other extrusions are counted as not read |
| LWPOLYLINE, POLYLINE | bulges become arcs: bulge = tan(included angle / 4); derivation and tests in `reader/geom.rs`. 2D, 3D (plan view). 3D mesh and polyface are counted as not read. Spline/curve-fit polylines draw their stored curve vertices |
| ELLIPSE | polyline approximation, chord tolerance as set in the options (default 5 mm in model space) |
| SPLINE | evaluated as a NURBS (Cox-de Boor, with weights) from degree, knots and control points, tested against hand-worked values and against ezdxf. A spline without valid knots draws its control polygon, one with fit points only draws a polyline through them, each with a warning |
| TEXT, MTEXT, ATTRIB | plain text and placement as labels. MTEXT codes removed or rewritten are listed in `report.mtext_stripped` (see `reader/mtext.rs` for the table): paragraphs and columns (line breaks), `\~`, escapes, braces, underline/overline/strike, `\H \W \F \f \C \c \T \Q \A \p ... ;`, stacking (written `a/b`), `\U+XXXX`, `%%d %%p %%c %%%`, `%%u %%o %%k`, caret codes |
| POINT | drawn as a small cross |
| INSERT | scale, rotation, base point, MINSERT arrays (offset turned by the rotation, not scaled), nesting to a depth limit (default 16), mirrored scale, and layer `0` and by-block inheritance (implemented from the usual CAD rule; ezdxf does not apply it, so only this crate's own test checks it). DIMENSION is drawn through its anonymous block |
| SOLID, TRACE, 3DFACE | outlines (filled) |
| HATCH | boundary loops as closed outlines: polyline paths (with bulges), line, arc, ellipse and spline edges |
| header | `$ACADVER`, `$INSUNITS`, `$MEASUREMENT`, `$EXTMIN`, `$EXTMAX` |

## What is not read

Counted by DXF type in `report.unread`: LEADER, MULTILEADER, TOLERANCE, IMAGE, WIPEOUT, VIEWPORT,
XLINE, RAY, REGION, 3DSOLID, BODY, MESH, OLE2FRAME, ATTDEF (a definition, not drawn), 3D mesh and
polyface polylines, entities with an oblique extrusion, entities switched invisible, and anything
else. Paper space entities are skipped and counted (`paper_space_skipped`). Z is dropped (plan
view). Line types, line weights, hatch patterns and fills, text fonts and styles, dimension
styles and xrefs are not read. Binary DXF is refused with a message. Entity handles and objects
other than the LAYER table are ignored.

**Proxy graphics** (`reader::proxy`): an `ACAD_PROXY_ENTITY`, which is how a converter writes an
object of a class it does not know (every Civil 3D object), is drawn from the picture its program
saved: polylines, polygons, circles, arcs, elliptic arcs, shells (as faces), meshes (as lines),
text, light-weight polylines in the DWG bit stream, with the layer, colour and transform commands
applied. The layout follows `ezdxf`'s `proxygraphic.py`. Construction lines, tilted circles and
arcs, and commands of other types are counted as not read; an empty picture, or the stand-in box a
program writes when it saved none, draws nothing and is counted.

**DWG** (`reader::dwg`): `read_file` converts a DWG to DXF 2018 with ODA File Converter when it
is installed (`AFROBIM_ODA_CONVERTER`, the usual install folders, then `PATH`), in a cache folder
keyed by the file's path, size and modification time, and reads the DXF. Without the converter, or
for `read_bytes`, a DWG is refused with instructions. The converter is never bundled.

Tried on two real Civil 3D drawings (AC1032) converted with ODA File Converter 27.10 on macOS: a
topographic survey (36 887 proxies; all drawn except 4 stand-in boxes) and a road model (601
proxies; 233 empty pictures).
Text in a code page other than UTF-8 is decoded as Latin-1.

## Units

`$INSUNITS` is mapped with the codes of `ezdxf.units` to a millimetres-per-unit proposal in
`under.units`. Where the file does not say (unitless or absent), metres are assumed for a metric file
and inches for an English one, `units.assumed` is true and the user should confirm; set
`under.unit_scale_mm` to override.

## Snap points

Endpoint, midpoint, centre (arcs, circles, ellipses, bulge arcs), quadrant (circles), node (points,
text and block insertion points) and intersections. Intersections are between straight segments
(lines, polylines, and curves flattened to `curve_tolerance_mm`, default 2 mm, so intersections with
a curve can differ from the true ones by about that much), found with a grid. Budgets in `SnapOptions`:
`max_points` (default 2 000 000), `max_intersection_points` (100 000), `max_pair_tests` (5 000 000),
`max_segments` (2 000 000). When one stops the work, `SnapSet::truncated_*` says so; what was found
before is valid.

## Large files: measured

Debug profile with `CARGO_PROFILE_DEV_OPT_LEVEL=3` (no release build was made), Linux, one core:

| file | size | entities | read time | peak memory |
|---|---|---|---|---|
| generated by ezdxf (lines, bulged polylines, arcs, circles, text), AC1032 | 112.5 MB | 600 000 | 1.4 to 1.5 s (about 75 to 83 MB/s) | 173 MB |

Building the drawing for that file at 5 mm tolerance took 0.5 to 0.6 s and 5.7 million primitives
(peak process memory 635 MB); at 0.5 mm the same file gives 16.4 million primitives and 1.5 GB, which
is why the default tolerance is 5 mm and a front end should choose one from its zoom. For 200 000
random lines, snap points (700 000 including 100 000 crossings) took 0.13 s and 107 MB.
`cargo run -p afrobim-dxf --example dxfstat -- FILE.dxf --drawing --snaps` prints these.

## Verification

- `tests/reader.rs`: hand-written DXF text for every entity type, with the expected numbers worked in
  the comments (bulge arcs, block placement and arrays, units, layers, binary and DWG refusal,
  snap points, round trip of this crate's own writer).
- `reader/geom.rs` tests: bulge geometry derived from its definition, the arbitrary axis algorithm, NURBS
  (Bezier, rational quarter circle, cubic with interior knots).
- `tests/ezdxf_crosscheck.rs` with `tests/ezdxf_gen.py`: builds a file with ezdxf in R12, R2000,
  R2010 and R2018 covering lines, arcs, circles, mirrored extrusions, bulged polylines (old and new
  style), 3D polylines, solids, 3D faces, ellipses, splines (open, rational, fit-derived), hatches
  (polyline and edge paths, clockwise arc edges), text and MTEXT, blocks with scale, rotation, mirror,
  non-uniform scale, arrays and nesting, and a rendered dimension. The reader's geometry and ezdxf's
  own flattening (0.001 units) are compared both ways (distance below 0.01) and every entity must match.
  Skipped, with a printed note, when Python 3 or ezdxf is missing.
- Real files: set `AFROBIM_DXF_SAMPLES` to a directory of `.dxf` files and
  `real_world_samples_match_ezdxf` compares each with ezdxf. Run here on five files from the MIT-licensed
  gdsestimating/dxf-parser test data (not committed): `ellipse.dxf`, `splines.dxf`, `polylines.dxf`
  (LaserWeb) and `mtext-test.dxf` (AutoCAD 2013, AC1027) matched ezdxf; `blocks.dxf` (319 blocks, MTEXT
  with formatting, nested inserts, a MULTILEADER counted as not read) cannot be read by ezdxf and was
  only read by this reader.
