//! Layout, rules, checks, grade lines and interference. Sizes, roughness, limits and clearances
//! are test inputs chosen for easy arithmetic. Coordinates are at true UTM magnitude (northing
//! near 9 960 000 m).

use afrobim_pipes::*;

const E0: f64 = 512_345.678;
const N0: f64 = 9_960_000.125;

fn spec(d: f64) -> PipeSpec {
    PipeSpec { shape: PipeShape::Circular { diameter_m: d }, material: Some("example material A".into()), wall_thickness_m: Some(0.05), roughness: Some(Roughness::Manning { n: 0.013 }), part: None }
}

fn pt(de: f64, dn: f64, rim: f64) -> RunPoint {
    RunPoint { easting_m: E0 + de, northing_m: N0 + dn, rim_level_m: rim }
}

fn run_spec(d: f64, start_invert: f64, slope: f64) -> RunSpec {
    RunSpec { structure_kind: StructureKind::Manhole, structure_prefix: "MH".into(), pipe_prefix: "P".into(), pipe: spec(d), start_invert_m: start_invert, slope }
}

/// Three structures 50 m apart along easting, inverts falling 0.01 m/m from 103.0.
fn chain() -> (PipeNetwork, Vec<StructureId>, Vec<PipeId>) {
    let mut n = PipeNetwork::new("chain");
    let (s, p) = n.add_run(&[pt(0.0, 0.0, 106.0), pt(50.0, 0.0, 106.0), pt(100.0, 0.0, 106.0)], &run_spec(0.6, 103.0, 0.01)).unwrap();
    (n, s, p)
}

fn flat(level: f64) -> GroundFn<impl Fn(f64, f64) -> f64> {
    GroundFn(move |_, _| level)
}

#[test]
fn run_is_built_from_positions_with_exact_coordinates() {
    let (n, s, p) = chain();
    assert_eq!(n.structure_count(), 3);
    assert_eq!(n.pipe_count(), 2);
    let st = n.structure(s[1]).unwrap();
    assert_eq!(st.easting_m, E0 + 50.0);
    assert_eq!(st.northing_m, N0);
    assert_eq!(st.name, "MH2");
    // plan length from coordinate differences: 50 m to well under a micrometre at UTM magnitude
    assert!((n.plan_length_m(p[0]).unwrap() - 50.0).abs() < 1e-8);
    // 103.0 falling 0.01 * 50 = 0.5 per pipe
    assert!((n.pipe(p[0]).unwrap().end_invert_m - 102.5).abs() < 1e-9);
    assert!((n.pipe(p[1]).unwrap().end_invert_m - 102.0).abs() < 1e-9);
    assert!((n.slope(p[1]).unwrap() - 0.01).abs() < 1e-9);
    // 3-4-5 triangle: 150 m x 200 m gives 250 m
    let mut m = PipeNetwork::new("t");
    let a = m.add_structure("a", StructureKind::Manhole, E0, N0, 10.0).unwrap();
    let b = m.add_structure("b", StructureKind::Manhole, E0 + 150.0, N0 + 200.0, 10.0).unwrap();
    let pid = m.connect(a, b, &spec(0.3), 9.0, 8.0, None).unwrap();
    assert!((m.plan_length_m(pid).unwrap() - 250.0).abs() < 1e-8);
}

#[test]
fn curved_pipe_arc_length_is_analytic() {
    // quarter circle of radius 20 m: from (0,0) via the 45 degree point to (20,20) about centre (0,20)
    let mut n = PipeNetwork::new("curve");
    let a = n.add_structure("a", StructureKind::Manhole, E0, N0, 10.0).unwrap();
    let r = 20.0_f64;
    let b = n.add_structure("b", StructureKind::Manhole, E0 + r, N0 + r, 10.0).unwrap();
    let pid = n.connect(a, b, &spec(0.3), 9.0, 8.0, None).unwrap();
    let k = std::f64::consts::FRAC_PI_4;
    n.pipe_mut(pid).unwrap().via = Some(PipeVia { easting_m: E0 + r * k.sin(), northing_m: N0 + r - r * k.cos() });
    // centre is (E0, N0 + 20); the 45 degree point of the arc is (E0 + r sin45, N0 + 20 - r cos45)
    let len = n.plan_length_m(pid).unwrap();
    assert!((len - std::f64::consts::PI / 2.0 * r).abs() < 1e-6, "{len}");
    let path = n.pipe_path(pid).unwrap();
    assert!(path.points.len() > 10);
    // every vertex is on the circle of radius 20 about (E0, N0 + 20)
    for p in &path.points {
        let d = (p[0] - E0).hypot(p[1] - (N0 + r));
        assert!((d - r).abs() < 1e-6, "{d}");
    }
}

