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
| `Sanral` | South African National Roads Agency, *Drainage Manual*, 6th edition, September 2013, with the *Drainage Manual Application Guide* (September 2013) for worked examples | `design::sanral` |

Culvert **inlet control** under the MoWT basis is `design::inlet` (FHWA HDS-5 equations, because DDM
prints inlet control only as nomographs) and the complete culvert check is `design::culvert_check`.
Under the SANRAL basis inlet control is the manual's own Table 7.2 (`design::sanral`).

`design::hydraulics` holds what belongs to neither: section geometry, Manning's equation, normal
depth, Froude number, critical depth in a pipe. Every coefficient there is an argument.

**Which basis governs a project is the engineer's choice.** The crate does not pick one from a
country or a client. A calculation names its basis on its first line and cites only that basis'
documents. The modules default to `Mowt`; that default, and the choice of the MoWT manual as the
first basis, is an assumption made from the owner's context (a highway design engineer at MoWT),
not an instruction from any authority.

A further basis is a new sibling module, a new `DesignBasis` variant and one arm in each of that
enum's methods; the elements, the shared hydraulics and `check_channel` do not change. The
SANRAL *Drainage Manual* (South Africa) is the third, `DesignBasis::Sanral`. Its values are
in `design::sanral` only and are never mixed into a MoWT or DMRB calculation: a calculation names
its basis on its first line and cites only that basis' documents. `DesignBasis::ALL` still lists the
original two (so existing code that maps over it compiles); `DesignBasis::EVERY` lists all three.

## Sources

| Document | Edition obtained | From |
|---|---|---|
| MoWT *Road Design Manual Vol. 2: Drainage Design* ("DDM") | January 2010 (PDF produced May 2011, 202 pages) | uace.or.ug/wp-content/uploads/2023/03/Volume-2-Drainage-Design-Manual.pdf |
| DMRB CG 501 *Design of highway drainage systems* | Version 2.2.0, September 2025 | standardsforhighways.co.uk |
| DMRB CD 521 *Hydraulic design of road edge surface water channels and outlets* | Version 1.2.0, September 2025 | standardsforhighways.co.uk |
| DMRB CD 522 *Drainage of runoff from natural catchments* | Version 0.1.0, September 2025 | standardsforhighways.co.uk |
| DMRB CD 529 *Design of outfall and culvert details* | Version 1.1.0, September 2025 | standardsforhighways.co.uk |
| FHWA *Hydraulic Design of Highway Culverts*, HDS-5 ("HDS-5") | Third edition, April 2012, FHWA-HIF-12-026 (323 pages), a US federal publication | https://www.fhwa.dot.gov/engineering/hydraulics/pubs/12026/hif12026.pdf, fetched 2026-11-15 |
| SANRAL *Drainage Manual* ("SDM") | Sixth edition, September 2013, ISBN 978-0-620-55428-2, 468 PDF pages (PDF dated 31 May 2013, modified 19 September 2013) | https://www.nra.co.za/uploads/27/Drainage-Manual-6th-Edition-Sept-2013.pdf, fetched 2026-11-15 |
| SANRAL *Drainage Manual Application Guide* ("SAG") | September 2013, ISBN 978-0-620-55429-9, 296 PDF pages | https://www.nra.co.za/uploads/27/Drainage-Manual-6th-Edition-Application-Guide-Sept-2013.pdf, fetched 2026-11-15 |
| SANRAL *Drainage Manual* Addendum 1 (corrections to the January 2013 web version, 29 May 2013) | 2 pages | https://www.nra.co.za/uploads/27/Drainage-Manual-6th-Edition-Addenda-1-changes-to-Jan-2013-web-version.pdf, fetched 2026-11-15 |

