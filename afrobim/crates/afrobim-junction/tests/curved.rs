//! A skewed junction on a curved primary, end to end.

mod common;

use afrobim_alignment::{Alignment, Pi, Profile, Pvi};
use afrobim_corridor::{build_model, library, Assembly, Frequency, Layer, OffsetTarget, Slope, Tin};
use afrobim_junction::*;
use common::*;

fn slot_assembly(name: &str) -> Assembly {
    Assembly::symmetric(name, vec![library::lane_to_offset("lane", OffsetTarget::Polyline(vec![]), Slope::Percent(-2.5), vec![Layer::new("Pave1", 0.1)])])
}

#[test]
fn skewed_junction_on_a_curved_primary_builds_end_to_end() {
    // Primary: tangent, R = 600 curve, tangent, at UTM magnitude. Secondary 70 degrees to it at the
    // middle of the curve. Everything from the crossing to the corridor must build, and the corridor
    // must run in `build_model`.
    let mut p = Alignment::from_pis("primary", 0.0, &[Pi::point(E0 - 400.0, N0 - 100.0), Pi::curve(E0, N0, 600.0), Pi::point(E0 + 400.0, N0 - 100.0)]).unwrap();
    let len = p.length();
    p.profile = Some(Profile::new("pp", vec![Pvi::point(0.0, 100.0), Pvi::point(len, 104.0)]).unwrap());
    let mid = p.point_at(len / 2.0).unwrap();
    let h = mid.heading_rad + 70.0f64.to_radians();
    let mut s = line("secondary", mid.x - 300.0 * h.cos(), mid.y - 300.0 * h.sin(), h, 600.0, 0.0);
    s.profile = Some(Profile::new("sp", vec![Pvi::point(0.0, 98.0), Pvi::point(600.0, 99.0)]).unwrap());
    let mut inp = cross_inputs(ReturnKind::Simple { radius_m: 10.0 });
    inp.extents = JunctionExtents { primary_behind_m: 80.0, primary_ahead_m: 80.0, secondary_behind_m: 80.0, secondary_ahead_m: 80.0 };
    inp.primary_edges = edges(3.5, 3.5);
    inp.secondary_edges = edges(3.0, 3.0);
    inp.vertical = Some(vertical_inputs());
    let at_ends = Frequency { at_geometry_points: true, ..Default::default() };
    inp.corridor = Some(CorridorInputs {
        name: None,
        library: vec![slot_assembly("a")],
        primary_approach_assembly: "a".into(),
        primary_through_assembly: "a".into(),
        secondary_approach_assembly: "a".into(),
        secondary_mouth_assembly: "a".into(),
        return_assembly: "a".into(),
        approach_frequency: at_ends.clone(),
        through_frequency: at_ends.clone(),
        mouth_frequency: at_ends.clone(),
        return_frequency: at_ends,
        target_step_m: 0.5,
        end_inset_m: 0.1,
    });
    let res = build_intersection(&inp, &[p.clone(), s.clone()]).unwrap();
    near(res.crossing.angle_rad.to_degrees(), 70.0, 1e-6, "angle");
    near(res.crossing.skew_rad.to_degrees(), 20.0, 1e-6, "skew");
    assert_eq!(res.returns.len(), 4);
    assert!(res.findings.iter().all(|f| f.severity != Severity::Error), "{:?}", res.findings.iter().filter(|f| f.severity == Severity::Error).collect::<Vec<_>>());
    // Tangent lengths are R / tan(half the interior angle): 70 and 110 degrees. The primary is
    // curved (radius 600), so the corner angle is that of the tangent lines at the corner, which
    // differs from 70 by the turn of the primary over the tangent length (14 m / 600 m = 1.3 deg):
    // allow 5 %. The exact checks of tangency on a curve are in `returns.rs`.
    let t70 = 10.0 / (35.0f64.to_radians()).tan();
    let t110 = 10.0 / (55.0f64.to_radians()).tan();
    let tl: Vec<f64> = res.returns.iter().map(|r| r.geometry.primary_tangent_length_m).collect();
    assert!(tl.iter().any(|t| (t - t70).abs() < 0.05 * t70), "{tl:?} vs {t70}");
    assert!(tl.iter().any(|t| (t - t110).abs() < 0.05 * t110), "{tl:?} vs {t110}");
    assert!(res.findings.iter().any(|f| f.code == "junction.on_curve" && f.message.contains("radius 600")));
    let c = res.corridor.as_ref().unwrap();
    let ground = Tin::from_grid(E0 - 600.0, N0 - 600.0, 100.0, 100.0, 13, 13, |_, _| 90.0).unwrap();
    let built = build_model(&c.model, &c.alignments, &ground).unwrap();
    assert_eq!(built.baselines.len(), 7);
    // The build is deterministic.
    let again = build_intersection(&inp, &[p, s]).unwrap();
    assert_eq!(again, res);
}