#[test]
fn flow_accumulates_down_a_tree_and_loops_are_reported() {
    // two branches join at J, then run to the outlet: J gets 0.05 + 0.03 from its feeders and adds 0.02 locally
    let mut n = PipeNetwork::new("tree");
    let a = n.add_structure("A", StructureKind::CatchBasin, E0, N0, 10.0).unwrap();
    let b = n.add_structure("B", StructureKind::CatchBasin, E0, N0 + 40.0, 10.0).unwrap();
    let j = n.add_structure("J", StructureKind::Manhole, E0 + 40.0, N0 + 20.0, 10.0).unwrap();
    let o = n.add_structure("O", StructureKind::Outfall, E0 + 90.0, N0 + 20.0, 9.0).unwrap();
    n.structure_mut(a).unwrap().inflow_m3_s = 0.05;
    n.structure_mut(b).unwrap().inflow_m3_s = 0.03;
    n.structure_mut(j).unwrap().inflow_m3_s = 0.02;
    let s = spec(0.45);
    let pa = n.connect(a, j, &s, 8.0, 7.8, None).unwrap();
    let pb = n.connect(b, j, &s, 8.0, 7.8, None).unwrap();
    let pj = n.connect(j, o, &s, 7.7, 7.2, None).unwrap();
    assert!(n.is_tree());
    let f = n.accumulate_flows();
    assert!(f.findings.is_empty());
    assert!((f.flows[&pa] - 0.05).abs() < 1e-15);
    assert!((f.flows[&pb] - 0.03).abs() < 1e-15);
    assert!((f.flows[&pj] - 0.10).abs() < 1e-15); // 0.05 + 0.03 + 0.02
    // a design flow on a pipe replaces the accumulated one and is passed on
    n.pipe_mut(pb).unwrap().design_flow_m3_s = Some(0.04);
    assert!((n.accumulate_flows().flows[&pj] - 0.11).abs() < 1e-15);
    // a loop
    n.connect(o, a, &s, 7.0, 6.9, None).unwrap();
    let t = n.topology();
    assert!(!t.is_tree());
    assert!(!t.loop_pipes.is_empty());
    let f = n.accumulate_flows();
    assert!(f.findings.iter().any(|x| x.check == "topology" && x.severity == Severity::Error && x.message.contains("loop")));
    // a branch: two pipes leaving one structure
    let mut m = PipeNetwork::new("branch");
    let a = m.add_structure("A", StructureKind::Manhole, E0, N0, 10.0).unwrap();
    let b = m.add_structure("B", StructureKind::Manhole, E0 + 10.0, N0, 10.0).unwrap();
    let c = m.add_structure("C", StructureKind::Manhole, E0, N0 + 10.0, 10.0).unwrap();
    m.structure_mut(a).unwrap().inflow_m3_s = 0.1;
    m.connect(a, b, &s, 8.0, 7.9, None).unwrap();
    m.connect(a, c, &s, 8.0, 7.9, None).unwrap();
    assert!(!m.is_tree());
    assert!(m.topology_findings().iter().any(|f| f.severity == Severity::Warning));
}