The MoWT manual was fetched again on 2026-11-15 from the address in the table (7 423 950 bytes,
202 pages) and re-read for Section 9.4.1 and its example. The three SANRAL files are listed on
SANRAL's download page https://www.nra.co.za/downloads?region_id=national (www.sanral.co.za is
only a landing page that points to nra.co.za). Downloads are kept outside the repository under
`/home/user/sa-sources/r5/`; no manual is committed. The September 2013 SANRAL print includes at
least the Addendum 1 items on pages 3-27 (added paragraph) and 7-1 (deleted paragraph); the
Addendum 1 texts for pages 8-1 and 8-7 were not found in it by search, so those two are not relied on.

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

## Citations: culvert inlet control and the complete check (MoWT basis)

DDM Section 9.4.1 (p. 88) and Appendices 9.2 to 9.5 give inlet control as nomographs only, "adapted
from publications by the U.S. Bureau of Public Roads, and from AASHTO". It prints no inlet control
equation and no table of coefficients. HDS-5 Appendix A says its equations are "used to develop the
inlet control nomographs" from the same Bureau of Public Roads research, so `design::inlet` takes
the equations and every coefficient from HDS-5 and the check uses them beside DDM's own outlet
control and limits. Each constant says which document it came from.

| Item | Where | In the crate |
|---|---|---|
| Unsubmerged Form 1 `HW/D = Hc/D + K(Ku Q/AD^0.5)^M + Ks S`; Form 2 `K(Ku Q/AD^0.5)^M`; submerged `c(Ku Q/AD^0.5)^2 + Y + Ks S` | HDS-5 Eqs. A.1, A.2, A.3, printed pp. A.1, A.2 | `inlet_control` |
| SI: `Ku = 1.811`; unsubmerged to `Q/AD^0.5 = 1.93`, submerged from 2.21 | HDS-5 A.2.1, A.2.2, pp. A.1, A.2 | `KU_SI`, `UNSUBMERGED_LIMIT_SI`, `SUBMERGED_LIMIT_SI` |
| Constants K, M, c, Y for circular concrete (Chart 1), circular corrugated metal (Chart 2), beveled ring (Chart 3), concrete box (Charts 8, 9, 10) | HDS-5 Table A.1, p. A.8 (PDF p. 197) | `InletEdge` |
| `Ks = -0.5`, mitred `+0.7`; adverse slope not to be used beyond D/2 | HDS-5 A.2.1, Section 3.3.4 p. 3.25 | `KS_STANDARD`, `KS_MITRED`, refusal |
| `Hc = dc + A/(2T)`; `dc` cannot exceed D; g = 9.8 | HDS-5 Eq. A.4, Section 3.3.3, Appendix B | private `circular_critical`, `box_critical` |
| Transition between the two zones | HDS-5 A.2 and Chapter 3 (curve drawn by hand, tangent to both) | straight line between the two equation values, labelled as this crate's reading |
| Equations within 10 percent of the nomographs | HDS-5 A.4, p. A.7 | warning on every result |
| Headwater ratio refused above the top graduation of the SI nomograph scale (6; 3.6 and 3.0; 8, 9, 10; 4; 10, 9, 8) | HDS-5 Charts 1A, 2A, 3A, 8A, 9A, 10A, read from the printed charts | `InletEdge::chart_limit_hw_over_d` |
| Governing headwater is the higher of inlet and outlet control | DDM 9.5.1 Step 9; HDS-5 Design Guideline 1 Step 5 | `culvert_check` |
| Allowable headwater | the engineer's input | `CulvertCheckInput::allowable_headwater_m` |
| `HW/D <= 1.5`, 1.2 recommended; freeboard 250 / 100 mm below the shoulder break point | DDM 9.3, 9.6 | applied to the governing headwater |

Under the MoWT basis the outlet control lines are the existing ones, unchanged; the new lines follow
them (`CulvertCheck::additional_lines`).

## Citations: SANRAL basis (`design::sanral`)

Page numbers are the manual's own printed numbers (chapter-page).

