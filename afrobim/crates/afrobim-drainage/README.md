# afrobim-drainage

Road drainage as AfroBIM model elements: side drains (open channels along an alignment) and pipe
culverts, with 3D solids, sheet drawings, validation notes, quantities and hydraulic checks.

Everything this crate produces is **a calculation for an engineer to check. It is not a certified
design.**

## Two parts

**Modelling** (`SideDrainDefinition`, `PipeCulvertDefinition`, their kernel modules, quantities).
Every dimension, level and gradient is entered by the engineer. The crate draws those values and
measures them by plain geometry. It holds no rates or costs, and pipe material is free text.

**Design** (`design` module). Hydrology and hydraulics to a design basis the engineer chooses:

| `DesignBasis` | Documents | Module |
|---|---|---|
| `Mowt` | Uganda Ministry of Works and Transport, *Road Design Manual Volume 2: Drainage Design*, January 2010 | `design::mowt` |
| `Dmrb` | UK *Design Manual for Roads and Bridges*: CG 501 v2.2.0, CD 521 v1.2.0, CD 522 v0.1.0, CD 529 v1.1.0 | `design::dmrb` |

`design::hydraulics` holds what belongs to neither: section geometry, Manning's equation, normal
depth, Froude number, critical depth in a pipe. Every coefficient there is an argument.

**Which basis governs a project is the engineer's choice.** The crate does not pick one from a
country or a client. A calculation names its basis on its first line and cites only that basis'
documents. The modules default to `Mowt`; that default, and the choice of the MoWT manual as the
first basis, is an assumption made from the owner's context (a highway design engineer at MoWT),
not an instruction from any authority.

A further basis is a new sibling module, a new `DesignBasis` variant and one arm in each of that
enum's methods; the elements, the shared hydraulics and `check_channel` do not change. The
expected next one is the SANRAL *Drainage Manual* (South Africa). **No South African value is
held yet.**

## Sources

| Document | Edition obtained | From |
|---|---|---|
| MoWT *Road Design Manual Vol. 2: Drainage Design* ("DDM") | January 2010 (PDF produced May 2011, 202 pages) | uace.or.ug/wp-content/uploads/2023/03/Volume-2-Drainage-Design-Manual.pdf |
| DMRB CG 501 *Design of highway drainage systems* | Version 2.2.0, September 2025 | standardsforhighways.co.uk |
| DMRB CD 521 *Hydraulic design of road edge surface water channels and outlets* | Version 1.2.0, September 2025 | standardsforhighways.co.uk |
| DMRB CD 522 *Drainage of runoff from natural catchments* | Version 0.1.0, September 2025 | standardsforhighways.co.uk |
| DMRB CD 529 *Design of outfall and culvert details* | Version 1.1.0, September 2025 | standardsforhighways.co.uk |

The documents are not in the repository. Every number in the code that comes from one of them
carries its section, table, clause or equation number in a comment beside it.

## Citations: MoWT basis (`design::mowt`)