#[test]
fn cover_slope_and_length_findings() {
    // flat ground at 105.0; crown = invert + 0.6; outside top = crown + 0.05 wall
    // P1 start: invert 103.0 -> outside top 103.65 -> cover 1.35; end 102.5 -> top 103.15 -> cover 1.85
    // P2 end: invert 102.0 -> top 102.65 -> cover 2.35
    let (n, _, p) = chain();
    let g = flat(105.0);
    let rules = RuleSet { min_cover_m: Some(1.5), max_cover_m: Some(2.0), min_slope: Some(0.012), max_slope: Some(0.02), max_pipe_length_m: Some(40.0), ..RuleSet::default() };
    let cover = n.check_cover(&g, &rules, None);
    let c1: Vec<&Finding> = cover.iter().filter(|f| f.elements == vec![ElementRef::Pipe(p[0])]).collect();
    assert!(c1.iter().any(|f| f.message.contains("1.350") && f.message.contains("less than the minimum 1.500")), "{cover:?}");
    assert!(cover.iter().any(|f| f.elements == vec![ElementRef::Pipe(p[1])] && f.message.contains("2.350") && f.message.contains("more than the maximum")));
    let slope = n.check_slope(&rules);
    assert_eq!(slope.len(), 2); // 0.01 is below 0.012 on both
    assert!(slope[0].message.contains("less than the minimum"));
    let len = n.check_length(&rules);
    assert_eq!(len.len(), 2);
    assert!(len[0].message.contains("50.000"));
    // with no rules there is nothing to report
    assert!(n.check_cover(&g, &RuleSet::default(), None).is_empty());
    assert!(n.check_slope(&RuleSet::default()).is_empty());
    // a pipe that rises is always reported
    let mut m = n.clone();
    m.pipe_mut(p[0]).unwrap().end_invert_m = 103.5;
    assert!(m.check_slope(&RuleSet::default()).iter().any(|f| f.message.contains("rises")));
}

#[test]
fn cover_against_a_tin_and_off_the_surface() {
    let (n, _, _) = chain();
    let tin = afrobim_corridor::Tin::from_grid(E0 - 10.0, N0 - 10.0, 60.0, 20.0, 3, 2, |_, _| 105.0).unwrap();
    // grid covers E0-10 .. E0+110, N0-10 .. N0+10
    let rules = RuleSet { min_cover_m: Some(1.0), ..RuleSet::default() };
    assert!(n.check_cover(&tin, &rules, Some(10.0)).is_empty());
    let small = afrobim_corridor::Tin::from_grid(E0 - 10.0, N0 - 10.0, 30.0, 20.0, 2, 2, |_, _| 105.0).unwrap();
    let f = n.check_cover(&small, &rules, None);
    assert!(f.iter().any(|f| f.message.contains("off the ground surface")));
}

#[test]
fn apply_rules_works_downstream_to_upstream() {
    // flat ground 105.0, pipes D 0.6 with wall 0.05. Rules (test inputs): min cover 1.0, max slope 0.02,
    // min slope 0.01, match invert with a drop of 0.05.
    // P2 (MH2 -> MH3): end invert kept at 102.0. Shallowest start by cover: 105 - 1.0 - 0.05 - 0.6 = 103.35;
    //   slope (103.35 - 102.0)/50 = 0.027 > 0.02, so start = 102.0 + 0.02 * 50 = 103.0.
    // P1 (MH1 -> MH2): end matched to P2 start 103.0 plus the drop 0.05 = 103.05.
    //   start by cover 103.35; slope 0.3/50 = 0.006 < 0.01, so start = 103.05 + 0.01 * 50 = 103.55.
    //   Cover there: 105 - 103.55 - 0.65 = 0.80 m, below the minimum 1.0: reported.
    let (mut n, _, p) = chain();
    let g = flat(105.0);
    let rules = RuleSet { name: "test rules".into(), min_slope: Some(0.01), max_slope: Some(0.02), min_cover_m: Some(1.0), structure_drop_m: Some(0.05), match_mode: Some(MatchMode::Invert), ..RuleSet::default() };
    let findings = n.apply_rules(&rules, Some(&g)).unwrap();
    let p2 = n.pipe(p[1]).unwrap();
    assert!((p2.end_invert_m - 102.0).abs() < 1e-9);
    assert!((p2.start_invert_m - 103.0).abs() < 1e-8);
    let p1 = n.pipe(p[0]).unwrap();
    assert!((p1.end_invert_m - 103.05).abs() < 1e-8);
    assert!((p1.start_invert_m - 103.55).abs() < 1e-8);
    assert!(findings.iter().any(|f| f.check == "cover" && f.elements == vec![ElementRef::Pipe(p[0])] && f.message.contains("0.800")), "{findings:?}");
    // the drop rule now holds
    assert!(n.check_structure_drops(&rules).is_empty());
    // a drop without a match mode is refused
    let bad = RuleSet { structure_drop_m: Some(0.1), ..RuleSet::default() };
    assert!(n.apply_rules(&bad, None).is_err());
}