| Item | Where | In the crate |
|---|---|---|
| Rational method `Q = C I A / 3.6` (A in km²), limit about 15 km² | SDM Eq. 3.8, 3.5.1.1, pp. 3-17, 3-18 | `rational_method` |
| Run-off components `Cs`, `Cp`, `Cv` by mean annual rainfall (< 600, 600 to 900, > 900 mm); urban `C2` ranges | SDM Table 3.7, p. 3-18 (same values in SAG Table 3.3 p. 3-19 and form legend Table 3C.1) | `RuralSlope::cs`, `Permeability::cp`, `Vegetation::cv`, `UrbanUse::range` |
| `C1 = Cs + Cp + Cv` by shares of the catchment | SDM 3.5.1.2.2 p. 3-18; SAG Eq. 3.10 p. 3-19 | `runoff_coefficient` |
| Dolomite factors 0.50 / 0.35 / 0.20 / 0.10; `C1D = C1(1 - D) + C1 D Sum(Dfactor x Cs share)` | SDM p. 3-19; formula as printed in SAG p. 3-20 | `RuralSlope::dolomite_factor` |
| Return period factor `Ft` for 2, 5, 10, 20, 50, 100 years (two catchment kinds); maximum flood `Ft = 1`, `C2 = 1` | SDM Table 3.8 and p. 3-19 | `return_period_factor` |
| `CT = alpha C1T + beta C2 + gamma C3` (`C3` lakes, entered) | SAG Eq. 3.12 p. 3-20 | `RunoffInput` |
| Kerby `Tc = 0.604 (rL/S^0.5)^0.467`, roughness `r` | SDM Eq. 3.11 p. 3-27, Table 3.10 p. 3-28 | `kerby_tc_h`, `KerbySurface` |
| SCS `Tc = (0.87 L^2 / (1000 Sav))^0.385`; 10-85 slope | SDM Eq. 3.12 p. 3-28, Eq. 3.13 p. 3-29 | `scs_tc_h`, `slope_10_85` |
| Tc as the sum of overland, channel and urban reaches; mean velocity 0.1 to 4 m/s | SDM 3.5.1.4 p. 3-29 | `time_of_concentration` |
| Area reduction `ARF = (90000 - 12800 ln A + 9830 ln(60 Tc))^0.4`, below 100 % | SDM Eq. 3.10 p. 3-25 | `area_reduction_factor_percent` |
| Point intensity `P/Tc`, Tc not below 15 minutes; `I = Pi x ARF/100` | SDM 3.5.1.3 p. 3-20; SAG Eqs. 3.3, 3.4 pp. 3-10, 3-12 | `rational_method` |
| Design return period by road class and the 20-year flood | SDM Figure 8.2 p. 8-7 | `design_return_period_years` |
| `H1/D <= 1.2` at `QT`; at `Q2T` the smaller of `2D` and the shoulder break point (classes 1 to 3) | SDM 8.3.2 p. 8-7; 7.2.2.4 p. 7-4 | `culvert_check_sanral` |
| Free-flow freeboard `F = 0.78 log10(QT) - 1.26`; 0.3 m with severe debris | SDM Figure 8.3 p. 8-8, 8.4.1 p. 8-9 | `free_flow_freeboard_m` |
| Channel freeboard `0.15E` / `0.20E` (Fr < 1), `0.25y` / `0.30y` (Fr > 1); extra at bends | SDM Tables 5.3, 5.4 p. 5-22 | `channel_freeboard_m`, `bend_additional_freeboard_m` |
| Grass cover velocities by grass, rainfall and clay content | SDM Table 5.6 p. 5-24 | `grass_permissible_velocity_m_s` |
| Maximum side slopes of unlined channels | SDM Table 5.5 p. 5-23 | `steepest_side_slope_h_per_v` |
| Median drain slopes 0.5 % unpaved, 0.25 % paved; 2.5 m/s over a jointed concrete lining | SDM 5.4.3 p. 5-23, 5.4.6.1 p. 5-25 | constants |
| Manning's equation | SDM Eq. 4.27 p. 4-14 | shared `check_channel` |
| Round and rectangular inlet control relations | SDM Table 7.2 p. 7-8 (SAG Table 7.1 p. 7-71) | `round_inlet_flow_m3_s`, `rect_inlet_flow_m3_s` and inverses |
| Outlet control `H1 = H2 + (Kin + Kout) v^2/2g + hf - S0 L`, Manning friction; Darcy friction as the example | SDM Eq. 7.4 p. 7-7; SAG Eqs. 7.4 to 7.7 pp. 7-75, 7-76 | `outlet_control_h1_m` |
| Greater of inlet and outlet `H1` governs | SDM 7.4.1 p. 7-10 | `culvert_check_sanral` |
| Minimum culvert 600 mm (to 30 m), 900 mm (over 30 m); slope not below about 1 %, velocity not below 1 m/s | SDM 7.2.2.5, 7.2.2.2 p. 7-4 | `culvert_layout_limits` |
| Concrete side drain ks 0.002 m, n 0.016; concrete culvert n 0.016 | SDM Photograph 4.5 p. 4-17, 7.3.2.1 p. 7-7 | single printed values only |

