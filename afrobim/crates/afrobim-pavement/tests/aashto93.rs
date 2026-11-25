// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! AASHTO 1993 flexible design as reproduced in FHWA NHI-05-037 Appendix C ("FHWA") and in the
//! University of Anbar lecture notes by Fadhil and Ahmed ("Notes"): the published worked
//! examples and the tables.

use afrobim_pavement::design::aashto93::suggestions::*;
use afrobim_pavement::design::aashto93::*;
use afrobim_pavement::design::{design as run, DesignInput, DesignMethod, DesignResult};
use afrobim_pavement::Material;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn sn(w18: f64, reliability: f64, s0: f64, delta_psi: f64, mr: f64) -> f64 {
    required_structural_number(w18, standard_normal_deviate(reliability).unwrap(), s0, delta_psi, mr).unwrap()
}

fn layer(name: &str, material: Material, inches: f64, a: f64, m: f64, modulus: Option<f64>) -> SnLayer {
    SnLayer { name: name.to_string(), material, thickness_mm: inches * 25.4, layer_coefficient: a, drainage_coefficient: m, resilient_modulus_psi: modulus }
}

/// FHWA NHI-05-037 Appendix C, flexible procedure: W18 = 11.6 million, R = 90 % (ZR = −1.282),
/// S0 = 0.45, ΔPSI = 1.7, MR = 7 500 psi, a1 = 0.44, a2 = 0.17, m2 = 1.0, EBS = 40 000 psi.
/// "Solve Eq. (C.2) for the required overall structural number: SN = 5.07."
#[test]
fn fhwa_appendix_c_worked_example() {
    let required = sn(11.6e6, 90.0, 0.45, 1.7, 7500.0);
    assert_eq!(format!("{required:.2}"), "5.07");
    // The solution satisfies the equation.
    assert!(close(log10_w18(-1.282, 0.45, required, 1.7, 7500.0), 11.6e6f64.log10(), 1e-10));

    // The example's structure: 6 in of asphalt on 14 in of granular base.
    // "SN2 = SN − D1·a1 = 2.43"; "D2 = SN2 / (m2·a2) = 14.3 → 14 inches".
    let input = Aashto93Input {
        w18: 11.6e6,
        reliability: Reliability::Percent(90.0),
        overall_standard_deviation: 0.45,
        delta_psi: 1.7,
        roadbed_modulus_psi: 7500.0,
        layers: vec![layer("Asphalt concrete", Material::HotMixAsphalt, 6.0, 0.44, 1.0, None), layer("Granular base", Material::GranularBase, 14.0, 0.17, 1.0, Some(40_000.0))],
    };
    let d = design(&input).unwrap();
    assert_eq!(d.zr, -1.282);
    assert!(close(d.provided_sn, 6.0 * 0.44 + 14.0 * 0.17, 1e-12)); // 5.02
    assert_eq!(format!("{:.2}", d.required_sn - 6.0 * 0.44), "2.43");
    let roadbed = d.checks.last().unwrap();
    assert_eq!(roadbed.support, "roadbed");
    // The base thickness that meets SN exactly: 14.3 in, as the example has before rounding.
    assert_eq!(format!("{:.1}", roadbed.thickness_needed_mm.unwrap() / 25.4), "14.3");
    // Rounded down to 14 in, as the example does, the structure is 0.05 short of 5.07, and the
    // check says so rather than rounding with it.
    assert!(!roadbed.passes() && !d.adequate() && d.layers().is_none());

    // A difference recorded, not hidden. The example prints SN1 = 2.62 above the base
    // (EBS = 40 000 psi), giving D1 = 5.95 → 6 in. Equation C.2 as printed gives 2.74 for those
    // inputs; with SN = 2.62 it carries 8.8 million ESALs, not 11.6 million.
    let above_base = &d.checks[0];
    assert_eq!((above_base.support.as_str(), above_base.support_modulus_psi), ("Granular base", 40_000.0));
    assert_eq!(format!("{:.2}", above_base.required_sn), "2.74");
    assert_eq!(format!("{:.1}", 10f64.powf(log10_w18(-1.282, 0.45, 2.62, 1.7, 40_000.0)) / 1.0e6), "8.8");
    assert!(!above_base.passes());
    assert_eq!(format!("{:.0}", above_base.thickness_needed_mm.unwrap()), "158");

    // With 6.5 in of asphalt and 15 in of base every check passes and the stack is offered.
    let mut thicker = input.clone();
    thicker.layers[0].thickness_mm = 6.5 * 25.4;
    thicker.layers[1].thickness_mm = 15.0 * 25.4;
    let d = design(&thicker).unwrap();
    assert!(d.adequate());
    let layers = d.layers().unwrap();
    assert_eq!((layers[0].name.as_str(), layers[0].thickness_mm, layers[1].material.clone()), ("Asphalt concrete", 165.1, Material::GranularBase));
    let calc = d.calculation();
    assert!(calc[0].contains("FHWA NHI-05-037") && calc[0].contains("University of Anbar"));
    assert!(calc.iter().any(|l| l.contains("R = 90%, ZR = -1.282 (FHWA NHI-05-037 Table C-3)")));
    assert!(calc.iter().any(|l| l.contains("SN = 5.07")));
    assert!(calc.iter().any(|l| l.starts_with("Above the Granular base (MR = 40000 psi): SN required = 2.74, provided = 2.86: OK.")));
    assert!(calc.iter().any(|l| l.starts_with("Above the roadbed (MR = 7500 psi): SN required = 5.07, provided = 5.41: OK.")));

    let boxed = run(&DesignInput::Aashto93(thicker)).unwrap();
    assert_eq!(boxed.method(), DesignMethod::Aashto1993);
    assert_eq!(DesignMethod::all().len(), 3);
}