#[test]
fn crown_match_across_a_size_change() {
    // upstream 0.45 m pipe into a 0.60 m pipe, crowns matched with no drop.
    // upstream end invert 102.0 -> crown 102.45; downstream start crown must be 102.45 -> invert 101.85.
    let mut n = PipeNetwork::new("crown");
    let a = n.add_structure("A", StructureKind::Manhole, E0, N0, 106.0).unwrap();
    let b = n.add_structure("B", StructureKind::Manhole, E0 + 30.0, N0, 106.0).unwrap();
    let c = n.add_structure("C", StructureKind::Manhole, E0 + 60.0, N0, 106.0).unwrap();
    let p1 = n.connect(a, b, &spec(0.45), 102.3, 102.0, None).unwrap();
    let p2 = n.connect(b, c, &spec(0.6), 101.0, 100.7, None).unwrap();
    n.set_drop_across_structure(b, 0.0, MatchMode::Crown).unwrap();
    assert!((n.pipe(p2).unwrap().start_invert_m - 101.85).abs() < 1e-12);
    // centreline match: upstream centre 102.0 + 0.225 = 102.225; downstream invert = 102.225 - 0.3 = 101.925 less the drop 0.1
    n.set_drop_across_structure(b, 0.1, MatchMode::Centreline).unwrap();
    assert!((n.pipe(p2).unwrap().start_invert_m - 101.825).abs() < 1e-12);
    let _ = p1;
}

#[test]
fn inverts_by_slope_and_by_cover() {
    let (mut n, _, p) = chain();
    n.set_inverts_by_slope(p[0], PipeEnd::End, 100.0, 0.02).unwrap();
    // start = 100.0 + 0.02 * 50
    assert!((n.pipe(p[0]).unwrap().start_invert_m - 101.0).abs() < 1e-8);
    n.set_inverts_by_slope(p[0], PipeEnd::Start, 100.0, 0.02).unwrap();
    assert!((n.pipe(p[0]).unwrap().end_invert_m - 99.0).abs() < 1e-8);
    // cover 1.2 below flat ground 105: invert = 105 - 1.2 - 0.05 - 0.6 = 103.15 at both ends
    n.set_inverts_by_cover(p[1], &flat(105.0), 1.2).unwrap();
    assert!((n.pipe(p[1]).unwrap().start_invert_m - 103.15).abs() < 1e-12);
    assert!((n.pipe(p[1]).unwrap().end_invert_m - 103.15).abs() < 1e-12);
    let off = GroundOptFn(|_, _| None);
    assert!(n.set_inverts_by_cover(p[1], &off, 1.0).is_err());
}

#[test]
fn insert_move_delete_swap_connect_disconnect() {
    let (mut n, s, p) = chain();
    // insert a structure 10 m along the first pipe: the foot is at 10/50 = 0.2, invert 103.0 - 0.2 * 0.5 = 102.9
    let (m, p_new) = n.split_pipe(p[0], "NEW", StructureKind::Junction, E0 + 10.0, N0 + 3.0, 106.0).unwrap();
    assert_eq!(n.structure_count(), 4);
    assert!((n.pipe(p[0]).unwrap().end_invert_m - 102.9).abs() < 1e-9);
    assert!((n.pipe(p_new).unwrap().start_invert_m - 102.9).abs() < 1e-9);
    assert_eq!(n.pipe(p[0]).unwrap().end, m);
    assert_eq!(n.pipe(p_new).unwrap().end, s[1]);
    // move
    n.move_structure(m, E0 + 10.0, N0).unwrap();
    assert_eq!(n.position(m).unwrap(), [E0 + 10.0, N0]);
    // delete with merge: the two halves join back
    let out = n.delete_structure_merge(m).unwrap();
    assert_eq!(out.merged_into, Some(p[0]));
    assert_eq!(n.structure_count(), 3);
    assert_eq!(n.pipe(p[0]).unwrap().end, s[1]);
    assert!((n.pipe(p[0]).unwrap().end_invert_m - 102.5).abs() < 1e-9);
    // plain delete removes the attached pipes
    let out = n.delete_structure(s[1]).unwrap();
    assert_eq!(out.removed_pipes.len(), 2);
    assert_eq!(n.pipe_count(), 0);
    // connect and disconnect
    let q = n.connect(s[0], s[2], &spec(0.3), 100.0, 99.0, Some("direct")).unwrap();
    assert_eq!(n.pipe(q).unwrap().name, "direct");
    n.disconnect(q).unwrap();
    assert!(n.pipe(q).is_err());
    assert!(n.connect(s[0], s[0], &spec(0.3), 1.0, 0.0, None).is_err());
}