### What stays an input under the SANRAL basis, and why

- **Rainfall.** The point rainfall depth in mm for the time of concentration and return period.
  The manual's routes are a depth-duration-frequency diagram (Figure 3.8), the Hershfield equation
  with TR102 data and a thunder-days map (Eq. 3.9, Figure 3.9) and the Design Rainfall Estimation
  software; all are site data and none is embedded.
- **Roughness (ks, Manning n).** Figures 4.8 to 4.10 match a material to a scale by eye; a value
  cannot be read reliably, so roughness is entered. Only the single values the text prints are held.
- **Soil permissible velocity** (Figure 5.15 is a graph).
- **Kin, Kout** (named in Eq. 7.4, no table; the example uses 0.5 and 1.0), the **downstream energy
  level H2**, the shoulder break point, `C3`, the area reduction factor when read from a figure.
- **The slope unit of Table 7.2** (see the slips below): the caller must choose `SlopeUnit`.
- **Culvert cover.** SDM 9.3.4 (p. 9-18) gives 0.9 m minimum cover for storm drains. The chapter on
  culverts states none, so no cover limit is applied to culverts.

### Not done under the SANRAL basis

The Standard Design Flood method, unit hydrograph, SCS-SA, empirical (Midgley and Pitman, Kovacs
regions) and statistical flood methods; the Hershfield equation and the DDF diagram; roughness
Figures 4.8 to 4.10; Figure 5.15; Table 3.13 (SDF only); kerb, grid and side outlets; low-level
crossings; bridges, major culverts and scour; stormwater system design; flood routing and the
assessment of existing structures (Chapter 10); outlet velocity and erosion protection (Figures
7.6 to 7.12); tapered inlets.

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
| HDS-5 Section A.3.1 (pp. A.3, A.4) | Arithmetic of Eqs. A.1 and A.3 with the printed Hc/D | as printed, except one row (slip below) |
| HDS-5 Design Guideline 1.3 (DG1.1 to DG1.4, PDF 272 to 275) | 54 in concrete pipe, Q = 5.663 m³/s, S = 0.01, groove end | HW/D 1.76 against the printed 1.77 (HY-8 7.9 ft; equation 7.9 ft) |
| HDS-5 Design Guideline 1.4 (DG1.8 to DG1.10, PDF 279 to 281) | 6 x 5 ft and 5 x 5 ft box, Q = 8.5 m³/s, S = 0.02, with and without 45 degree bevels | 1.59, 1.94, 1.714 against 1.57, 1.91, 1.71; bevelled 5 x 5 ft 2.62 m against the printed 33.10 m less 30.48 m |
| HDS-5 Chart 1A (PDF 218) printed SI example | D = 1050 mm, Q = 3.40 m³/s, three scales | 2.58, 2.14, 2.21 against 2.5, 2.1, 2.2 (within the 10 percent HDS-5 allows) |
| HDS-5 Chart 8A (PDF 232) printed SI example | 1500 x 600 mm box, Q = 2.12 m³/s | within 10 percent on all three scales |
| DDM Section 9.5.1 Step 7 (p. 98) | 2135 x 1830 mm box, Q = 11.33 m³/s: a **transition zone** case (x = 2.14) | HW/D 1.260 against the printed 1.27 (HWi 2.31 m against 2.32 m) |
| SAG Example 3.1 (pp. 3-5 to 3-21) | Tc by Eq. 3.12, Sav by Eq. 3.13, C1 0.3665, C1D 0.337, C1T 0.2258 and 0.2797, C2 0.48, CT 0.3783 and 0.3999, ARF 96 %, Q for Alternatives 1, 2, 3 | Tc 1.338 h; Q20 164, 184, 128 and Q50 224, 243, 166 m³/s, as printed |
| SAG Example 7.1 (pp. 7-69 to 7-76) | Class 3, Q20 = 85 m³/s gives 15 years; orifice flow 8.55 m³/s; box 9.893 m³/s; outlet control H1 2.908 m (six pipes) and 2.704 m (seven pipes) | as printed |

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