| Item | Where in DDM | In the crate |
|---|---|---|
| Design recurrence interval: side ditches 10/10/5/5 years, pipe culvert under 2 m 25/10/5/5, culvert 2 to 6 m 50/25/10/10, by the four columns | Table 3.2 and its Note 1 | `design_recurrence_years`, `culvert_structure_type` |
| Rational method `Q = C·i·A / 360` (m³/s, mm/h, ha) | Section 4.4.2, Eqn. 4.10 | `rational_peak_flow_m3_s`, `rational_method` |
| Limit of 80 ha; Modified Rational to 120 ha | Sections 4.4.2, 4.4.3 | warnings of `rational_method` |
| Weighted coefficient `Cw = Σ(Ai·Ci) / A` | Eqn. 4.6 | `weighted_runoff_coefficient` |
| Urban runoff coefficients (ranges) | Table 4.7(a) | `UrbanLandUse::c_range` |
| Rural runoff coefficient `C = Cs + Ck + Cv` and its note factors | Table 4.7(b) | `rural_runoff_coefficient` |
| Multipliers 1.0 / 1.1 / 1.2 / 1.25 for 2-10 / 25 / 50 / 100 years | Table 4.8 | `recurrence_multiplier` (never applied automatically) |
| Storage factor `Cs = 2tc / (2tc + td)`; `Q = Cs·C·i·A / 360` | Eqn. 4.7, Eqn. 4.11 | `storage_reduction_factor`, `RationalInput::storage_factor` |
| Sheet flow time `Tt = (6.943 / i^0.4)·(nL/√S)^0.6` min | Table 4.1 No. 8 | `sheet_flow_time_min` |
| Sheet flow roughness | Table 4-3 | `SheetFlowSurface::manning_n` |
| Velocity method `V = k·Sp^0.5`, `Tt = L / 60V` | Table 4.1 No. 7 | `concentrated_flow_velocity_m_s`, `travel_time_min` |
| Intercepts k | Table 4-2 | `LandCover::intercept_k` |
| Minimum time of concentration 5 min; 10 min under 0.4 ha | Section 4.2.1(e), Table 4.4 | `MINIMUM_TC_MIN`, `MINIMUM_TC_SMALL_AREA_MIN` |
| IDF form `i = a / (b + td)^c` | Eqn. 4.1 | `idf_intensity_mm_h` (constants are inputs) |
| Manning `Q = (1/n)·A·R^(2/3)·S^(1/2)` | Example 4-1, Section 9.5.1 Step 12, Appendix 10.4 Col. 8 | `hydraulics`, `check_channel` |
| Froude number `F = V / √(g·y)`, g = 9.81 | Appendix 10.4 Col. 11 | `froude_number` |
| Permissible velocity 0.6 / 1.1 / 1.5 / 2.0 / 3.0 / 3.0 / 3.0 m/s by channel material | Table 9.1 | `ChannelMaterial::permissible_velocity_m_s` |
| "Roughness coefficient" 45 / 50 / 40 / 25 / 25 / 40 / 60 | Table 9.1 | `ChannelMaterial::roughness_coefficient_as_printed` |
| Barrel losses `H = [1 + Ke + 19.63 n² L / R^1.33]·V²/2g`, g = 9.8 | Eqns. 9.3, 9.4, 9.6 | `barrel_head_loss_m` |
| Outlet-control headwater `HWoi = H + ho − So·L`, `ho = max(TW, (dc + D)/2)` | Eqns. 9.9, 9.11; Section 9.5.1 Step 8 | `outlet_control` |
| Validity of that method below 1.2 D and 0.75 D | Section 9.5.1 Step 8 g | warnings of `outlet_control` |
| Entrance loss coefficients, pipes | Appendix 9.6 | `PipeEntrance::ke` |
| Manning's n ranges, pipes | Appendix 9.7 | `PipeWall::manning_n_range` |
| `HW/D ≤ 1.5`; 1.2 recommended | Section 9.3 | `HEADWATER_RATIO_LIMIT`, `HEADWATER_RATIO_RECOMMENDED` |
| Freeboard below the shoulder break point: 250 mm major roads, 100 mm minor | Section 9.6 | `CULVERT_FREEBOARD_*`, `OutletControl::freeboard_m` |
| Culvert velocity "ideally between 1 and 3.5 m/s" | Section 9.7 | warning of `outlet_control` |
| Skew not over 45° | Section 9.8 | culvert note |
| Gradient: under 1 % avoid, under 0.5 % not to be used; over 10 % anchors, over 20 % special anchors | Sections 9.9.1, 9.9.2 | culvert notes |
| Minimum pipe culvert diameter 600 mm | Section 9.10 | culvert note |

## Citations: UK DMRB basis (`design::dmrb`)