#[test]
fn swap_part_keeps_inverts() {
    let (mut n, _, p) = chain();
    let cat = PartsCatalogue::from_csv("example", include_str!("data/example_parts.csv")).unwrap();
    let before = n.pipe(p[0]).unwrap().start_invert_m;
    n.swap_pipe_part(p[0], cat.get("EXAMPLE-P2").unwrap()).unwrap();
    let pipe = n.pipe(p[0]).unwrap();
    assert_eq!(pipe.shape, PipeShape::Circular { diameter_m: 0.9 });
    assert_eq!(pipe.part.as_deref(), Some("EXAMPLE-P2"));
    assert_eq!(pipe.roughness, Some(Roughness::ColebrookWhite { ks_m: 0.0015 }));
    assert_eq!(pipe.wall_thickness_m, Some(0.08));
    assert_eq!(pipe.start_invert_m, before);
    // a structure part cannot be applied to a pipe
    assert!(n.swap_pipe_part(p[0], cat.get("EXAMPLE-S1").unwrap()).is_err());
    let s = n.structures().next().unwrap().id;
    n.swap_structure_part(s, cat.get("EXAMPLE-S2").unwrap()).unwrap();
    let st = n.structure(s).unwrap();
    assert_eq!(st.kind, StructureKind::CatchBasin);
    assert_eq!(st.inner, Some(StructureInner::Rectangular { length_m: 0.9, width_m: 0.6 }));
}

#[test]
fn hydraulic_checks_velocity_capacity_and_depth() {
    // D = 0.6, n = 0.013, S = 0.01: full Q = 0.614012 m3/s, V full = 2.171622 m/s
    let (mut n, s, p) = chain();
    n.structure_mut(s[0]).unwrap().inflow_m3_s = 0.1202426100; // depth ratio 0.3, V = 1.685472
    let rules = RuleSet { min_velocity_m_s: Some(1.8), max_velocity_m_s: Some(1.6), max_depth_ratio: Some(0.25), ..RuleSet::default() };
    let r = n.check_hydraulics(&rules, &HydraulicSettings::default());
    assert_eq!(r.pipes.len(), 2);
    let row = &r.pipes[0];
    assert!((row.flow.unwrap().depth_ratio - 0.3).abs() < 1e-5);
    assert!((row.capacity_ratio.unwrap() - 0.1958311881).abs() < 1e-6);
    assert!(row.critical_depth_m.unwrap() > 0.0);
    assert!(r.findings.iter().any(|f| f.check == "velocity" && f.message.contains("less than the minimum")));
    assert!(r.findings.iter().any(|f| f.check == "velocity" && f.message.contains("more than the maximum")));
    assert!(r.findings.iter().any(|f| f.check == "depth_ratio"));
    // over capacity
    n.structure_mut(s[0]).unwrap().inflow_m3_s = 0.7;
    let r = n.check_hydraulics(&RuleSet::default(), &HydraulicSettings::default());
    assert!(r.findings.iter().any(|f| f.check == "capacity" && f.message.contains("full-bore capacity")));
    // missing roughness is an error finding, not a panic
    n.pipe_mut(p[0]).unwrap().roughness = None;
    let r = n.check_hydraulics(&RuleSet::default(), &HydraulicSettings::default());
    assert!(r.findings.iter().any(|f| f.check == "data" && f.severity == Severity::Error));
    // a loop is reported through the hydraulics check
    let mut l = n.clone();
    l.connect(s[2], s[0], &spec(0.6), 90.0, 89.0, None).unwrap();
    assert!(l.check_hydraulics(&RuleSet::default(), &HydraulicSettings::default()).findings.iter().any(|f| f.message.contains("loop")));
}