/// Notes: the example printed on the design nomograph (W18 = 5 × 10⁶, R = 95 %, S0 = 0.35,
/// MR = 5 000 psi, ΔPSI = 1.9, "Solution: SN = 5.0") and the Appendix H example (W18 =
/// 18.6 × 10⁶, MR = 5.7 × 10³ psi, R = 95 %, ΔPSI = 2.1, S0 = 0.35, "first SN ... = 5.6").
/// Both answers are chart readings; the equation is within 0.1 of each.
#[test]
fn lecture_notes_nomograph_examples() {
    let chart = sn(5.0e6, 95.0, 0.35, 1.9, 5000.0);
    assert_eq!(format!("{chart:.1}"), "5.0");
    let appendix_h = sn(18.6e6, 95.0, 0.35, 2.1, 5700.0);
    assert_eq!(format!("{appendix_h:.2}"), "5.55");
    assert!((appendix_h - 5.6).abs() < 0.1);
    // Its stage reliability: "R individual = (0.9)^0.5 = 0.95".
    assert_eq!(format!("{:.0}", stage_reliability_percent(90.0, 2)), "95");
    assert!(close(stage_reliability_percent(90.0, 1), 90.0, 1e-12));
    // Less serviceability to lose, or a weaker roadbed, needs more structure.
    assert!(sn(18.6e6, 95.0, 0.35, 1.89, 5700.0) > appendix_h);
    assert!(sn(18.6e6, 95.0, 0.35, 2.1, 4000.0) > appendix_h);
    assert!(sn(18.6e6, 99.0, 0.35, 2.1, 5700.0) > appendix_h);
}

/// Notes, Example 1: 4 600 veh/day two-way on a four-lane rural highway, 2.5 % growth over 20
/// years (G = 25.54), DD = 50 %, DL = 85 %; "W18 = 5.35 × 10⁶".
#[test]
fn lecture_notes_example_1_design_lane_esals() {
    // G is the same series as MoWT Equation 1: ((1 + r)^t − 1) / r.
    let g = afrobim_pavement::design::mowt::cumulative_traffic(1.0, 2.5, 20) / 365.0;
    assert_eq!(format!("{g:.2}"), "25.54");
    // A slip in the Notes: the table gives 2.24 ESALs for the 6-axle trucks, the calculation
    // line uses 2.44, and the printed answer follows from 2.44.
    let as_calculated = design_lane_esals(&[(460.0, 0.5), (414.0, 0.85), (276.0, 1.2), (138.0, 1.55), (92.0, 2.44)], 25.54, 0.5, 0.85);
    let as_tabulated = design_lane_esals(&[(460.0, 0.5), (414.0, 0.85), (276.0, 1.2), (138.0, 1.55), (92.0, 2.24)], 25.54, 0.5, 0.85);
    assert_eq!(format!("{:.2}", as_calculated / 1.0e6), "5.35");
    assert_eq!(format!("{:.2}", as_tabulated / 1.0e6), "5.28");
}