**HDS-5** (checked against the page images)

- Section A.3.1 (p. A.4): the row `Qc/AD^0.5 = 2.54` prints a result of 0.55 where its own columns
  (0.138 + 0.84 - 0.01) give 0.97. The example takes "Table A.1, Chart 34, Scale 3" and Y = 0.53;
  Chart 34 is in Table A.2 (discontinued charts), which gives Y = 0.57.
- Table A.1 is headed "Charts in Appendix G"; the charts are in Appendix C.
- Equation A.2 (Form 2) has no `Ks S` term, while the legend gives `Ks` for all forms and Section 3.3.4
  says the nomograph HW/D contains `-0.5 S`. The crate follows the print: Form 2 has no slope term.
- The printed HW in metres of the examples on Charts 1A, 2A and 8A is 0.02 to 0.055 m above
  `(HW/D) x D` in every row (Chart 1A: 2.5 x 1.05 = 2.625, printed 2.68).
- Design Guideline 1 does not say which nomograph scale it used (headwall or projecting groove end;
  which box wingwall flare). The tests use the scale that reproduces the printed value and require
  the other candidate scale to lie within 10 percent where it applies.

**MoWT DDM, inlet control example**

- Section 9.5.1 Step 7 names the entrance "Wingwalls, 45 degree bevel, rounded" and reads HW/D from
  Appendix 9.5 without naming the scale; the test uses HDS-5 Chart 10 scale 2 (90 degree headwall
  with 45 degree bevels).

**SANRAL**

- **Table 7.2 gives no unit for S0.** `(S0/0,4)^0,05` means 40 % if S0 is in m/m and 0.4 % if in per
  cent; the factor differs by about 20 % at a 0.15 % slope. The crate refuses to choose: `SlopeUnit`
  is a required argument. SAG Example 7.1 uses only the orifice relation, which has no slope, so it
  does not settle this. Only the round culvert weir relations are affected.
- **Table 7.2 is not continuous at H1/D = 1.2.** Round: the weir relation gives
  `Q/(D²√(gD)) = 0.44 x 1.2^1.5 = 0.578` (at S0/0.4 = 1), the orifice relation 0.557. Rectangular
  square inlet: 2.017 against 2.059 in units of `B D^1.5`. In the overlap the inverse functions return
  the larger `H1`, and never less than `1.2 D` for a rectangular culvert.
- **Figure 7.4 against Table 7.2**: SAG Example 7.1 reads H1 = 2.4 m for 8.9 m³/s through a 1.8 m
  square culvert and then checks it, getting 9.893 m³/s at 2.4 m. The relation puts 8.9 m³/s at about
  2.15 m (the crate holds it at 1.2 D = 2.16 m). The chart reading is the more conservative.