#[test]
fn grade_lines_friction_only_hand_worked() {
    // MH1 -> MH2, 50 m, D = 0.6, n = 0.013, inverts 10.5 -> 10.0 (S = 0.01), Q = 0.3 m3/s from MH1,
    // tailwater 12.0 m, above the downstream crown 10.6, so the pipe is full and under pressure.
    // A = 0.2827433, V = Q / A = 1.061033 m/s, R = 0.15
    // Sf = (n V / R^(2/3))^2 = (0.013 * 1.061033 / 0.2823108)^2 = 0.002387203
    // L along the pipe = sqrt(50^2 + 0.5^2) = 50.0025;  hf = Sf L = 0.119366
    // HGL upstream = 12.0 + 0.119366 = 12.119366;  V^2/2g = 0.057380;  EGL = 12.176746
    let mut n = PipeNetwork::new("grade");
    let a = n.add_structure("MH1", StructureKind::Manhole, E0, N0, 14.0).unwrap();
    let b = n.add_structure("MH2", StructureKind::Outfall, E0 + 50.0, N0, 14.0).unwrap();
    let pid = n.connect(a, b, &spec(0.6), 10.5, 10.0, None).unwrap();
    n.structure_mut(a).unwrap().inflow_m3_s = 0.3;
    let r = n.grade_lines(12.0, &HydraulicSettings::default());
    assert!(r.friction_only);
    assert!(r.method.contains("friction only"));
    let pg = r.pipes.iter().find(|p| p.pipe == pid).unwrap();
    assert!(pg.pressure_flow);
    assert!((pg.velocity_m_s - 1.0610329539).abs() < 1e-8);
    assert!((pg.friction_loss_m - 0.1193661415).abs() < 1e-6, "{}", pg.friction_loss_m);
    assert!((pg.hgl_end_m - 12.0).abs() < 1e-12);
    assert!((pg.hgl_start_m - 12.1193661415).abs() < 1e-6);
    assert!((pg.egl_start_m - 12.1767460).abs() < 1e-6);
    let s1 = r.structures.iter().find(|s| s.structure == a).unwrap();
    assert!((s1.hgl_m - pg.hgl_start_m).abs() < 1e-12);
    assert!(!s1.above_rim);
    // with a loss coefficient K = 0.5 (test input) at MH1 the line rises by K V^2/2g = 0.5 * 0.057380 = 0.028690
    n.structure_mut(a).unwrap().loss_coefficient = Some(0.5);
    let r = n.grade_lines(12.0, &HydraulicSettings::default());
    assert!(!r.friction_only);
    let s1 = r.structures.iter().find(|s| s.structure == a).unwrap();
    assert!((s1.loss_m - 0.5 * 0.0573797619).abs() < 1e-6);
    // a rim below the grade line is flagged
    n.structure_mut(a).unwrap().rim_level_m = 12.0;
    let r = n.grade_lines(12.0, &HydraulicSettings::default());
    assert!(r.findings.iter().any(|f| f.check == "hgl" && f.message.contains("above its rim")));
}

#[test]
fn grade_line_with_free_surface_uses_the_uniform_depth() {
    // tailwater below the invert: the water surface is invert + normal depth at both ends.
    // Q = 0.1202426 m3/s in D = 0.6, S = 0.01, n = 0.013 gives y = 0.18 m.
    let mut n = PipeNetwork::new("free");
    let a = n.add_structure("MH1", StructureKind::Manhole, E0, N0, 14.0).unwrap();
    let b = n.add_structure("MH2", StructureKind::Outfall, E0 + 50.0, N0, 14.0).unwrap();
    let pid = n.connect(a, b, &spec(0.6), 10.5, 10.0, None).unwrap();
    n.structure_mut(a).unwrap().inflow_m3_s = 0.12024261;
    let r = n.grade_lines(9.0, &HydraulicSettings::default());
    let pg = r.pipes.iter().find(|p| p.pipe == pid).unwrap();
    assert!(!pg.pressure_flow);
    assert!((pg.hgl_start_m - 10.68).abs() < 1e-5);
    assert!((pg.hgl_end_m - 10.18).abs() < 1e-5);
}

