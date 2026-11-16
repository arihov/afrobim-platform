# afrobim-junction

At-grade intersections and roundabouts for AfroBIM, modelled on the Civil 3D intersection and
roundabout tools. **Engine only**: it takes alignments (and profiles, assemblies) and returns new
alignments, profiles and a corridor description built from the same types, with findings, tables
and plain drawing geometry. Nothing is drawn or stored by this crate.

## No design value is built in

Every kerb radius, lane edge offset, widening, taper, crossfall, flare length, entry width, island
dimension, tie-in length and spacing is an **input**. A quadrant with both arms and no return
definition is refused; so is a missing profile, crossfall or assembly. The checks report values and
compare them with nothing: **no limit is applied anywhere**.

Documents checked for values: the repository holds no geometric design manual in a readable form.
`Manuals/` has the UK traffic sign manuals and TSRGD (some are Git LFS pointers) and a sign
program's help; `packs/` has sign data. `afrobim-design` cites the Uganda MoWT Road Design Manual
Volume I (January 2010), but that PDF was fetched from a third-party site by an earlier round and is
not in the repository, and `afrobim-visibility` cites sign siting rules only. **No value in this crate
comes from a document**, and none is taken from memory. Where a standard defines a term (entry
angle, entry path radius, flare ratio) this crate defines its own, says so, and applies no limit.

## Conventions

Metres, radians, `f64`, plan X east and Y north, coordinates exactly as given (the tests run at
easting 450 000, northing 9 960 000). Stations are internal stations. Offsets are positive to the
right looking up-station. Cross-fall is in percent, positive where the surface rises away from the
centreline (a crowned road is negative on both sides), as in the alignment and corridor crates.
Everything derives `Serialize` and `Deserialize`.

## Use

```rust
let inputs = IntersectionInputs { name, primary: "Main".into(), secondary: "Side".into(), /* ... */ };
let result = build_intersection(&inputs, &alignments)?;       // IntersectionResult
result.crossings_table().to_csv();  result.returns_table().to_csv();  result.drawing(0.25);
let corridor = result.corridor.unwrap();                       // CorridorModel + alignments
afrobim_corridor::build_model(&corridor.model, &corridor.alignments, &ground)?;
let roundabout = build_roundabout(&roundabout_inputs, &alignments)?;
```

* `find_crossings(a, b, extend_m)`: every crossing, with stations on each, heading, the acute
  angle and the skew from a right angle, which arms exist, and `Cross`, `Tee` or `Other`. Each hit
  is refined on the true elements (lines, arcs and clothoids) to 1e-11 m. A secondary that stops
  short can be found with `extend_m`, but the build refuses a crossing on an extension.
* `IntersectionInputs`: primary and secondary by name, crossing index, `CorridorType`
  (`PrimaryCrownMaintained` or `AllCrownsMaintained`), lane edge offsets each side of each road
  with widening tables (`Widening`: start, entry taper, full length, exit taper, width), junction
  extents, one `ReturnSpec` per quadrant, islands by station and offset, and optional `VerticalInputs`
  and `CorridorInputs`.
* `IntersectionResult`: the four full edge alignments, the kerb returns, kerb lines (the edges
  trimmed at the returns), islands, the locked secondary profile, the corridor description, findings,
  and a `DependencyRecord`.
