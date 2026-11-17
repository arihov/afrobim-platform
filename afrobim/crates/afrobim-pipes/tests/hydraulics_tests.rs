//! Hydraulics against hand-worked numbers. The pipe sizes, roughness values and viscosity here are
//! test inputs chosen for easy arithmetic; they are not recommendations.

use afrobim_drainage::design::{normal_depth_m, ChannelGeometry};
use afrobim_pipes::hydraulics::{friction_slope, uniform_velocity_m_s};
use afrobim_pipes::*;

fn circ(d: f64) -> PipeShape {
    PipeShape::Circular { diameter_m: d }
}

#[test]
fn full_flow_circular_manning_hand_worked() {
    // D = 0.6 m, n = 0.013, S = 0.01 (test inputs)
    // A = pi D^2 / 4 = pi * 0.36 / 4 = 0.2827433 m2
    // R = A / P = D / 4 = 0.15 m;  R^(2/3) = exp((2/3) ln 0.15) = 0.2823108
    // V = R^(2/3) S^(1/2) / n = 0.2823108 * 0.1 / 0.013 = 2.171622 m/s
    // Q = V A = 2.171622 * 0.2827433 = 0.614012 m3/s
    let f = full_flow(&circ(0.6), &Roughness::Manning { n: 0.013 }, 0.01, &HydraulicSettings::default()).unwrap();
    assert!((f.area_m2 - 0.2827433388).abs() < 1e-9);
    assert!((f.hydraulic_radius_m - 0.15).abs() < 1e-12);
    assert!((f.velocity_m_s - 2.1716216051).abs() < 1e-8);
    assert!((f.discharge_m3_s - 0.6140115433).abs() < 1e-8);
}

#[test]
fn half_full_circular_carries_half_the_full_flow_at_the_full_velocity() {
    // At y = D/2 a circle has half the area and the same hydraulic radius as when full, so with a
    // constant n: Q = Qfull / 2 and V = Vfull. Solving Q = Qfull/2 must return y/D = 0.5.
    let rough = Roughness::Manning { n: 0.013 };
    let s = HydraulicSettings::default();
    let full = full_flow(&circ(0.6), &rough, 0.01, &s).unwrap();
    let nf = normal_flow(&circ(0.6), &rough, 0.01, full.discharge_m3_s / 2.0, &s).unwrap();
    assert!((nf.depth_ratio - 0.5).abs() < 1e-9, "{}", nf.depth_ratio);
    assert!((nf.velocity_m_s - full.velocity_m_s).abs() < 1e-8);
    assert!(!nf.exceeds_capacity);
}

#[test]
fn partial_flow_depth_is_recovered_from_the_discharge() {
    // y = 0.18 m in D = 0.6 m (y/D = 0.3): theta = 2 acos(1 - 2*0.3) = 2 acos(0.4) = 2.318559 rad
    // A = D^2 (theta - sin theta) / 8 = 0.36 * (2.318559 - 0.733201) / 8 = 0.0713406 m2
    // P = D theta / 2 = 0.6955677 m;  R = 0.1025646 m;  V = R^(2/3) * 0.1 / 0.013 = 1.685472 m/s
    // Q = 1.685472 * 0.0713406 = 0.120243 m3/s  (0.1958 of the full-bore 0.614012)
    let rough = Roughness::Manning { n: 0.013 };
    let nf = normal_flow(&circ(0.6), &rough, 0.01, 0.1202426100, &HydraulicSettings::default()).unwrap();
    assert!((nf.depth_m - 0.18).abs() < 1e-6, "{}", nf.depth_m);
    assert!((nf.area_m2 - 0.0713406083).abs() < 1e-7);
    assert!((nf.velocity_m_s - 1.6854721730).abs() < 1e-5);
    // Froude number F = V / sqrt(g A / T); T = D sin(theta/2) = 0.6 * sin(1.1592795) = 0.5499
    let t = 0.6 * (2.318558961454817_f64 / 2.0).sin();
    let fr = nf.velocity_m_s / (9.81 * nf.area_m2 / t).sqrt();
    assert!((nf.froude.unwrap() - fr).abs() < 1e-6);
}

