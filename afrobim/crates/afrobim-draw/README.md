# afrobim-draw

Neutral 2D drawing primitives and 3D solids shared by every AfroBIM module and front end. A module
turns an element into a `Drawing` (a flat list of `Primitive`s, Y up, in whatever unit the module
works in) and, for the 3D view, the IFC export and quantities, into `Mesh3` solids. The desktop
canvas, the DXF and PDF writers and the sheets all consume the same description, so a sign or a
marking is drawn, exported and fabricated from one source. The crate depends on `serde` only.

## Types (all in `src/lib.rs`)

| Type | What it is |
|---|---|
| `Colour` | The eight named sign colours (white, black, green, blue, brown, yellow, red, grey). `display_rgb()` is an on-screen approximation, not a specification value: the standards give colours by name |
| `Pt2` | A 2D point with `+`, `-` and `distance_to` |
| `Primitive` | `Fill` (a closed, filled outline, may be concave), `Line` (a band of a width with square ends), `Arc` (a centreline, degrees anticlockwise from +X; a full turn is a circle), `Glyphs` (lettering as closed contours filled even-odd), `Symbol` (a placeholder for artwork not yet resolved to geometry) |
| `Drawing` | `primitives` and `extents`. `scaled_to_origin(k)` multiplies every coordinate and width by `k` and moves the lower-left extent to the origin (used to bring a sign from stroke widths to millimetres) |
| `Extents` | `min`, `max`, with `width()` and `height()` |
| `Mesh3` | A coloured triangle mesh in millimetres, Z up, in the element's own frame (X along the width from the lower-left corner, Y into the depth, Z up). Triangles are counter-clockwise seen from outside. `cuboid`, `cylinder` (a prism of a chosen number of sides) and `volume` (the signed volume, meaningful for a closed mesh) |

## How it is tested

Two unit tests in `src/lib.rs`: a cuboid has the volume of its box and a 64-sided prism is within
0.2 % of the cylinder it stands for and never larger; `scaled_to_origin` moves the lower-left
extent to the origin and scales coordinates and widths. The other crates test their own drawings
(the sign engine against the C# engine's golden fixtures, for example).

## Limits

- No units are carried: a `Drawing` is in the unit of the module that made it (stroke widths for a
  sign layout, millimetres for most others). The producer and the consumer must agree.
- `Mesh3` does not check that it is closed or sound; the IFC export and the quantity code test the
  meshes they take. `volume` of an open mesh is not a volume.
- `Symbol` carries no geometry; a consumer that cannot resolve an `id` draws nothing for it.
- `Colour::display_rgb` values are display approximations only.