#[test]
fn effective_roadbed_modulus() {
    // One season: the modulus itself. Relative damage is 1.18×10⁸ · MR^−2.32.
    assert!(close(effective_roadbed_modulus_psi(&[(6000.0, 12.0)]).unwrap(), 6000.0, 1e-6));
    assert!(close(relative_damage(1000.0), 1.18e8 * 1000f64.powf(-2.32), 1e-12));
    // Two equal seasons: between the two, nearer the weaker.
    let e = effective_roadbed_modulus_psi(&[(4000.0, 6.0), (8000.0, 6.0)]).unwrap();
    assert!(e > 4000.0 && e < 6000.0);
    assert!(close(relative_damage(e), 0.5 * (relative_damage(4000.0) + relative_damage(8000.0)), 1e-12));
    // Recorded, not reconciled: the Notes' Appendix H seasons (which add up to 13 months) give
    // 5 470 psi by this expression, where the Notes print 5.7 × 10³ psi.
    let notes = effective_roadbed_modulus_psi(&[(5000.0, 5.0), (6500.0, 5.5), (4000.0, 1.5), (20_000.0, 1.0)]).unwrap();
    assert_eq!(format!("{:.0}", notes / 10.0), "547");
    assert_eq!(effective_roadbed_modulus_psi(&[]), None);
    assert_eq!(effective_roadbed_modulus_psi(&[(0.0, 1.0)]), None);
}

#[test]
fn fhwa_table_c_3_standard_normal_deviates() {
    let expected = [
        (50.0, 0.000), (60.0, -0.253), (70.0, -0.524), (75.0, -0.674), (80.0, -0.841), (85.0, -1.037), (90.0, -1.282), (91.0, -1.340), (92.0, -1.405),
        (93.0, -1.476), (94.0, -1.555), (95.0, -1.645), (96.0, -1.751), (97.0, -1.881), (98.0, -2.054), (99.0, -2.327), (99.9, -3.090), (99.99, -3.750),
    ];
    assert_eq!(STANDARD_NORMAL_DEVIATES, expected);
    for (r, z) in expected {
        assert_eq!(standard_normal_deviate(r), Some(z));
    }
    // Nothing between the listed values.
    assert_eq!(standard_normal_deviate(87.5), None);
    assert_eq!(standard_normal_deviate(40.0), None);
    assert!(STANDARD_NORMAL_DEVIATES.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 < w[0].1));
}

#[test]
fn the_equation_and_its_solution() {
    // Each term at SN = 0, ΔPSI = 2.7, MR = 1 psi, ZR = 0: only −0.20 − 8.07 is left.
    assert!(close(log10_w18(0.0, 0.0, 0.0, 2.7, 1.0), -8.27, 1e-12));
    assert!(close(log10_w18(-1.0, 0.5, 0.0, 2.7, 1.0), -8.77, 1e-12));
    assert!(close(log10_w18(0.0, 0.0, 9.0, 2.7, 1.0), 9.36 - 8.27, 1e-12));
    assert!(close(log10_w18(0.0, 0.0, 0.0, 2.7, 10.0), 2.32 - 8.27, 1e-12));
    assert!(close(log10_w18(0.0, 0.0, 0.0, 0.27, 1.0), -8.27 - 1.0 / 1094.4, 1e-12));
    // The solution reproduces W18 over a range of inputs.
    for w in [5.0e4, 1.0e6, 2.0e7, 1.0e8] {
        for mr in [3000.0, 10_000.0, 30_000.0] {
            let s = required_structural_number(w, -1.645, 0.45, 1.7, mr).unwrap();
            assert!(close(log10_w18(-1.645, 0.45, s, 1.7, mr), w.log10(), 1e-9), "{w} {mr}");
        }
    }
    assert_eq!(required_structural_number(0.0, -1.0, 0.4, 1.7, 5000.0), None);
    assert_eq!(required_structural_number(1.0e6, -1.0, 0.4, 0.0, 5000.0), None);
    assert_eq!(required_structural_number(1.0e30, -1.0, 0.4, 1.7, 5000.0), None);
    // SN = a1·D1 + a2·D2·m2 + a3·D3·m3 with D in inches.
    let layers = [
        layer("AC", Material::HotMixAsphalt, 4.0, 0.44, 1.0, None),
        layer("Base", Material::GranularBase, 6.0, 0.14, 1.2, None),
        layer("Subbase", Material::GranularSubbase, 8.0, 0.11, 0.8, None),
    ];
    assert!(close(structural_number(&layers), 4.0 * 0.44 + 6.0 * 0.14 * 1.2 + 8.0 * 0.11 * 0.8, 1e-12));
}