#[test]
fn box_section_agrees_with_the_drainage_rectangle() {
    // 1.2 x 0.8 m box, n = 0.015, S = 0.004 (test inputs).
    // Full: A = 0.96, P = 4.0, R = 0.24, R^(2/3) = 0.3861958, V = 0.3861958 * 0.0632456 / 0.015 = 1.628344, Q = 1.563211
    // At y = 0.5: A = 0.6, P = 2.2, R = 0.272727, V = 1.773200, Q = 1.063920 (an open rectangle)
    let shape = PipeShape::Box { width_m: 1.2, height_m: 0.8 };
    let rough = Roughness::Manning { n: 0.015 };
    let s = HydraulicSettings::default();
    let full = full_flow(&shape, &rough, 0.004, &s).unwrap();
    assert!((full.discharge_m3_s - 1.5632105022).abs() < 1e-8);
    let nf = normal_flow(&shape, &rough, 0.004, 1.063920147856379, &s).unwrap();
    assert!((nf.depth_m - 0.5).abs() < 1e-7);
    // the reused drainage solver for a rectangular channel gives the same depth
    let y = normal_depth_m(&ChannelGeometry::rectangle(1.2), 0.015, 0.004, 1.063920147856379).unwrap();
    assert!((y - nf.depth_m).abs() < 1e-7);
}

#[test]
fn colebrook_white_full_circular_hand_worked() {
    // D = 0.6 m, S = 0.01, ks = 1.5 mm, nu = 1.31e-6 m2/s (test inputs); D_h = 4R = 0.6 m
    // sqrt(2 g D S) = sqrt(2 * 9.81 * 0.6 * 0.01) = 0.3431035
    // V = -2 * 0.3431035 * log10(0.0015 / (3.7 * 0.6) + 2.51 * 1.31e-6 / (0.6 * 0.3431035)) = 2.168493 m/s
    // Q = V A = 2.168493 * 0.2827433 = 0.613127 m3/s
    let s = HydraulicSettings { kinematic_viscosity_m2_s: Some(1.31e-6) };
    let rough = Roughness::ColebrookWhite { ks_m: 0.0015 };
    let f = full_flow(&circ(0.6), &rough, 0.01, &s).unwrap();
    assert!((f.velocity_m_s - 2.1684928316).abs() < 1e-8);
    assert!((f.discharge_m3_s - 0.6131269034).abs() < 1e-8);
    // the friction slope recovered from that velocity is the bed slope: Darcy-Weisbach with Colebrook f
    let sf = friction_slope(&rough, f.velocity_m_s, f.hydraulic_radius_m, &s).unwrap();
    assert!((sf - 0.01).abs() < 1e-8, "{sf}");
    // no viscosity, no answer
    assert!(uniform_velocity_m_s(&rough, 0.15, 0.01, &HydraulicSettings::default()).is_err());
}

#[test]
fn flow_above_full_bore_is_reported_as_exceeding_capacity() {
    let rough = Roughness::Manning { n: 0.013 };
    let nf = normal_flow(&circ(0.6), &rough, 0.01, 0.7, &HydraulicSettings::default()).unwrap();
    assert!(nf.exceeds_capacity);
    assert_eq!(nf.depth_ratio, 1.0);
    // V = Q / A_full = 0.7 / 0.2827433 = 2.475 m/s
    assert!((nf.velocity_m_s - 0.7 / 0.2827433388230814).abs() < 1e-9);
}

#[test]
fn elliptical_section_with_equal_axes_matches_the_circle() {
    let c = circ(0.6).section_at(0.18).unwrap();
    let e = PipeShape::Elliptical { width_m: 0.6, height_m: 0.6 }.section_at(0.18).unwrap();
    assert!((c.area_m2 - e.area_m2).abs() / c.area_m2 < 2e-4, "{} {}", c.area_m2, e.area_m2);
    assert!((c.wetted_perimeter_m - e.wetted_perimeter_m).abs() / c.wetted_perimeter_m < 2e-4);
    let full = PipeShape::Elliptical { width_m: 1.0, height_m: 0.6 }.full_area_m2().unwrap();
    // ellipse area pi a b with a = 0.5, b = 0.3
    assert!((full - std::f64::consts::PI * 0.5 * 0.3).abs() < 1e-4);
}

#[test]
fn egg_without_a_profile_has_no_hydraulics() {
    let egg = PipeShape::Egg { width_m: 0.8, height_m: 1.2, profile: vec![] };
    assert!(egg.section_at(0.5).is_none());
    assert!(normal_flow(&egg, &Roughness::Manning { n: 0.013 }, 0.01, 0.1, &HydraulicSettings::default()).is_err());
    // with a user profile it works: a 0.8 m wide, 1.2 m high rectangle written as a profile
    let egg = PipeShape::Egg { width_m: 0.8, height_m: 1.2, profile: vec![[0.0, 0.8], [1.2, 0.8]] };
    let s = egg.section_at(0.5).unwrap();
    assert!((s.area_m2 - 0.4).abs() < 1e-12);
    assert!((s.wetted_perimeter_m - (0.8 + 1.0)).abs() < 1e-12);
}