- **H1 is a depth in one place and an energy in another.** SDM 8.3.2 (p. 8-7) says "H1 is the water
  depth at the inlet"; 7.1 (p. 7-1), 7.3.2 (p. 7-6) and 10.1 (p. 10-2) use the energy level above the
  inlet invert, and Chapter 10 says its definition differs from Chapter 7. The crate treats H1 as the
  energy level of Chapter 7 throughout.
- **Alternative 3 of SAG Example 3.1** applies no area reduction (Steps 7c, 8c) while Alternatives 1
  and 2 do; the crate reproduces it with `AreaReduction::NotApplied`.
- **Figure 8.2** is read from the dots printed on the figure (20 and 150 m³/s; 50 and 100, 20 and 50,
  10 and 20 years). Only the Class 3 line at 85 m³/s is confirmed by a worked example (SAG Example 7.1);
  the other classes are the figure's dots read at graph accuracy.
- SDM Eq. 7.4 prints `v1` with `Kin` and `v2` with `Kout`; the example (and the crate) use the barrel
  velocity for both.
- SDM gives `Kin` and `Kout` no table and Figures 4.8 to 4.10 no readable values.

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

### Complete culvert check (MoWT basis): inlet control, greater-of, limits

```rust
use afrobim_drainage::design::{culvert_check::CulvertCheck, inlet::InletEdge};

// Flow, Manning n, entrance Ke (DDM Appendix 9.6), tailwater depth, inlet type (HDS-5 Table A.1 row),
// allowable headwater above the inlet invert (m), and optionally (shoulder break level, major road).
let check: CulvertCheck = culvert_def.culvert_check(
    3.0, 0.012, 0.5, 1.0, InletEdge::ConcreteGrooveEndHeadwall, 2.0, Some((1152.0, true)),
)?;
check.governing;                 // Control::Inlet or Control::Outlet
check.headwater_m;               // the greater of the two, above the inlet invert
check.passes();                  // within the allowable, HW/D <= 1.5, freeboard met
// The existing lines stay exactly as they were; append the new ones after them:
let mut lines = culvert_def.outlet_control(3.0, 0.012, 0.5, 1.0)?.calculation();
lines.extend(check.additional_lines());   // same as check.calculation()
// Inlet control alone, any circular or box barrel:
use afrobim_drainage::design::inlet::{inlet_control, InletGeometry, InletInput};
let ic = inlet_control(&InletInput {
    geometry: InletGeometry::Box { width_m: 2.135, height_m: 1.83 },
    edge: InletEdge::BoxHeadwallBevel45, flow_m3_s: 11.33, slope_m_per_m: 0.05, length_m: 90.0,
})?;                              // ic.regime, ic.hw_over_d, ic.headwater_m, ic.calculation()
```

### SANRAL basis