fn straight(name: &str, a: (f64, f64), b: (f64, f64), z0: f64, z1: f64, d: f64) -> PipeNetwork {
    let mut n = PipeNetwork::new(name);
    let s0 = n.add_structure("a", StructureKind::Null, a.0, a.1, 30.0).unwrap();
    let s1 = n.add_structure("b", StructureKind::Null, b.0, b.1, 30.0).unwrap();
    let mut sp = spec(d);
    sp.wall_thickness_m = None;
    n.connect(s0, s1, &sp, z0, z1, None).unwrap();
    n
}

#[test]
fn interference_between_crossing_pipes_and_networks() {
    // A runs east at centre level 10.0 (invert 9.7, D 0.6, no wall, radius 0.3).
    // B runs north across it at centre level 11.0 (invert 10.7): axes 1.0 m apart, gap 0.4 m.
    let a = straight("A", (E0, N0), (E0 + 100.0, N0), 9.7, 9.7, 0.6);
    let b = straight("B", (E0 + 50.0, N0 - 50.0), (E0 + 50.0, N0 + 50.0), 10.7, 10.7, 0.6);
    assert!(a.check_interference_with(&b, 0.3).is_empty());
    let f = a.check_interference_with(&b, 0.5);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Warning);
    assert!(f[0].message.contains("0.400 m apart"), "{}", f[0].message);
    // overlap: B centre at 10.5 -> gap 0.5 - 0.6 = -0.1
    let b2 = straight("B2", (E0 + 50.0, N0 - 50.0), (E0 + 50.0, N0 + 50.0), 10.2, 10.2, 0.6);
    let f = a.check_interference_with(&b2, 0.1);
    assert_eq!(f[0].severity, Severity::Error);
    assert!(f[0].message.contains("overlap by 0.100"));
    // within one network: pipes that share a structure are not compared, a crossing pair is
    let mut n = PipeNetwork::new("one");
    let s: Vec<StructureId> = [(0.0, 0.0), (100.0, 0.0), (50.0, -50.0), (50.0, 50.0)].iter().map(|(x, y)| n.add_structure("", StructureKind::Null, E0 + x, N0 + y, 30.0).unwrap()).collect();
    let mut sp = spec(0.6);
    sp.wall_thickness_m = None;
    n.connect(s[0], s[1], &sp, 9.7, 9.7, None).unwrap();
    n.connect(s[2], s[3], &sp, 10.7, 10.7, None).unwrap();
    assert_eq!(n.check_interference(0.5).len(), 1);
    assert!(n.check_interference(0.3).is_empty());
    assert!(n.check_interference(-1.0).iter().any(|f| f.severity == Severity::Error));
}

#[test]
fn interference_pipe_to_structure() {
    // a pipe passes 1.5 m from the axis of a 1.2 m chamber (wall 0.1: radius 0.7): gap = 1.5 - 0.3 - 0.7 = 0.5
    let a = straight("A", (E0, N0), (E0 + 100.0, N0), 9.7, 9.7, 0.6);
    let mut b = PipeNetwork::new("B");
    let s = b.add_structure("MH", StructureKind::Manhole, E0 + 50.0, N0 + 1.5, 12.0).unwrap();
    let t = b.add_structure("T", StructureKind::Null, E0 + 50.0, N0 + 40.0, 12.0).unwrap();
    let mut sp = spec(0.3);
    sp.wall_thickness_m = None;
    b.connect(s, t, &sp, 8.0, 7.9, None).unwrap();
    {
        let st = b.structure_mut(s).unwrap();
        st.inner = Some(StructureInner::Circular { diameter_m: 1.2 });
        st.wall_thickness_m = Some(0.1);
        st.sump_level_m = Some(7.5);
    }
    // the pipe B itself (from the chamber) heads north away from A, so only the chamber matters;
    // pipe A at 10.0 is within the chamber's vertical extent 7.5 .. 12.0
    let f = a.check_interference_with(&b, 0.6);
    assert!(f.iter().any(|f| f.message.contains("pipe a") || f.message.contains("structure MH")), "{f:?}");
    let f = a.check_interference_with(&b, 0.45);
    assert!(!f.iter().any(|f| f.elements.contains(&ElementRef::Structure(s))), "{f:?}");
}
