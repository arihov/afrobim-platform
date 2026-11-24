# afrobim-green

Green BIM for road works: embodied carbon, material waste and transport from the model's
quantity lines, and the operational energy of street lighting.

## What this crate does not contain

**No carbon factors, densities, masses, waste percentages, haul distances, burning hours,
dimming levels, design lives or grid factors.** Every number comes from the user's factor file
or the user's settings. With no factor for a material nothing is guessed: the line is reported
unmatched with the reason. The numbers in the tests are invented fixtures for easy hand
arithmetic and are not data about any material, grid or road.

The crate is independent of `afrobim-cost`. The app converts its quantity lines into `LineIn`.

## Data flow

```text
LineIn (id, mark, category, description, quantity, unit, attributes)
   | first rule with the same category and unit   (Settings.rules; default_rules() is category -> use only)
   v
Excluded(reason) | Energy | Unmatched(reason) | Counted
   | factor file (FactorSet, the user's)    tonnes, waste, A1-A3, A4
   v
Assessment: rows, totals by material, embodied total, transport part, coverage,
            lighting block, notes        --> assessment_csv
```

## Formulae

For a line of quantity `q` and unit `u`:

| Step | Formula |
|---|---|
| rule and unit | the rule is the first with the line's category and unit (case and spaces ignored; `no` reads as `nr`). A category with rules but none in the line's unit is unmatched, naming the units its rules expect |
| quantity | not finite or negative: unmatched; 0: excluded ("quantity is 0: not modelled") |
| tonnes | `t`: `q`; `kg`: `q / 1000`; `m3`: `q * t_per_m3`; `m2`: `q * t_per_m2`; `m`: `q * t_per_m`; `nr`: `q * t_per_nr` |
| waste | `waste_t = tonnes * waste_pct / 100` when the rule has `waste_pct`, else 0 |
| material carbon A1-A3 | `a1a3 = (tonnes + waste_t) * kgco2e_per_t` |
| transport A4 | `a4 = (tonnes + waste_t) * haul_km * transport_kgco2e_per_tkm` when the rule has `haul_km` and the factor file has the transport factor, else 0 |
| haul only | `a1a3 = 0`; density from the factor; counted only when `haul_km` and the transport factor are both present |
| row total | `total_kg = a1a3 + a4` |
| embodied | `embodied_kgco2e = sum(total_kg)` over counted rows: A1-A3, plus A4 where a haul was entered (`EMBODIED_BASIS`); `counted_with_a4` / `counted_without_a4` and `counted_with_waste` / `counted_without_waste` count the lines each way, and notes list the counted rules with no haul or no waste; `transport_kgco2e` is its A4 part |
| installed power | `installed_kw = sum(nr * power_w) / 1000` (attribute `power_w` on `lighting.luminaire` lines) |
| energy | `kwh_per_year = installed_kw * hours_per_year * dimming_factor` |
| operational carbon | `kgco2e_per_year = kwh_per_year * grid_kgco2e_per_kwh`; `life_kgco2e = kgco2e_per_year * design_life_years` |

Lighting energy is never added to `embodied_kgco2e`. The dimming factor (an energy fraction, 0 to
1 inclusive) is never assumed to be 1: if it is missing, `kwh_per_year` is `None` and `missing`
names it. Installed power and energy are also `None` while any luminaire line lacks `power_w`, and
installed power is `None` with no luminaire lines, so a partial sum or a zero is never shown as the
whole.

Reasons an assessed line is **Unmatched**: `no rule for <category>`; the unit differs from the
rule's (both named); `no factor file loaded`; `no factor for <key>`; `<key>: no t_per_<unit> in
the factor file`; `<key>: no kgco2e_per_t in the factor file`; a missing attribute for a
`MaterialFromAttribute` rule; an invalid `waste_pct` or `haul_km`; a haul-only line without
`haul_km` or the transport factor; a quantity that is not a finite number or is negative; a
gantry count line whose gantry has no steel mass line (`no steel mass: a member has no mass per
metre`).

`counted + excluded + unmatched + energy_lines == lines.len()` always. `embodied_kgco2e` is
`None` when no factor file is loaded or nothing was counted, never 0 standing for zero carbon.

## Rules

`Rule { category, unit, use_, waste_pct, haul_km }`, matched by exact category and unit, first match
wins. `Use` is one of `Material { key }`, `MaterialFromAttribute { attribute, prefix, fallback }` (key
is `prefix + attribute value`, e.g. `pavement: Hot mix asphalt`; the `fallback` key is used when the
attribute is missing), `HaulOnly { key }`,
`LightingEnergy` and `Excluded { reason }`. Keys are matched against the factor file ignoring
case and extra white space.

`default_rules()` gives the category to use only, with no numbers. Where several categories
describe the same physical quantity exactly one carries the material and the others are
excluded with a reason: the pavement volume (not its area), the post length by section (not
its mass, which comes from a built-in steel density, nor its count or coating), the barrier length (not posts or terminals), the lining volume (not its area
or the drain length), the gantry steel mass, the sign face area, the marking painted area, the
culvert length. Earthworks fill is `HaulOnly` (key `earth fill`); cut and excavation are
excluded so moved earth is counted once. The keys the defaults look up (`concrete`,
`structural steel`, `steel post`, `safety barrier`, ...) must exist in your factor file or the
lines are reported unmatched. Waste and haul are entered per rule by the user. The app keeps only
the user's changes (`Overrides`) and merges them over `default_rules()` when a project is opened.

## Factor file

```text
# name: <your name for this file>
# source: <publication>
# edition: <edition>
# boundary: A1-A3
# transport_kgco2e_per_tkm: <number>
# grid_kgco2e_per_kwh: <number>
# grid_source: <publication>
key,material,kgco2e_per_t,t_per_m3,t_per_m2,t_per_m,t_per_nr,source,edition
```

Empty cells are not entered. Fields can be quoted with `""` for a quote; a field cannot hold a
line break. Negative, non-finite or unreadable numbers, short rows and repeated keys are errors
that name the 1-based line. An empty `kgco2e_per_t` is kept as "not entered" (NaN in memory,
`null` in JSON); the line is reported unmatched, never priced at zero.
`template_csv(&[(key, material), ...])` writes a blank file to fill from the source you choose.

## Output

`assessment_csv` writes blocks separated by a blank line: factor set, every line with outcome,
reason and figures, totals by material, embodied and transport totals, coverage counts, the
lighting block with its missing inputs, and notes. An empty figure means not calculated.
