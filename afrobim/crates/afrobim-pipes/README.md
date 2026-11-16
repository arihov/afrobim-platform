# afrobim-pipes

Gravity pipe networks for road and site drainage: the pipe network object of a civil design
package (modelled on the gravity pipe network of Civil 3D), as an engine with no user interface,
tuned for BIM (parts with IFC classification and cost codes, exact coordinates, plain data out).

Everything this crate produces is **a calculation for an engineer to check. It is not a certified
design.**

## No engineering defaults

The crate holds **no** pipe size, roughness, minimum or maximum cover, velocity, slope, manhole
spacing, drop, trench width, bedding, clearance or viscosity. Each is an input:

* sizes, materials, wall thicknesses and roughness come from a parts catalogue the project loads
  (JSON or CSV), or are typed in;
* limits come from a `RuleSet`; `RuleSet::default()` has every rule off, and a rule that is off is
  not checked;
* the Colebrook-White kinematic viscosity, a structure loss coefficient K, a clearance, a tailwater
  level and every quantity input are arguments.

The only constants are definitional (unit conversions in the LandXML reader) and standard gravity
9.81 m/s2, taken from `afrobim-drainage`. The files under `tests/data` are labelled examples for
tests, not catalogues.

## Scope

| Module | What it does |
|---|---|
| `model` | `PipeNetwork` (name, status, type, source record, reference surface name, reference alignment name), `Structure` (id, name, kind, E, N, rim, sump, inner size, wall, part, inflow, loss coefficient), `Pipe` (id, name, start and end structure, start and end invert, shape, wall, material, roughness, part, design flow, optional curve through a given point) |
| `parts` | `PartsCatalogue` of `Part`s from JSON or CSV; each part has a shape or structure kind and inner size, material, wall thickness, roughness, IFC entity and predefined type (free text), cost item code |
| `edit` | `add_run` from a sequence of positions, `split_pipe` (insert a structure), `move_structure`, `delete_structure`, `delete_structure_merge`, `connect`, `disconnect`, `swap_pipe_part`, `swap_structure_part`, `set_inverts_by_slope`, `set_drop_across_structure`, `set_inverts_by_cover`, `apply_rules` |
| `rules` | `RuleSet`: min and max slope, min and max cover, max pipe length, drop across structure, match invert, crown or centreline, min and max velocity, max depth ratio |
| `checks` | cover against a surface (`afrobim_corridor::Tin` or any closure), slope, length, structure drops, velocity, capacity and partial-flow depth at a design flow |
| `topology` | tree check, loops, branches, flow accumulation from structure inflows |
| `grade` | hydraulic and energy grade lines from a tailwater level |
| `interference` | 3D clearance, pipe to pipe and pipe to structure, within one network and between two |
| `output` | plan geometry, profile of a run (own path or projected to an alignment), section crossings at a station |
| `tables`, `quantities` | structure and pipe tables with CSV; pipe length by size and material, structure count by kind and part, trench excavation, bedding and pipe displacement by depth band |
| `landxml` | LandXML 1.2 `PipeNetworks` read and write |

Every type derives `Serialize` and `Deserialize`. Every check returns `Finding`s: a check code, a
severity (info, warning, error), the element ids (`ElementRef::Structure` or `ElementRef::Pipe`)
and a plain sentence.

Coordinates are metres, exactly as given, at true UTM magnitude (the tests work near northing
9 960 000 m). Distances come from coordinate differences, curve geometry is computed relative to
the first point, JSON uses `serde_json` with `float_roundtrip`, and LandXML is written with the
shortest text that reads back to the same `f64`.

## Methods and citations

**Manning's equation**, `V = R^(2/3) S^(1/2) / n`, `R = A / P`: `afrobim_drainage::design::manning_velocity_m_s`
(this crate depends on `afrobim-drainage` and reuses it), whose own citations are the MoWT
*Road Design Manual Vol. 2: Drainage Design* (January 2010; Example 4-1, Section 9.5.1 Step 12) and
DMRB CD 521 v1.2.0 Equation 5.25. Textbook: V. T. Chow, *Open-Channel Hydraulics*, McGraw-Hill, 1959.