* `DependencyRecord::stale(&alignments, &assemblies)` lists what changed since the build (a 64-bit
  FNV-1a hash of each alignment's plan, profile, superelevation and each assembly), so the
  application knows when to rebuild. The inputs name the alignments; they never hold them.

## Constructions

The derivations are in the module comments (`geom.rs`, `crossing.rs`, `edges.rs`, `kerb.rs`,
`vertical.rs`, `corridor.rs`, `roundabout.rs`); the short form:

**Offset of an element.** The offset `d` to the right has the same heading at the same `s`, speed
`1 + k d`, length `L + d (theta1 - theta0)` and curvature `k / (1 + k d)`: exact for straights and
arcs. A clothoid is cut into pieces of at most 0.01 rad, each an element with the exact offset
start, heading and end curvatures; the joint gaps (micrometres) are reported.

**Edges.** Road offset by `base + widening(s)`. Constant-offset stretches are exact offsets; each
taper is one straight between its end points (exact on a straight road, a chord on a curve; the
deviation is reported).

**Kerb return, simple.** The circle tangent to both edges, outside the paved cross, has its centre
where the two edges, each offset outward by `R`, cross; tangent points are the feet from the centre.
For a right-angle cross with edges at offsets `d1` and `d2`: corner `(d2, d1)`, centre
`(d2 + R, d1 + R)`, tangent points `(d2 + R, d1)` and `(d2, d1 + R)`, arc length `R pi / 2`, tangent
lengths `R`. In general the tangent length is `R / tan(theta/2)` for interior angle `theta`. Works on
curved edges.

**Kerb return, three-centred compound** (this crate's construction). Radii `R1, R2, R3` and offsets
`o1`, `o3` (distance of the end of the first arc and the start of the last arc from their edges):
`phi1 = acos(1 - o1/R1)`, `phi3 = acos(1 - o3/R3)`, `phi2 = Delta - phi1 - phi3` must be positive; the
start along the first edge follows from closing on the second edge. It takes both edges as the straight
lines tangent to them at the corner and reports the distance of the tangent points from the real edges.
Three equal radii with `o = R (1 - cos 30 deg)` are the simple 90 degree curve (a test).

**Chamfer.** A straight between points at entered distances from the corner along each edge.

**Secondary profile locked to the primary.** With the primary crown maintained, the secondary's
level across the primary carriageway is the primary surface (`profile + crossfall x |offset|`) along
the secondary centreline: PVIs at the two edge crossings and the crown (and optionally at a regular
spacing between), then a tie-in of the entered length back to the secondary's own profile. The tie-in
grades and the shift of the crown level from the secondary's own level are reported. With all crowns
maintained nothing is changed and the difference of the two levels at the crossing is reported.

**Edge profiles.** Surface level at the edge as above, sampled to PVIs at the entered spacing.
The secondary edges use the locked profile and the secondary crossfall.

**Return profile blend.** Levels `z0, z1` and grades `g0, g1` along the return from the edges at the
tangent points (a grade is negated where the return runs against its edge's stationing). If the end
tangent lines meet inside the return at `s* = (z1 - z0 - g1 L)/(g0 - g1)` the profile is **one PVI with
an asymmetric parabola** over the whole return, which meets both levels and both grades exactly.
Otherwise it is the **Hermite cubic** through both levels and grades, sampled to PVIs (the `Profile`
type cannot hold a cubic). A straight grade is two PVIs. **Low points** are where the grade changes
from negative to non-negative, bisected to 1e-12 m; with none, the lower end is reported.

**Corridor description.** One baseline on the primary (approach behind, through, approach ahead), one
per existing secondary arm (mouth, approach), one per return. Assemblies come by name from a library
you pass; a target slot in an assembly (an empty `Polyline` offset target, an empty `Table` or
`Profile` elevation target) is filled with the edge polyline or the edge levels at the baseline's
stations. A return's assembly is written for its paved side as its **right** side and placed on the
side the pavement is on; its width runs to the corner line (tangent point, corner, tangent point), so
the region is the sliver between the arc and the edges.

**Roundabout.** Defined in `roundabout.rs` (inscribed circle, carriageway, apron, central island,
entry and exit kerbs as a flare line and a fillet arc tangent to the inscribed circle, splitter with a
rounded nose). Reported: entry angle (arm axis against the circulating flow at the give-way point),
entry and exit widths and radii, flare length and `(e - v)/l`, the angle to the next arm in the direction
of circulation, and, when the three offsets are entered, the **entry path radius**: the circumradius
of three points (flare start in from the kerb, give-way line out from the splitter, the bisector to the
next arm out from the central island). That path is this crate's construction, **not** a published fastest
path method.

## What it cannot do (limits)

* **No design check against limits.** Skew, curvature, grade, kerb radius, widths: values only.
* A crossing on an extension (a secondary that stops short of the primary) is refused by the build.
* Compound returns assume straight edges near the corner. Returns are built for the four quadrants of
  one crossing; no roundabout-to-intersection mixing, no multi-leg junction beyond two crossing roads.
* The vertical part needs profiles on both roads, and, in the corridor description, the vertical inputs.
  The locked profile is piecewise linear across the primary carriageway (more PVIs if you enter a spacing).
* A widening taper on a curved road is a straight chord; the offset of a spiral is piecewise.
* The edge level is the road's crossfall plane extended to the edge, including a widened edge.
* Corridor limits (also returned in `JunctionCorridor::missing`): no link takes an offset target and an
  elevation target together, so a return's sliver gets its width from the corner line but its level
  from the assembly's own slope (the edge levels are given as tables keyed by the baseline station, so
  the implied cross slope can be entered); `ElevationTarget` reads the baseline's station only; the
  corridor crate has no region bounded by an outline, so islands are returned as outlines only; regions
  are laid out by station, so on a skewed junction the wedge between a secondary arm and the primary
  edge is not clipped between regions.
* `end_inset_m` (an input) moves the first and last section of each region in from its end. A section
  line through the end vertex of a target polyline meets it or not by rounding, and a return's sliver has
  no width at its tangent points. On a curved edge the target polyline is a chord of step `h` with
  sagitta `h^2 / (8 R_edge)`, so the inset must exceed about `h sqrt(R_return / (4 R_edge))`.
* Roundabout kerbs assume the arm alignment is straight near the circle (a deviation finding says by how
  much it is not); an entry whose flare line passes farther from the centre than the inscribed radius has
  no tangent entry arc and is refused. Splitter nose roundings need the last side to be long enough.
* Nothing is stored in a document: no kernel element, no IFC, no LandXML export of the results.

## Tests

`cargo test -p afrobim-junction`: 42 tests on closed forms with the working in the test comments:
a right-angle cross with unequal offsets (tangent points, centre, arc length `R pi / 2`), skewed
crosses (`R / tan(theta/2)`), returns on a curved primary (exact tangency), three equal compound arcs
equal to the simple curve, 3-4-5 chamfer, line and arc crossings against the circle equation, arc
offsets, the locked profile and the return blend against hand calculation including the low point of an
asymmetric parabola, circles and tangent entry and exit arcs of a roundabout (end on the circle on the
tangent heading), nose fillet closed form, entry angles `acos(+-0.75)`, the entry path circumradius,
`build_model` run on the corridor description, JSON round trips, CSV, dependency staleness.
