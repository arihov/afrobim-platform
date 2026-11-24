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
//! For a line of quantity `q` in unit `u`, matched by exact category to the first rule, the rule's
//! `unit` must equal `u`. Then:
//!
//! * tonnes: `u = t`: `q`; `kg`: `q / 1000`; `m3`: `q * t_per_m3`; `m2`: `q * t_per_m2`;
//!   `m`: `q * t_per_m`; `nr`: `q * t_per_nr`, the factors coming from the line's factor row. A
//!   missing conversion makes the line unmatched.
//! * waste: `waste_t = tonnes * waste_pct / 100` when the rule has `waste_pct`, else 0.
//! * material carbon (A1-A3): `a1a3 = (tonnes + waste_t) * kgco2e_per_t`.
//! * transport (A4): `a4 = (tonnes + waste_t) * haul_km * transport_kgco2e_per_tkm` when the rule
//!   has `haul_km` and the factor file has the transport factor, else 0.
//! * a `HaulOnly` line moves material without carrying material carbon: `a1a3 = 0`, density from
//!   its factor, and it is counted only when `haul_km` and the transport factor are both present.
//! * `total_kg = a1a3 + a4`. `embodied_kgco2e` is the sum of `total_kg` over counted lines, so it
//!   includes A4; `transport_kgco2e` is the A4 part of it.
//! * lighting, over `lighting.luminaire` lines: `luminaires = sum(nr)`;
//!   `installed_kw = sum(nr * power_w) / 1000` (attribute `power_w`);
//!   `kwh_per_year = installed_kw * hours_per_year * dimming_factor`;
//!   `kgco2e_per_year = kwh_per_year * grid_kgco2e_per_kwh`;
//!   `life_kgco2e = kgco2e_per_year * design_life_years`. A missing input leaves what depends on
//!   it as `None` and names it in `missing`; the dimming factor is never assumed to be 1. Lighting
//!   energy is never added into `embodied_kgco2e`.
//!
//! Every line ends in exactly one outcome, so `counted + excluded + unmatched + energy_lines`
//! equals the number of lines.

mod assess;
mod csv;
mod factors;
mod output;
mod rules;

pub use assess::{assess, Assessment, LightingResult, LineIn, MaterialTotal, Outcome, Row};
pub use factors::{factors_from_csv, factors_to_csv, normalise_key, template_csv, Factor, FactorSet, FACTOR_COLUMNS};
pub use output::assessment_csv;
pub use rules::{default_rules, LightingUse, Rule, Settings, Use};