```rust
use afrobim_drainage::design::{sanral::*, DesignBasis, ChannelInput, ChannelGeometry};

// Rational method: runoff from the Table 3.7 shares, Tc by reach, rainfall entered (mm for Tc and T).
let tc = time_of_concentration(&[TcReach::Channel { length_km: 7.25, slope_m_per_m: 0.02146 }])?;
let flow = rational_method(&RationalInput {
    runoff: RunoffInput { /* rural shares, urban parts, alpha, beta, gamma, ReturnPeriod::Years(20) */ ..runoff },
    area_km2: 28.5,
    time_of_concentration_h: tc.total_h,
    rainfall: RainfallInput { point_rainfall_mm: 85.55, source: "Design Rainfall Estimation, station X".into(),
                              area_reduction: AreaReduction::Equation },
})?;                              // flow.peak_flow_m3_s, flow.calculation()
let t_years = design_return_period_years(RoadClass::Class3, 85.0)?;          // 15.0, SDM Figure 8.2

// Channel: the shared Manning check with SANRAL's Table 5.3 freeboard.
let c = check_channel_sanral(&ChannelInput { basis: DesignBasis::Sanral, /* section, n, S, Q ... */ ..input },
                             ChannelKind::Trapezoidal, None)?;   // c.passes(), c.calculation()

// Culvert: inlet (Table 7.2) and outlet (Eq. 7.4) control, H1/D <= 1.2, Q2T against min(2D, SBP).
let r = culvert_check_sanral(&SanralCulvertInput {
    barrel: SdmBarrel::Round { diameter_m: 1.8 }, barrels: 6, length_m: 25.0, slope_m_per_m: 0.0015,
    slope_unit: SlopeUnit::MetrePerMetre,    // the manual does not say; the caller must choose
    k_in: 0.5, k_out: 1.0, friction: BarrelFriction::Colebrook { ks_m: 0.002 },
    design: CulvertFlood { flow_m3_s: 44.5, h2_m: 2.175 }, twice: None,
    shoulder_break_height_m: Some(2.5), road_class: RoadClass::Class3,
})?;                              // r.passes(), r.calculation()

// Side drains and culvert layout notes work with the new basis as with the others:
SideDrainModule::with_basis(DesignBasis::Sanral);
PipeCulvertModule::with_basis(DesignBasis::Sanral);
```

## Limits

- Uniform flow only: no backwater from culverts or outfalls, no drawdown, no bends.
- Rainfall intensity is always an input, under every basis.
- Manning's n of a drain is always an input.
- The gradient of a drain is entered; which way it falls is not modelled.
- `PipeCulvertDefinition::outlet_control` alone is still **half a culvert check** and says so. The
  complete MoWT check is `culvert_check` (inlet control by FHWA HDS-5 equations, then the greater of
  the two). The headwater it returns is the equation value; the HDS-5 nomographs agree with the
  equations to within 10 percent (HDS-5 A.4), so a nomograph reading can differ.
- Inlet control covers circular concrete and corrugated metal pipe and concrete boxes with the inlet
  rows of HDS-5 Table A.1 listed in `InletEdge`. Skewed, offset, tapered, embedded, arch and
  pipe-arch inlets are not implemented. The flow zone boundaries (1.93 and 2.21 in SI) are "about"
  values, and the transition between them is this crate's straight line, not an HDS-5 equation.
- The headwater ratio is refused above the top of the SI nomograph scales (6 for Charts 1 and 2; see
  `InletEdge::chart_limit_hw_over_d`); nothing is extrapolated. HDS-5 Form 2 has no slope term as
  printed.
- The `culvert_check` outlet control is full-barrel (DDM Step 8) with the DDM validity warnings; flow
  over the road, performance curves and backwater are not done.
- Under the SANRAL basis, the culvert relations of Table 7.2 need a `SlopeUnit` the manual does not
  state, `Kin`, `Kout`, `H2` and roughness are inputs, and the Hershfield, DDF and Design Rainfall
  routes are not included: enter the point rainfall.
- Layout notes cover only what the chosen basis' documents state. No note does not mean no rule.
- The app (`afrobim-app`) still selects a basis with a MoWT/DMRB switch. `DesignBasis::Sanral` is not
  reachable from it until the app is wired (see the examples above).

## Not in this cut

Box and slab culverts as model elements (the box inlet control equations exist in `design::inlet`
for the API, but there is no box element) and bridges; flow over the road, performance
curves and the approximate capacity tables (DDM Appendices 9.19 to 9.21); headwalls,
wingwalls, aprons and bedding; energy dissipators and scour; subsurface drainage; kerbs, gutters,
inlets, storm sewers and gullies; DMRB outlets, gratings, weirs and combined channel and pipe
systems; catchment delineation; the SCS, unit hydrograph and TRRL flood models; DDM Table 4.1
formulae Nos. 1 to 6; detention; costs.
