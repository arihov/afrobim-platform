// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! AfroBIM green BIM for road works: embodied carbon, material waste and transport from the
//! model's quantity lines, and the operational energy of street lighting.
//!
//! ```text
//!  quantity lines (LineIn) --rules--> counted | excluded | unmatched | energy
//!        (from the app)     (Settings)       |
//!  factor file (user's) ------------------> tonnes, waste, A1-A3, A4 per row
//!                                            |--> totals by material, embodied total
//!  lighting settings + grid factor (user's) --> kW, kWh/year, kgCO2e/year, over design life
//! ```
//!
//! This crate contains no carbon factors, densities, masses, waste percentages, haul distances,
//! burning hours, dimming levels, design lives or grid factors. Every number is the user's, from
//! the factor file ([`FactorSet`]) or the settings ([`Settings`]). With no factor for a material
//! nothing is guessed: the line is reported unmatched with the reason, and a total is `None`
//! rather than 0 when nothing could be counted. The numbers in the tests are invented fixtures.
//!
//! # Formulae
//!
//! For a line of quantity `q` in unit `u`, the first rule with the line's exact category and unit
//! (`no` reads as `nr`) is used; a category can have a rule per unit. A category that has rules
//! but none in `u` is looked at only to give the reason: the line is unmatched, naming the units
//! the category's rules expect. A quantity that is not a number or is
//! negative is unmatched; a quantity of 0 is excluded ("quantity is 0: not modelled"), so
//! nothing counted at zero can make a zero total. Then:
//!
//! * tonnes: `u = t`: `q`; `kg`: `q / 1000`; `m3`: `q * t_per_m3`; `m2`: `q * t_per_m2`;
//!   `m`: `q * t_per_m`; `nr`: `q * t_per_nr`, the factors coming from the line's factor row. A
//!   missing conversion makes the line unmatched.
//! * waste: `waste_t = tonnes * waste_pct / 100` when the rule has `waste_pct`, else 0.
//! * material carbon (A1-A3): `a1a3 = (tonnes + waste_t) * kgco2e_per_t`. A rule that looks the
//!   factor up by a line attribute (a pavement material, a post section) can name a fallback key
//!   for lines without the attribute.
//! * transport (A4): `a4 = (tonnes + waste_t) * haul_km * transport_kgco2e_per_tkm` when the rule
//!   has `haul_km` and the factor file has the transport factor, else 0.
//! * a `HaulOnly` line moves material without carrying material carbon: `a1a3 = 0`, density from
//!   its factor, and it is counted only when `haul_km` and the transport factor are both present.
//! * `total_kg = a1a3 + a4`. `embodied_kgco2e` is the sum of `total_kg` over counted lines: A1-A3
//!   for all of them and A4 only for those whose rule has a `haul_km` (and the file a transport
//!   factor), so it is labelled [`EMBODIED_BASIS`]. `counted_with_a4` and `counted_without_a4`
//!   (and the same for waste) count the lines each way, and the notes list the counted rules that
//!   have no haul or no waste. `transport_kgco2e` is the A4 part of the total.
//! * lighting, over `lighting.luminaire` lines: `luminaires = sum(nr)`;
//!   `installed_kw = sum(nr * power_w) / 1000` (attribute `power_w`), None when there are no
//!   luminaire lines or any lacks `power_w`;
//!   `kwh_per_year = installed_kw * hours_per_year * dimming_factor`;
//!   `kgco2e_per_year = kwh_per_year * grid_kgco2e_per_kwh`;
//!   `life_kgco2e = kgco2e_per_year * design_life_years`. A missing input leaves what depends on
//!   it as `None` and names it in `missing`; the dimming factor is never assumed to be 1. Lighting
//!   energy is never added into `embodied_kgco2e`.
//!
//! Every line ends in exactly one outcome, so `counted + excluded + unmatched + energy_lines`
//! equals the number of lines. A gantry whose members lack a mass per metre has no steel mass
//! line; its count line is then unmatched ("no steel mass") rather than excluded as carried by
//! steel that is not there.
//!
//! What the user changed from the default rules is kept as [`Overrides`] (waste, haul, and a use
//! the user changed), merged over [`default_rules`] when loaded, so a default rule added later
//! reaches old projects.

mod assess;
mod csv;
mod factors;
mod output;
mod rules;

pub use assess::{assess, Assessment, LightingResult, LineIn, MaterialTotal, Outcome, Row, EMBODIED_BASIS};
pub use factors::{factors_from_csv, factors_to_csv, normalise_key, template_csv, Factor, FactorSet, FACTOR_COLUMNS};
pub use output::assessment_csv;
pub use rules::{default_rules, LightingUse, Overrides, Rule, RuleOverride, Settings, Use};