| Item | Where | In the crate |
|---|---|---|
| 1 in 1 year no surcharge; 1 in 5 years no flooding | CG 501 v2.2.0 clause 5.1, Table 5.2 | `RoadDrainageCriterion` |
| Flooding within the highway boundary to 1 in 100 years plus climate change | CG 501 v2.2.0 clause 5.3 | `RoadDrainageCriterion` |
| Natural catchment runoff: 1 in 100 years plus climate change | CD 522 v0.1.0 clause 5.1 | `NATURAL_CATCHMENT_RETURN_PERIOD_YEARS` |
| Climate uplift 20 %, sensitivity 40 % | CG 501 v2.2.0 clauses 4.2, 4.3 | `CLIMATE_UPLIFT_*` (UK policy; applied to nothing) |
| Manning's n: concrete 0.013 / 0.016, asphalt 0.017 / 0.021 (average / poor) | CD 521 v1.2.0 Table 5.18.1 | `ChannelSurface::manning_n` |
| Grassed channel `n = 0.05 / (1 − mg·H / (R^(5/3)·S^(1/2)))`, mg and H by grass type | CD 521 v1.2.0 Equation 5.19 | `grassed_manning_n`, `GrassType` |
| Manning resistance equation | CD 521 v1.2.0 Equation 5.25 | `hydraulics`, `check_channel` |
| Design flow of trapezoidal, triangular, symmetrical triangular, rectangular channels | CD 521 v1.2.0 Equations 5.25.1 to 5.25.4 | `design_flow_m3_s`, `symmetrical_triangular_flow_m3_s` |
| Hydraulic-radius factor r | CD 521 v1.2.0 Equations 5.14 to 5.16 | `hydraulic_radius_factor` |
| Shape parameter `m = B·y/A − 1` | CD 521 v1.2.0 Equation 5.10 | `shape_parameter_m` |
| Equivalent slope of a varying gradient | CD 521 v1.2.0 Equation 5.17, clauses 5.17.1, 5.17.2 | `equivalent_slope` |
| Minimum culvert diameter 450 mm | CD 529 v1.1.0 clause 3.5.1 | culvert note |
| Circular culvert longer than 12 m: at least 1.2 m diameter | CD 529 v1.1.0 clause 3.4 | culvert note |
| Over 900 mm diameter: a structure | CD 529 v1.1.0 clause 1.1 NOTE 2 | culvert note |
| Culvert invert 75 mm below the bed of a highway drain | CD 529 v1.1.0 clause 3.10 | `CULVERT_INVERT_BELOW_DRAIN_BED_M` (not checked: the drain bed level at a culvert is not modelled) |

Under the DMRB basis there is **no rainfall or runoff method and no culvert hydraulics**:

- UK rainfall methods must not be carried to other countries. CD 521 Equations 5.20 to 5.22 use
  the 2minM5 rainfall depth from a UK map (its Figure 5.3); CD 522 clause 5.2 requires IH 124 or
  the Flood Estimation Handbook. None is implemented. The design flow is an input.
- CD 529 clause 3.3 refers the hydraulic design of culverts wholly to CIRIA C786, which is not a
  free document and is not held.
- CD 521 is written for shallow channels at the carriageway edge. Using its roughness values for
  a deeper side drain is this crate's reading, not CD 521's. The DMRB document for ditches is
  CD 532, which was not in the brief and is not held; the four documents state no permissible
  velocity and no freeboard for a ditch.

## Worked examples reproduced in the tests

| Source | What | Result |
|---|---|---|
| DDM Example 4-1 | Time of concentration, four segments | 14.6 + 4.8 + 3.7 + 1.4 = 24.5 min, as printed |
| DDM Example 4.2 | Weighted C | 0.235 as printed; 0.3155 for the proposed case (printed 0.315) |
| DDM Example 4.3 | Rational method, 43.7 ha | C = 0.32, Q = 3.3 m³/s, as printed |
| DDM Appendix 4.1 | IDF equation of the demonstration station | 18.78 mm/h at 2 h, within 0.02 |
| DDM Section 9.5.1 Step 8 | Barrel losses by Eqn. 9.6 | 0.798 m (the manual reads 0.85 m from a chart) |
| DDM Section 9.5.1 Step 12 | Normal depth in a 2.13 m rectangular barrel | 0.558 m (the manual stops its trial at 0.55 m) |
| CD 521 Appendix B, B1 and B3 | r, A, B, m of a triangular and a trapezoidal channel | 0.981, 0.072, 1.200, 1.00; 0.984, 0.1575, 1.800, 0.71 |
| CD 521 Appendix B, B5 | Grassed channel n and capacity | n = 0.062, Q = 0.061 m³/s, as printed |
| CD 521 Appendix B, B7, B8 | Channel-full flow, triangular and trapezoidal | R = 0.0588 m, Q = 0.0592 m³/s; R = 0.0861 m, Q = 0.106 m³/s, as printed |

## Ambiguities and slips found in the documents

These are flagged, not resolved silently.

**MoWT DDM**