**Colebrook-White**, `V = -2 sqrt(2 g D S) log10(ks/(3.7 D) + 2.51 nu/(D sqrt(2 g D S)))` with `D = 4R`:
C. F. Colebrook, "Turbulent flow in pipes, with particular reference to the transition region
between the smooth and rough pipe laws", *J. Inst. Civil Engineers* 11(4), 1939, pp. 133-156; also
Chadwick, Morfett and Borthwick, *Hydraulics in Civil and Environmental Engineering*. `D = 4R` is the
usual extension to part-full and non-circular sections. The friction slope for the grade line is the
Darcy-Weisbach relation `Sf = f V^2 / (2 g D)` with `f` from Colebrook's implicit equation.

**Continuity**, `Q = V A`, and flow accumulation at a node (flow leaving equals flow entering plus
local inflow): any hydraulics text.

**Partial flow in a circular pipe**: the geometry of a circular segment (Chow 1959, geometric elements
of a circular section), `theta = 2 acos(1 - 2y/D)`, `A = D^2 (theta - sin theta)/8`, `P = D theta/2`,
`T = D sin(theta/2)`; the depth for a discharge is found by bisection on `Q(y) = A V(R)` below the
depth of greatest free-surface discharge. Chow's chapters, tables and pages were not checked against
a held copy and are not given. The hand-worked tests are the check: half depth carries exactly half
of the full-bore discharge at the full-bore velocity (constant n), and the depth `y/D = 0.3` case is
worked in `tests/hydraulics_tests.rs`. Other shapes use the same bisection on their own section
functions; the box and rectangle results agree with `afrobim_drainage`'s open-channel solver.

**Froude number** `F = V / sqrt(g A / T)` and **critical depth in a circular pipe**:
`afrobim_drainage::design::{froude_number, circular_critical_depth_m}`.

**Energy and hydraulic grade lines** (`grade`): steady flow from a tailwater input. A pipe whose
downstream end is at or above its crown, or whose flow exceeds full bore, is taken as full under
pressure with the friction head loss `hf = Sf L` (energy equation between the ends, Chow 1959 or any
hydraulics text) and `HGL_up = H_down + hf`; otherwise the water surface is the uniform-flow depth
above the invert. `EGL = HGL + V^2/2g`.

**Structure losses.** Only `h = K V^2 / (2 g)` with `K` entered per structure, `V` the velocity in the
pipe leaving. **No method for choosing K is included**, so when no structure has a K the result is
friction only and `GradeReport::friction_only` is true and its `method` text says so.

**Closest points of two 3D segments** (interference): C. Ericson, *Real-Time Collision Detection*,
Morgan Kaufmann, 2005, section 5.1.9.

**Circle through three points** (curved pipes): elementary geometry (circumcircle), computed
relative to the start point.

**Trench volume**: average end area, `A = w d + z d^2`, band area
`w (b - a) + z [2 d (b - a) - (b^2 - a^2)]`, with the method stated in `quantities.rs` (cross-sections
in vertical planes perpendicular to the plan direction, so plan distance is the spacing).

## LandXML

Worked from the LandXML 1.2 schema <http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd>,
fetched 2026-11-16 (225 951 bytes). The elements and attributes used are `PipeNetworks`,
`PipeNetwork@name/@pipeNetType/@alignmentRef/@state`, `Structs/Struct@name/@elevRim/@elevSump/@desc`,
`Center`, `CircStruct`, `RectStruct`, `InletStruct`, `OutletStruct`, `Connection`, `Invert@elev/@flowDir/@refPipe`,
`Pipes/Pipe@name/@refStart/@refEnd/@length/@slope`, `CircPipe`, `RectPipe`, `ElliPipe`, `EggPipe`
(`@mannings`, `@material`, `@thickness`), `PipeFlow@flowIn`, `Feature/Property`, and the `Units` element.
Output written by this crate was checked with `xmllint --schema` against that file and validates.
XML is read with `roxmltree`, the library used by `afrobim-alignment`. The mapping, the properties
stored in `Feature name="afrobim"`, and the limits are listed in the `landxml` module documentation.

