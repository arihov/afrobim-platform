# afrobim-supports

Sign posts and foundations as AfroBIM model elements: definitions, 3D solids, a sheet
elevation, validation notes and quantities.

## What this does not do

**This crate is not a design tool.** Read this section before relying on anything it produces.

- **No structural design.** It does not size, check or rate a post, a foundation, a fixing or a
  connection. It does not know whether one post or three is enough.
- **No wind loading.** No wind speed, pressure, exposure, drag factor or load combination is
  held or computed.
- **No standard section tables.** There is no catalogue of tubes, hollow sections or rolled
  shapes, no steel grades, no concrete classes, no embedment rules and no clearances. A section
  is whatever dimensions the engineer types in.
- **No compliance claim.** No dimension, default, count or spacing here is presented as meeting
  any standard, and no clause is cited. Material, finish and concrete are free text that the
  crate stores and prints but never interprets.
- **No rates or costs.**

Every dimension is entered by the engineer. The crate draws those dimensions and measures them
by plain geometry. If a number looks reasonable, that is because someone entered it, not because
the software checked it.

### The built-in numbers

There are exactly two kinds, and neither is a specification value.

| Constant | Value | What it is |
|---|---|---|
| `PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM` | 100 | Round starting value so a new post can be drawn |
| `PLACEHOLDER_POST_WALL_MM` | 10 | as above |
| `PLACEHOLDER_POST_LENGTH_MM` | 3000 | as above |
| `PLACEHOLDER_POST_EMBEDMENT_MM` | 1000 | as above |
| `PLACEHOLDER_FOUNDATION_SIDE_MM` | 1000 | Side of the starting foundation cube (top at ground, no blinding, no socket) |
| `STEEL_DENSITY_KG_M3_NOMINAL` | 7850 | Conventional nominal density of structural steel for mass estimates; callers override it |