#[test]
fn inputs_govern_and_bad_ones_are_refused() {
    let base = Aashto93Input { w18: 2.0e6, reliability: Reliability::StandardNormalDeviate(-1.1), overall_standard_deviation: 0.4, delta_psi: 2.0, roadbed_modulus_psi: 6000.0, layers: vec![] };
    let d = design(&base).unwrap();
    assert!(d.checks.is_empty() && !d.adequate() && d.layers().is_none());
    assert!(d.calculation().iter().any(|l| l.contains("ZR = -1.1 (entered)")) && d.calculation().iter().any(|l| l == "No structure was entered to check."));
    assert!(d.warnings().iter().any(|w| w.contains("the Guide itself was not read")));
    let mut bad = base.clone();
    bad.reliability = Reliability::Percent(88.0);
    assert!(design(&bad).unwrap_err().to_string().contains("Table C-3"));
    let mut bad = base.clone();
    bad.w18 = -1.0;
    assert!(design(&bad).is_err());
    let mut bad = base.clone();
    bad.layers = vec![layer("AC", Material::HotMixAsphalt, -1.0, 0.44, 1.0, None)];
    assert!(design(&bad).is_err());
}

/// The values the sources print, offered as labelled suggestions only.
#[test]
fn cited_suggestions() {
    use DrainageQuality::*;
    use FunctionalClass::*;
    use MoistureExposure::*;
    assert!(NOTES_SOURCE.contains("University of Anbar") && NOTES_SOURCE.contains("as reproduced from the AASHTO 1993 Guide"));
    assert!(FHWA_SOURCE.contains("FHWA NHI-05-037") && FHWA_SOURCE.contains("as reproduced from the AASHTO 1993 Guide"));
    // FHWA Table C-2 (Notes Table 4.4).
    assert_eq!(reliability_range_percent(InterstateAndOtherFreeways, true), (85.0, 99.9));
    assert_eq!(reliability_range_percent(InterstateAndOtherFreeways, false), (80.0, 99.9));
    assert_eq!(reliability_range_percent(PrincipalArterials, true), (80.0, 99.0));
    assert_eq!(reliability_range_percent(PrincipalArterials, false), (75.0, 95.0));
    assert_eq!(reliability_range_percent(Collectors, true), (80.0, 95.0));
    assert_eq!(reliability_range_percent(Collectors, false), (75.0, 95.0));
    assert_eq!(reliability_range_percent(Local, true), (50.0, 80.0));
    assert_eq!(reliability_range_percent(Local, false), (50.0, 80.0));
    assert_eq!(S0_FLEXIBLE_FHWA, (0.35, 0.50, 0.45));
    assert_eq!(SERVICEABILITY_FLEXIBLE_FHWA, (4.2, 2.5));
    assert_eq!(ROAD_TEST_LAYER_COEFFICIENTS, (0.44, 0.14, 0.11));
    // Notes Equations 4.17 and 4.18 with the Notes' own check values:
    // "a2 = 0.14 for EBS = 30000 psi"; "a3 = 0.11 for ESB = 15000 psi".
    assert_eq!(format!("{:.2}", granular_base_coefficient(30_000.0)), "0.14");
    assert_eq!(format!("{:.2}", granular_subbase_coefficient(15_000.0)), "0.11");
    // Notes Table 4.6, row by row.
    let row = |q| [LessThan1Percent, From1To5Percent, From5To25Percent, GreaterThan25Percent].map(|e| drainage_coefficient_range(q, e));
    assert_eq!(row(Excellent), [(1.40, 1.35), (1.35, 1.30), (1.30, 1.20), (1.20, 1.20)]);
    assert_eq!(row(Good), [(1.35, 1.25), (1.25, 1.15), (1.15, 1.00), (1.00, 1.00)]);
    assert_eq!(row(Fair), [(1.25, 1.15), (1.15, 1.05), (1.00, 0.80), (0.80, 0.80)]);
    assert_eq!(row(Poor), [(1.15, 1.05), (1.05, 0.80), (0.80, 0.60), (0.60, 0.60)]);
    assert_eq!(row(VeryPoor), [(1.05, 0.95), (0.95, 0.75), (0.75, 0.40), (0.40, 0.40)]);
}