The `Center` of a `Pipe` is, by the schema, the point the curved pipe arc passes through; it is read
and written as the curve's through point.

## Limits

* **Uniform flow only.** No gradually varied flow, backwater, drawdown, hydraulic jump, air
  entrainment, sediment, or surcharge dynamics. A flow above the full-bore discharge is reported as
  exceeding capacity. In the grade line, when the tailwater is above the uniform-flow depth but below
  the crown, backwater is not computed and an information finding says so.
* **No structure loss method** (see above). No entrance, exit, bend or junction loss formulae. No
  drop-structure or benching hydraulics. No inlet capacity of catch basins or gullies.
* **No rainfall or runoff.** Flows are inputs (per pipe, or structure inflows summed down the network).
* **Trees only.** Loops are reported as errors and flows are not accumulated through them. A branch
  (several pipes leaving one structure) is allowed but the flow is not split: each pipe is given the
  whole accumulated flow, and a warning says so.
* **Egg sections** have no built-in profile (profiles differ between standards). Give the width at
  depth points (`profile`), otherwise their hydraulics are not computed. Elliptical sections are
  true ellipses, integrated numerically from 400 slices.
* **Interference** treats a pipe as a circle of its outer radius (half the larger inside dimension
  plus wall) swept along its centreline: conservative for box, egg and elliptical pipes. Structures are
  vertical cylinders (a rectangular structure uses the circle that contains it). Pipes that share a
  structure are not compared with each other, and a pipe is not compared with the structures it joins.
  Neighbouring chords of one curved pipe are not compared with each other.
* **Cover** is to the outside top of the barrel (crown plus wall thickness; a pipe with no wall
  thickness set has none). Sampling is at the ends and, if asked, at a spacing; the worst place per
  pipe is reported. `apply_rules` fixes cover only at the upstream end of each pipe.
* **`apply_rules`** is a simple downstream-to-upstream pass, not an optimiser. Outlet end inverts are
  anchors. It never moves structures, so a maximum length rule is only reported. Conflicts between
  rules (for example a minimum slope that leaves too little cover) are reported as findings, not hidden.
* **Moving a structure or swapping a part leaves the inverts alone**, so slopes and crowns change.
* **Profiles and sections:** projection to an alignment uses `Alignment::station_offset` (the
  perpendicular foot, internal stations); a point with no foot on the alignment is an error. The
  section line is straight and perpendicular to the alignment at the station.
* **Quantities:** pipe lengths are centre to centre of structures (so they include half of each
  structure). Trench volume has no structure pits, working space, support, rock, de-watering, swell or
  compaction, and the bedding is under the pipe only. Pipe displacement of non-round outlines is
  approximate (`pi t^2` corner term).
* **LandXML:** a network needs two structures and one pipe, and every structure needs a pipe (a
  schema rule); names must be unique. Channels, `StructFlow`, Hazen-Williams coefficients and `Units`
  nested in `Pipes` or `Structs` are not read. The `thickness` unit is taken as `linearUnit`, which
  the schema does not state.
* **IFC names.** The only names seen in the repository's `afrobim-ifc` sources are the entities
  `IFCPIPESEGMENT` (written there with predefined types `CULVERT` and `GUTTER`) and
  `IFCDISTRIBUTIONCHAMBERELEMENT`. No part is given a classification by the crate; the fields are
  free text for the part's author. This crate does not write IFC.

## Tests

`cargo test -p afrobim-pipes` (from `afrobim/`). The hydraulic tests carry their arithmetic in
comments (full-bore Manning and Colebrook-White, half-full circle, a partial-flow depth, a box
against the drainage rectangle solver, a pressure-flow grade line, a trench). The network tests use
coordinates at northing near 9 960 000 m.