- **Table 3.2 column headings** read "PIa, PIb | PIII Gravel A | PIII, Gravel B | Gravel C".
  "PIII" appears twice and "PII", which the legend defines, nowhere. `DesignStandard` names the
  columns as printed; the engineer picks the column. The rows are "< 2 m" and "2 m < span < 6 m",
  so exactly 2 m is in neither; the crate puts it in the longer-interval row.
- **Table 9.1 "roughness coefficient"** (25 to 60) has no unit and names no formula. Values of
  that size are Strickler coefficients (K = 1/n), not Manning's n. That is this crate's reading,
  so Manning's n is entered and `manning_n_if_strickler` is only an offered conversion.
- **Table 4.8 multipliers**: the manual does not say what they multiply, sets no cap on C, and
  its own 25-year Example 4.3 does not apply one. They are an explicit input, never automatic.
- **Eqns. 4.2 and 4.3** (`V = 16.13·s^0.5` unpaved, `20.33·s^0.5` paved, "m/s") disagree with
  Table 4-2 by a factor of 3.28: the table's 0.491 and 0.619 with slope in per cent are
  4.91·s^0.5 and 6.19·s^0.5 in m/s. The equations look like ft/s constants. Not implemented.
- **Tables 4.5 and 4.6** (IDF constants) are both headed "Group I", give different constants for
  Entebbe, Kampala and Gulu, and Table 4.6 has Nairobi's 5-year `a` (92.26) above its 10-year
  (70.79). They stop at 10 years. Not held; intensity is an input.
- **Example 4-1** takes n = 0.011 for a concrete pipe "from Table 4.3", where 0.011 is smooth
  asphalt. **Example 4.2** writes 2.22 ha where its table has 2.20. **Example 4.3** says 30 m of
  short grass in the text and uses 90 m in the table and the sum.
- **Culvert example (Section 9.5.1)**: Step 8 d prints "(dc + D/2)" for (dc + D)/2; H is read
  from a chart as 0.85 m where Eqn. 9.6 gives 0.80 m; Step 12 accepts dn = 0.55 m where the trial
  has not converged (0.558 m), giving 9.68 m/s, and Step 15 then quotes 9.5 m/s.
- **Gravity** is 9.8 under Eqn. 9.4, 9.81 in Appendix 10.4, and 19.63 = 2 × 9.815 in Eqn. 9.5b,
  which also rounds the exponent 4/3 to 1.33. Each is used as printed where it is printed.
- **Froude number** in Appendix 10.4 uses the water depth of a river section; the crate uses the
  hydraulic depth A/T, which is the same for a rectangle.
- **No side-drain shapes, no freeboard for open drains** are defined anywhere in the manual.
  Section 9.6's freeboard is for the headwater at a drainage structure.
- **Critical depth** is a chart (Appendix 9.10); the crate solves `Q²T / gA³ = 1` instead.

**DMRB**

- CD 521 worked example B5 cites "clause 5.20" for the grass coefficients, which are under
  Equation 5.19; B6 cites "Table 5.44" for the minimum pipe velocities, which are Table 5.74.
- The documents do not class a "side drain": a drain taking road runoff falls under CG 501's
  1 and 5 year criteria, one taking runoff from land outside the highway under CD 522's 100
  years. Which applies is left to the engineer.

## How each quantity is computed

**Side drain.** The section is the trapezoid family: bed width `b`, depth `d`, side slopes `zl`,
`zr` (horizontal to one vertical); a V has `b = 0`, a rectangle `zl = zr = 0`. Lining thickness `t`
is measured square to the faces.

| Quantity | Formula |
|---|---|
| Length along the alignment | end station − start station |
| Length of the drain | `∫ (1 + κ·o) ds`: κ the alignment's curvature (+ turning left), `o` the offset (+ right). On a circular curve `L·(R ± o)/R`; exact for clothoids too |
| Finished area | `d·(b + (zl + zr)·d / 2)` |
| Excavated area | `(d + t)·(be + (zl + zr)·(d + t) / 2)`, with `be = b + t·(√(1+zl²) − zl + √(1+zr²) − zr)`: faces `t` outside the finished faces, bed `t` below the finished bed |
| Excavation | excavated area × drain length |
| Lining area | `(b + d·(√(1+zl²) + √(1+zr²)))` × drain length |
| Lining volume | (excavated area − finished area) × drain length |