`PostDefinition::placeholder()` and `FoundationDefinition::placeholder()` are the only users of
the `PLACEHOLDER_*` values. They set `placeholder: true` in the definition, and while that flag
is set the module raises the Info note `PLACEHOLDER_DIMENSIONS` on the element ("The post
dimensions are placeholders ... the engineer must set the section, length and embedment"). The
editor that lets the engineer enter real values must clear the flag. `SupportArrangement` has no
placeholder or default mounting height: the caller must supply one.

## What is modelled

- **`PostDefinition`**: a `Section`, total `length_mm` (including the buried part),
  `embedment_mm` (length below ground), free-text `material` and `finish`.
- **`Section`**, described only by entered dimensions:
  `Chs { outside_diameter_mm, wall_mm }`,
  `Rhs { width_mm, depth_mm, wall_mm, outer_corner_radius_mm, inner_corner_radius_mm }` (radii
  default to 0, i.e. square corners),
  `ISection` and `Channel { depth_mm, flange_width_mm, web_mm, flange_mm }` (square corners, no
  root radii), `SolidRound { diameter_mm }`, `SolidRect { width_mm, depth_mm }`.
- **`FoundationDefinition`**: `shape` (`Block { length_mm, width_mm, depth_mm }` or
  `Cylinder { diameter_mm, depth_mm }`), `top_below_ground_mm`, free-text `concrete`, optional
  `blinding_mm`, optional `socket` (`Round { diameter_mm, depth_mm }` or
  `Rect { length_mm, width_mm, depth_mm }`, a void centred in plan running down from the top).
- **`SupportArrangement`**: mounting height to the sign's lower edge, number of posts, edge
  inset, optional top offset and setback behind the face. It gives post positions and the post
  length; it does not say whether the arrangement is adequate.
- **`PostModule`** (category `"post"`) and **`FoundationModule`** (category `"foundation"`)
  implement `afrobim_kernel::Module`.
- **Hosting**: `add_supports`, `replace_supports`, `supports_of`, `remove_supports`,
  `sync_support_sites`.
- **Quantities**: `PostQuantities`, `FoundationQuantities`, `SupportQuantities` and CSV writers.

The definitions are stored as JSON in the element's `data`, with a `"type"` tag on each enum:

```json
{ "section": { "type": "Chs", "outside_diameter_mm": 88.9, "wall_mm": 4.0 },
  "length_mm": 4000.0, "embedment_mm": 800.0, "material": "...", "finish": "..." }
```

## Frames and conventions

All lengths are millimetres.

**Solids** follow `afrobim_draw::Mesh3`: the element's own frame, +X along its width from its
lower-left corner, +Y into its depth, +Z up. For supports **z = 0 is ground level**:

- a post runs from z = −`embedment_mm` to z = `length_mm` − `embedment_mm`;
- a foundation's concrete runs from z = −(`top_below_ground_mm` + depth) to
  z = −`top_below_ground_mm`, with a separate solid named `Blinding` beneath it when there is
  blinding.

The plan bounding box has its corner on the origin: a post whose section is `w` wide and `d`
deep occupies x ∈ [0, w], y ∈ [0, d], and its axis is the vertical through (w/2, d/2). A
foundation occupies x ∈ [0, length], y ∈ [0, width] (a cylinder [0, D] × [0, D]).

Section orientation: `width` dimensions run along +X (parallel to the sign face), `depth`
dimensions along +Y (away from the face). An I-section or channel has its flanges parallel to X;
the channel's web is on the x = 0 side.

**Display solids are not the measured solids.** A hollow post is shown as its outer solid, a
round as a 48-sided prism (0.29 % under the true cylinder, never over), a rounded hollow-section
corner as square, and a socket is not cut out of the foundation. Quantities never come from a
mesh.

**2D drawing**: an elevation looking along +Y (the side a sign face is read from). Drawing X is
the frame's X, drawing Y is height, the lowest point of the element is on y = 0 and its left
edge on x = 0. A post shows its outline, the inside faces of a hollow section and a line at
ground level. A foundation shows the concrete, the blinding and the socket outline; the cover
between the top of the concrete and ground is not drawn.

**Element size**: a post element is section width × length; a foundation element is plan length
× (depth + blinding).

**Site placement.** A sign's `SitePlacement` is read as *the point on the ground vertically
below the sign's lower-left corner*; the face is `mounting_height_mm` above it. With θ =
`rotation_deg` (counter-clockwise from site +X, as the kernel documents), a point `u` along the
sign's width and `v` behind its face is at

```
x = sign.x + u·cos θ − v·sin θ
y = sign.y + u·sin θ + v·cos θ
```

Each support takes the sign's rotation and `z`, and is placed by the corner of its plan bounding
box: a post with axis at (u, v) is placed at (u − w/2, v − d/2). A foundation is centred on its
post's axis. Supports do not copy the sign's alignment, station or offset. After moving a sign,
call `sync_support_sites`.

This reading of the sign's `z` is an assumption of this crate: the kernel does not yet say
whether a sign's `z` is ground level or the level of its lower edge.

**Sheet layout** by `add_supports`: an exploded elevation to the right of the sign. Posts in a
row, then foundations, `Document::ARRANGE_GAP` apart, all drawn to one ground line
`mounting_height_mm` below the sign's lower edge.

**Hosting**: a post's `host` is its sign; a foundation's `host` is its post. Names come from the
sign's mark: `ADS-001-P1`, `ADS-001-F1`, numbered from the left. Layer `Supports`.

## Post positions and length

- One post: on the sign's centre line, `width / 2`. The inset is not used.
- `n ≥ 2` posts: evenly spaced between the insets,
  `x_i = inset + i · (width − 2·inset) / (n − 1)`.
- Post length = embedment + mounting height + sign height + top offset (top offset is the post
  top relative to the sign's upper edge; zero by default). `add_supports` writes this length
  into each post, replacing the `length_mm` of the definition passed in.

## How quantities are computed

### Posts

| Quantity | Formula |
|---|---|
| Section area, CHS | π/4 · (D² − (D − 2t)²) |
| Section area, RHS | (b·d − (4 − π)·ro²) − ((b − 2t)(d − 2t) − (4 − π)·ri²) |
| Section area, I-section and channel | 2·b·tf + (d − 2·tf)·tw |
| Section area, solids | π/4 · D², or b·d |
| Volume | section area × total length |
| Coating area | outside outline of the section × total length |
| Coating area above ground | outside outline × (length − embedment) |
| Mass | volume × the density the caller passes |

Outside outline: π·D for rounds; 2(b + d) − (8 − 2π)·ro for RHS; 4·b + 2·d − 2·tw for an
I-section or channel. **Coating area is outside faces only**: the bore of a hollow section and
the two cut ends are excluded. For an open section every face is outside.

Limits of the geometry: an I-section or channel has no root radii and an RHS has square corners
unless the radii are entered, so areas differ slightly from a manufacturer's tabulated values.
Caps, base plates, brackets, clips and fixings are not modelled or measured.

### Foundations

| Quantity | Formula |
|---|---|
| Gross volume | plan area × depth (L·W·h, or π/4·D²·h) |
| Socket void | π/4·d²·h, or l·w·h |
| Concrete | gross volume − socket void |
| Blinding | foundation plan area × blinding thickness |
| Excavation | plan outline grown by the working space on every side × (top below ground + depth + blinding) |
| Side face area | plan perimeter × depth |

The excavation is a plain vertical-sided prism with the caller's working space (default 0): no
batter, bulking or overbreak. The side face area is the formwork area *if* the sides are formed;
whether they are is the engineer's call. Concrete is not reduced for a post cast directly into
it; only an explicit socket is deducted. Reinforcement, backfill and disposal are not measured.

### Schedules

`SupportQuantities::measure(doc.elements(), &QuantityOptions { density_kg_m3, working_space_mm })`
measures every post and foundation. Elements whose definition is unreadable or has Error notes
go to `skipped` with the reason and are in no total. `posts_to_csv`, `foundations_to_csv` (each
ending in a `TOTAL` row with the count in the second column) and `summary_to_csv` write the
result. Rounding: mm and mm² to at most 1 decimal, post volume to 6 decimals, areas and concrete
volumes to 3, mass to 2. Totals are summed unrounded and rounded once.

## Validation

`generate` returns Error notes, and no geometry, for:

| Code | Case |
|---|---|
| `SECTION_DIMENSION_NOT_POSITIVE` | any section dimension ≤ 0 or not a number |
| `SECTION_WALL_TOO_THICK` | wall ≥ half the diameter (CHS) or half the smaller side (RHS) |
| `SECTION_CORNER_RADIUS_INVALID` | negative radius, radius that does not fit, or radii that leave no wall at the corner |
| `SECTION_FLANGE_TOO_THICK` | 2 × flange ≥ depth |
| `SECTION_WEB_TOO_THICK` | web ≥ flange width |
| `POST_LENGTH_NOT_POSITIVE` | length ≤ 0 |
| `POST_EMBEDMENT_NEGATIVE` | embedment < 0 |
| `POST_EMBEDMENT_NOT_LESS_THAN_LENGTH` | embedment ≥ length |
| `FOUNDATION_DIMENSION_NOT_POSITIVE` | any foundation dimension ≤ 0 |
| `FOUNDATION_TOP_ABOVE_GROUND` | `top_below_ground_mm` < 0 |
| `FOUNDATION_BLINDING_NOT_POSITIVE` | blinding given but ≤ 0 |
| `SOCKET_DIMENSION_NOT_POSITIVE` | any socket dimension ≤ 0 |
| `SOCKET_DEEPER_THAN_FOUNDATION` | socket depth > foundation depth |
| `SOCKET_WIDER_THAN_FOUNDATION` | socket does not fit inside the plan outline |

Info notes: `PLACEHOLDER_DIMENSIONS`, `POST_MATERIAL_NOT_STATED`,
`FOUNDATION_CONCRETE_NOT_STATED`.

These checks are about whether the shape can exist. None of them is a structural or standards
check, and a post that passes them all may still be entirely unsuitable.

## Verification

There is no reference implementation for supports, so the tests check against arithmetic worked
by hand in the comments beside each assertion: section areas and outlines, mesh volumes against
the same formulae (with the prism tolerance stated), post positions for 1 to 4 posts, the post
length build-up, foundation volume less a socket, exact CSV text, a save and reload through
`Document`, every validation case, and site placement for signs at 0°, 90° and 30°.

```
cd afrobim && cargo test -p afrobim-supports
```