Ground is taken as level with both top edges. Nothing is added for working space, overbreak,
topsoil, bulking or waste. On a curve the volume takes the section's centroid to travel the
drain's centreline.

**Pipe culvert.** Length per barrel as entered (along the pipe); total = barrels × length. Fall =
inlet invert − outlet invert; plan length = `√(L² − fall²)`; gradient = fall / plan length.

`drains_to_csv` and `culverts_to_csv` write the schedules; `measure_document` measures a document
and lists what it could not measure.

## Geometry

- **Drain**: the sheet drawing is the cross-section looking up-station. The 3D solid is a closed
  trough swept along the route (the lining; for an unlined drain a 50 mm display skin that is in
  no quantity). Frame: origin at the drain centreline at its start station, not rotated, z = 0 at
  the top edges when the alignment has no profile. **Where the alignment has a vertical profile**
  the top edges lie at the profile level at each station of the route (held at the end levels
  beyond the profile; the road's cross-fall and the ground are not followed), so the solid rises
  and falls with the road. With no profile the solid is level at z = 0 as before. The entered
  gradient is used for hydraulics only.
- **Culvert**: the sheet drawing is a section along one barrel. The 3D solids are hollow pipes
  falling from inlet to outlet. Frame: origin at the inlet end of the group's centre line at inlet
  invert level, +X along the flow in plan. With no profile and a road level entered, the site
  placement's `z` puts the inlet invert below a road at level zero; without a road level it is
  zero. With a profile the site placement's `z` is the inlet invert's own datum level.

## Calls the app needs

```rust
use afrobim_drainage::{design::DesignBasis, *};

// Once, when building the module registry. The basis is the engineer's setting.
modules.register(Box::new(SideDrainModule::with_basis(basis)));
modules.register(Box::new(PipeCulvertModule::with_basis(basis)));

// Ribbon: add a drain or a culvert on an alignment already in the document (by name).
let drain = add_side_drain(&mut doc, "SD-01", &SideDrainDefinition::placeholder("MC10", 0.0, 100.0, 5.0))?;
let culvert = add_pipe_culvert(&mut doc, "C-01", &PipeCulvertDefinition::placeholder("MC10", 50.0))?;

// After the property editor writes a definition back to element.data, or an alignment changes:
let problems: Vec<String> = sync_drainage(&mut doc); // re-routes, re-places, resizes

// 3D view: Generated.solids placed by element.site (x, y, z mm; rotation_deg), as for supports.
// Layers: DRAINS_LAYER, CULVERTS_LAYER. Categories: DRAIN_CATEGORY, CULVERT_CATEGORY.

// Schedules.
let q = measure_document(&doc);
let csv = drains_to_csv(&q.drains) + &culverts_to_csv(&q.culverts);

// Hydraulic checks: each returns a struct with .calculation() -> Vec<String>.
let check = SideDrainDefinition::from_value(&element.data)?.check_capacity(basis, flow_m3_s, None)?;
let flow = design::mowt::rational_method(&input)?;                 // MoWT only
let tc = design::mowt::time_of_concentration(&segments, i_mm_h, design::mowt::MINIMUM_TC_MIN)?;
let hw = culvert_def.outlet_control(flow_m3_s, n, ke, tailwater_m)?; // MoWT only
```

## Limits

- Uniform flow only: no backwater from culverts or outfalls, no drawdown, no bends.
- Rainfall intensity is always an input, under both bases.
- Manning's n of a drain is always an input.
- The gradient of a drain is entered; which way it falls is not modelled.
- A culvert's outlet-control headwater is **half a culvert check**. Inlet control exists in DDM
  only as nomographs and is not implemented; the higher of the two governs. Every result says so.
- Layout notes cover only what the chosen basis' documents state. No note does not mean no rule.

## Not in this cut

Box and slab culverts and bridges; inlet-control nomographs, flow over the road, performance
curves and the approximate capacity tables (DDM Appendices 9.3 to 9.5, 9.19 to 9.21); headwalls,
wingwalls, aprons and bedding; energy dissipators and scour; subsurface drainage; kerbs, gutters,
inlets, storm sewers and gullies; DMRB outlets, gratings, weirs and combined channel and pipe
systems; catchment delineation; the SCS, unit hydrograph and TRRL flood models; DDM Table 4.1
formulae Nos. 1 to 6; detention; costs.
