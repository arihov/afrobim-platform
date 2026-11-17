//! Tests against closed-form cases. Coordinates are true UTM magnitudes.

use afrobim_corridor::Tin;
use afrobim_terrain::volume_between;

use crate::*;

const X0: f64 = 450_000.0;
const Y0: f64 = 9_960_000.0;

fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tol {tol})");
}

fn rel(a: f64, b: f64, r: f64) {
    assert!((a - b).abs() <= r * b.abs().max(1.0), "{a} vs {b} (rel {r})");
}

/// A planar ground `z = z0 + a (x - X0) + b (y - Y0)` over a large square around the origin.
fn plane(z0: f64, a: f64, b: f64) -> Tin {
    Tin::from_grid(X0 - 100.0, Y0 - 100.0, 10.0, 10.0, 31, 31, |x, y| z0 + a * (x - X0) + b * (y - Y0)).unwrap()
}

fn params(spacing: f64, facet_deg: f64) -> ProjectionParams {
    ProjectionParams { ray_spacing: spacing, facet_angle: facet_deg.to_radians() }
}

fn rect(w: f64, h: f64) -> Vec<[f64; 2]> {
    vec![[X0, Y0], [X0 + w, Y0], [X0 + w, Y0 + h], [X0, Y0 + h]]
}

// ------------------------------------------------------------------ feature lines

#[test]
fn stations_and_grade_table() {
    // 3-4-5 triangle legs: first segment 50 m long rising 2 m (4 %), second 50 m falling 1 m.
    let fl = FeatureLine::from_points("a", &[[X0, Y0, 100.0], [X0 + 30.0, Y0 + 40.0, 102.0], [X0 + 60.0, Y0 + 80.0, 101.0]]).unwrap();
    close(fl.length().unwrap(), 100.0, 1e-9);
    let t = fl.grade_table().unwrap();
    assert_eq!(t.len(), 2);
    close(t[0].plan_length, 50.0, 1e-9);
    close(t[0].grade_percent, 4.0, 1e-9);
    close(t[1].grade_percent, -2.0, 1e-9);
    close(t[0].slope_h_per_v.unwrap(), 25.0, 1e-9);
    close(fl.z_at(75.0).unwrap(), 101.5, 1e-9);
    let csv = grade_table_csv(&t);
    assert!(csv.starts_with("from,to,station_from"));
    assert_eq!(csv.lines().count(), 3);
}

#[test]
fn arc_span_quarter_circle() {
    // Centre (X0, Y0+10), radius 10, from (X0, Y0) counter-clockwise to (X0+10, Y0+10).
    let fl = FeatureLine::from_points("arc", &[[X0, Y0, 10.0], [X0 + 10.0, Y0 + 10.0, 20.0]]).unwrap().with_bulges(&[(std::f64::consts::FRAC_PI_2 / 4.0).tan()]).unwrap();
    let l = 10.0 * std::f64::consts::FRAC_PI_2;
    close(fl.length().unwrap(), l, 1e-9);
    let m = fl.xy_at(l / 2.0).unwrap();
    let c = 10.0 * std::f64::consts::FRAC_1_SQRT_2;
    close(m[0], X0 + c, 1e-6);
    close(m[1], Y0 + 10.0 - c, 1e-6);
    close(fl.z_at(l / 2.0).unwrap(), 15.0, 1e-9);
    // Reversed: clockwise arc, same midpoint.
    let mut r = fl.clone();
    r.reverse();
    close(r.length().unwrap(), l, 1e-9);
    let m2 = r.xy_at(l / 2.0).unwrap();
    close(m2[0], m[0], 1e-6);
    close(m2[1], m[1], 1e-6);
    r.reverse();
    assert_eq!(r.vertices.len(), fl.vertices.len());
    close(r.vertices[0].bulge, fl.vertices[0].bulge, 1e-12);
}

#[test]
fn edit_grade_elevation_points_and_quick_edit() {
    let mut fl = FeatureLine::from_points("e", &[[X0, Y0, 100.0], [X0 + 50.0, Y0, 100.0], [X0 + 100.0, Y0, 100.0]]).unwrap();
    let i = fl.insert_elevation_point(25.0, 101.0).unwrap();
    assert_eq!(i, 1);
    assert_eq!(fl.vertices[1].kind, VertexKind::Elevation);
    close(fl.z_at(12.5).unwrap(), 100.5, 1e-12);
    // Set 2 % from vertex 0 to vertex 2 (PI at 50 m), holding the start: end 101, middle re-interpolated.
    fl.set_grade(0, 2, 2.0, Hold::Start).unwrap();
    close(fl.vertices[2].z, 101.0, 1e-12);
    close(fl.vertices[1].z, 100.5, 1e-12);
    // Slope ratio 4:1 falling, holding the end.
    fl.set_slope_ratio(2, 3, 4.0, false, Hold::End).unwrap();
    close(fl.vertices[2].z, 100.0 + 12.5, 1e-9);
    let rows = fl.quick_edit(&[QuickEdit { vertex: 0, elevation: Some(90.0), grade_to_next_percent: Some(1.0) }]).unwrap();
    close(fl.vertices[0].z, 90.0, 1e-12);
    close(rows[0].grade_percent, 1.0, 1e-9);
    fl.raise_lower(1.0).unwrap();
    close(fl.vertices[0].z, 91.0, 1e-12);
    fl.delete_vertex(1).unwrap();
    assert_eq!(fl.vertices.len(), 3);
    // Insert a PI: the elevation points of the span are removed.
    let k = fl.insert_pi(0, X0 + 20.0, Y0 + 5.0, None).unwrap();
    assert_eq!(k, 1);
    assert_eq!(fl.pi_indices().len(), 4);
    fl.delete_vertex(1).unwrap();
    assert_eq!(fl.pi_indices().len(), 3);
    assert!(fl.delete_vertex(0).is_ok());
    assert!(fl.delete_vertex(0).is_err(), "two PIs must remain");
}

#[test]
fn fillet_right_angle() {
    let mut fl = FeatureLine::from_points("f", &[[X0, Y0, 100.0], [X0 + 50.0, Y0, 105.0], [X0 + 50.0, Y0 + 50.0, 110.0]]).unwrap();
    let rep = fl.fillet(1, 10.0).unwrap();
    close(rep.tangent_length, 10.0, 1e-9);
    close(rep.arc_length, 10.0 * std::f64::consts::FRAC_PI_2, 1e-9);
    close(fl.length().unwrap(), 40.0 + 15.707963267948966 + 40.0, 1e-8);
    assert_eq!(fl.pi_indices().len(), 4);
    // Left turn: counter-clockwise arc, positive bulge; tangent points at 40 m and 40 + arc.
    let a = fl.xy_at(40.0).unwrap();
    close(a[0], X0 + 40.0, 1e-9);
    close(fl.z_at(40.0).unwrap(), 104.0, 1e-9);
    assert!(fl.vertices[1].bulge > 0.0);
    // Too large a radius does not fit.
    let mut g = FeatureLine::from_points("g", &[[X0, Y0, 0.0], [X0 + 5.0, Y0, 0.0], [X0 + 5.0, Y0 + 5.0, 0.0]]).unwrap();
    assert!(g.fillet(1, 10.0).is_err());
}

#[test]
fn break_trim_join_reverse() {
    let fl = FeatureLine::from_points("l", &[[X0, Y0, 100.0], [X0 + 100.0, Y0, 110.0]]).unwrap();
    let parts = fl.break_at(30.0).unwrap();
    assert_eq!(parts.len(), 2);
    close(parts[0].length().unwrap(), 30.0, 1e-9);
    close(parts[1].length().unwrap(), 70.0, 1e-9);
    close(parts[1].vertices[0].z, 103.0, 1e-9);
    let j = parts[0].join(&parts[1], 1e-6).unwrap();
    close(j.line.length().unwrap(), 100.0, 1e-9);
    close(j.elevation_mismatch, 0.0, 1e-9);
    // Join with the second line reversed.
    let mut rev = parts[1].clone();
    rev.reverse();
    let j2 = parts[0].join(&rev, 1e-6).unwrap();
    close(j2.line.length().unwrap(), 100.0, 1e-9);
    assert!(parts[0].join(&FeatureLine::from_points("far", &[[X0 + 500.0, Y0, 0.0], [X0 + 600.0, Y0, 0.0]]).unwrap(), 1.0).is_err());
    let t = fl.trim(20.0, 60.0).unwrap();
    close(t.length().unwrap(), 40.0, 1e-9);
    close(t.vertices[0].z, 102.0, 1e-9);
    // A closed line broken at a station becomes one open line.
    let sq = FeatureLine::closed_from_points("sq", &[[X0, Y0, 0.0], [X0 + 10.0, Y0, 0.0], [X0 + 10.0, Y0 + 10.0, 0.0], [X0, Y0 + 10.0, 0.0]]).unwrap();
    let b = sq.break_at(15.0).unwrap();
    assert_eq!(b.len(), 1);
    close(b[0].length().unwrap(), 40.0, 1e-9);
    close(b[0].vertices[0].y, Y0 + 5.0, 1e-9);
}

#[test]
fn weed_removes_collinear_points_only() {
    let mut fl = FeatureLine::from_points("w", &[[X0, Y0, 100.0], [X0 + 10.0, Y0 + 0.001, 101.0], [X0 + 20.0, Y0, 102.0], [X0 + 30.0, Y0 + 5.0, 103.0]]).unwrap();
    let rep = fl.weed(WeedTolerances { plan_distance: 0.01, elevation: 0.01, max_span_length: 100.0 }).unwrap();
    assert_eq!(rep.pis_removed, 1);
    assert_eq!(fl.vertices.len(), 3);
    // An elevation tolerance that the bump violates keeps the point.
    let mut g = FeatureLine::from_points("w2", &[[X0, Y0, 100.0], [X0 + 10.0, Y0, 105.0], [X0 + 20.0, Y0, 100.0]]).unwrap();
    assert_eq!(g.weed(WeedTolerances { plan_distance: 0.01, elevation: 0.5, max_span_length: 100.0 }).unwrap().pis_removed, 0);
    // A span-length limit also keeps it.
    let mut h = FeatureLine::from_points("w3", &[[X0, Y0, 100.0], [X0 + 10.0, Y0, 100.0], [X0 + 20.0, Y0, 100.0]]).unwrap();
    assert_eq!(h.weed(WeedTolerances { plan_distance: 0.01, elevation: 0.5, max_span_length: 15.0 }).unwrap().pis_removed, 0);
    assert_eq!(h.weed(WeedTolerances { plan_distance: 0.01, elevation: 0.5, max_span_length: 25.0 }).unwrap().pis_removed, 1);
}

#[test]
fn drape_on_surface_adds_points_at_triangle_edges() {
    // z depends on x only: 100 + 0.1 |x - (X0 + 25)| at the nodes of a 10 m grid, so along a row the
    // ground is straight between nodes and the grade changes at x = 20 and 30 (not at 10 or 40).
    let ground = Tin::from_grid(X0 - 100.0, Y0 - 100.0, 10.0, 10.0, 31, 31, |x, _| 100.0 + 0.1 * (x - (X0 + 25.0)).abs()).unwrap();
    let (fl, rep) = FeatureLine::from_polyline_on_surface("d", &[[X0 + 3.0, Y0 + 2.0], [X0 + 47.0, Y0 + 2.0]], &ground, true).unwrap();
    assert_eq!(rep.points_added, 2, "kinks at 20 and 30 m");
    assert_eq!(fl.vertices.len(), 4);
    for v in &fl.vertices {
        close(v.z, ground.elevation_at(v.x, v.y).unwrap(), 1e-9);
    }
    close(fl.vertices[1].x, X0 + 20.0, 1e-6);
    // Without added points only the two ends are set.
    let (fl2, rep2) = FeatureLine::from_polyline_on_surface("d", &[[X0 + 3.0, Y0 + 2.0], [X0 + 47.0, Y0 + 2.0]], &ground, false).unwrap();
    assert_eq!((fl2.vertices.len(), rep2.points_added), (2, 0));
    // Outside the surface is an error.
    assert!(FeatureLine::from_polyline_on_surface("o", &[[X0 + 3.0, Y0], [X0 + 5000.0, Y0]], &ground, false).is_err());
}

#[test]
fn from_alignment_and_profile() {
    let plan = [[X0, Y0], [X0 + 100.0, Y0], [X0 + 200.0, Y0 + 100.0]];
    let st = [1000.0, 1100.0, 1241.4213562373095];
    let profile = [[1000.0, 50.0], [1050.0, 52.0], [1250.0, 48.0]];
    let fl = FeatureLine::from_alignment_profile("ap", &plan, &st, &profile).unwrap();
    assert_eq!(fl.vertices.len(), 4);
    assert_eq!(fl.vertices[1].kind, VertexKind::Elevation);
    close(fl.vertices[1].x, X0 + 50.0, 1e-9);
    close(fl.vertices[2].z, 52.0 - 4.0 * 50.0 / 200.0, 1e-9);
    assert!(FeatureLine::from_alignment_profile("bad", &plan, &st, &profile[..1]).is_err());
}

#[test]
fn template_sweep_ditch() {
    // A ditch template along a straight line: 2 m down at 3 m, flat 1 m, up again.
    let fl = FeatureLine::from_points("ditch", &[[X0, Y0, 100.0], [X0 + 50.0, Y0, 100.0]]).unwrap();
    let steps = [TemplateStep { offset: 3.0, dz: -1.0 }, TemplateStep { offset: 1.0, dz: 0.0 }, TemplateStep { offset: 3.0, dz: 1.0 }];
    let sw = sweep_template(&fl, None, Side::Left, &steps, &params(10.0, 15.0)).unwrap();
    assert_eq!(sw.lines.len(), 4);
    close(sw.lines[3][0][1], Y0 + 7.0, 1e-9);
    close(sw.lines[3][0][2], 100.0, 1e-9);
    close(sw.lines[1][0][2], 99.0, 1e-9);
    let area: f64 = sw.triangles.iter().map(|t| ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0])) / 2.0).sum();
    close(area, 7.0 * 50.0, 1e-6);
}

// ------------------------------------------------------------------ sites

#[test]
fn site_crossings_priority_then_last_edited() {
    let mut site = Site::new("s");
    let a = FeatureLine::from_points("a", &[[X0, Y0, 100.0], [X0 + 100.0, Y0, 102.0]]).unwrap();
    let b = FeatureLine::from_points("b", &[[X0 + 50.0, Y0 - 50.0, 90.0], [X0 + 50.0, Y0 + 50.0, 94.0]]).unwrap();
    site.add(a).unwrap();
    site.add(b).unwrap();
    // Equal priority: b was added (stamped) last, so b wins: z at the crossing = 92.
    let rep = site.resolve_crossings(0.1).unwrap();
    assert_eq!(rep.len(), 1);
    assert_eq!(rep[0].winner, "b");
    assert_eq!(rep[0].rule, PriorityRule::LastEdited);
    close(rep[0].z_a, 101.0, 1e-9);
    close(rep[0].z_b, 92.0, 1e-9);
    close(rep[0].x, X0 + 50.0, 1e-9);
    close(site.get("a").unwrap().z_at(50.0).unwrap(), 92.0, 1e-9);
    close(site.get("b").unwrap().z_at(50.0).unwrap(), 92.0, 1e-9);
    assert!(rep[0].changed);
    // A higher style priority beats the edit order.
    let mut site = Site::new("s2");
    site.add(FeatureLine::from_points("a", &[[X0, Y0, 100.0], [X0 + 100.0, Y0, 102.0]]).unwrap().with_priority(5)).unwrap();
    site.add(FeatureLine::from_points("b", &[[X0 + 50.0, Y0 - 50.0, 90.0], [X0 + 50.0, Y0 + 50.0, 94.0]]).unwrap()).unwrap();
    let rep = site.resolve_crossings(0.1).unwrap();
    assert_eq!(rep[0].winner, "a");
    assert_eq!(rep[0].rule, PriorityRule::StylePriority);
    close(site.get("b").unwrap().z_at(50.0).unwrap(), 101.0, 1e-9);
    assert!(crossings_csv(&rep).contains("StylePriority"));
    // Editing a line makes it the last edited.
    let mut site = Site::new("s3");
    site.add(FeatureLine::from_points("a", &[[X0, Y0, 100.0], [X0 + 100.0, Y0, 102.0]]).unwrap()).unwrap();
    site.add(FeatureLine::from_points("b", &[[X0 + 50.0, Y0 - 50.0, 90.0], [X0 + 50.0, Y0 + 50.0, 94.0]]).unwrap()).unwrap();
    site.edit("a", |l| l.raise_lower(0.5)).unwrap();
    let rep = site.resolve_crossings(0.1).unwrap();
    assert_eq!(rep[0].winner, "a");
    close(rep[0].z_used, 101.5, 1e-9);
}

#[test]
fn site_crossing_with_arc() {
    let mut site = Site::new("arc");
    // Quarter circle from (X0,Y0) to (X0+10,Y0+10), crossing the vertical line x = X0 + 10 * sin(45 deg).
    let arc = FeatureLine::from_points("arc", &[[X0, Y0, 0.0], [X0 + 10.0, Y0 + 10.0, 15.0]]).unwrap().with_bulges(&[(std::f64::consts::FRAC_PI_2 / 4.0).tan()]).unwrap();
    let c = 10.0 / 2f64.sqrt();
    site.add(arc).unwrap();
    site.add(FeatureLine::from_points("v", &[[X0 + c, Y0 - 5.0, 3.0], [X0 + c, Y0 + 20.0, 3.0]]).unwrap().with_priority(1)).unwrap();
    let rep = site.resolve_crossings(0.02).unwrap();
    assert_eq!(rep.len(), 1);
    close(rep[0].z_used, 3.0, 1e-9);
    // The arc now has an elevation point (or vertex) at the crossing station with z = 3.
    let s = rep[0].station_a;
    close(site.get("arc").unwrap().z_at(s).unwrap(), 3.0, 1e-6);
}

// ------------------------------------------------------------------ criteria

#[test]
fn slope_formats_agree_and_are_validated() {
    close(Slope::Ratio(2.0).rise_per_run().unwrap(), 0.5, 1e-15);
    close(Slope::GradePercent(50.0).rise_per_run().unwrap(), 0.5, 1e-15);
    assert!(Slope::Ratio(0.0).rise_per_run().is_err());
    assert!(Slope::GradePercent(-5.0).rise_per_run().is_err());
    assert!(Slope::Ratio(f64::NAN).rise_per_run().is_err());
    let p = Criterion::TargetDistance { distance: 8.0, slope: Slope::Ratio(2.0), direction: Direction::Fall };
    let q = Criterion::TargetDistance { distance: 8.0, slope: Slope::GradePercent(50.0), direction: Direction::Fall };
    let (a, b) = (p.daylight([X0, Y0], 100.0, [0.0, 1.0], 1.0, None).unwrap().unwrap(), q.daylight([X0, Y0], 100.0, [0.0, 1.0], 1.0, None).unwrap().unwrap());
    close(a.z, 96.0, 1e-12);
    assert_eq!(a, b);
    let e = Criterion::TargetElevation { elevation: 104.0, cut_slope: Slope::Ratio(2.0), fill_slope: Slope::Ratio(5.0) }.daylight([X0, Y0], 100.0, [1.0, 0.0], 1.0, None).unwrap().unwrap();
    close(e.t, 8.0, 1e-12);
    let e = Criterion::TargetElevation { elevation: 96.0, cut_slope: Slope::Ratio(2.0), fill_slope: Slope::Ratio(5.0) }.daylight([X0, Y0], 100.0, [1.0, 0.0], 1.0, None).unwrap().unwrap();
    close(e.t, 20.0, 1e-12);
    let r = Criterion::TargetRelativeElevation { delta: -3.0, slope: Slope::Ratio(4.0) }.daylight([X0, Y0], 100.0, [1.0, 0.0], 1.0, None).unwrap().unwrap();
    close(r.t, 12.0, 1e-12);
    close(r.z, 97.0, 1e-12);
    assert!(Criterion::TargetSurface { cut_slope: Slope::Ratio(2.0), fill_slope: Slope::Ratio(2.0), max_distance: 10.0 }.daylight([X0, Y0], 1.0, [1.0, 0.0], 1.0, None).is_err());
}

// ------------------------------------------------------------------ grading: closed forms

/// A rectangular pad `w x h` at level `level` on flat ground at 100, batters at the given ratio,
/// facet angle 10 degrees. Returns (build, ground).
fn flat_pad(w: f64, h: f64, level: f64, cut: f64, fill: f64) -> (GroupBuild, Tin) {
    let ground = plane(100.0, 0.0, 0.0);
    let pkg = building_pad("pad", &rect(w, h), &PlatformSurface::Level { level }, Slope::Ratio(cut), Slope::Ratio(fill), 100.0, params(10.0, 10.0)).unwrap();
    let b = pkg.build(Some(&ground)).unwrap();
    (b, ground)
}

/// Volume of the batters of a rectangular pad of height `d` on flat ground with slope ratio `f`
/// (horizontal per vertical): the batter reaches `r = d f` out.
///
/// * Box under the pad: `w h d`.
/// * Two long sides, each a triangular prism with section `r d / 2`: `2 * (r d / 2) w`.
/// * Two short sides: `2 * (r d / 2) h`.
/// * Four corners: the fan of a quarter cone. With `k` facets of angle `da = (pi / 2) / k` the fan is
///   `k` tetrahedra, each with a plan triangle `r^2 sin(da) / 2` and mean height `d / 3`.
fn pad_volume(w: f64, h: f64, d: f64, f: f64, k: usize) -> f64 {
    let r = d * f;
    let da = std::f64::consts::FRAC_PI_2 / k as f64;
    w * h * d + r * d * (w + h) + 4.0 * k as f64 * r * r * da.sin() / 2.0 * d / 3.0
}

#[test]
fn rectangular_pad_in_fill_gives_frustum_volumes() {
    // 40 x 30 pad, 5 m above flat ground, fill 3:1. Facet 10 degrees gives 9 facets per corner.
    let (b, ground) = flat_pad(40.0, 30.0, 105.0, 2.0, 3.0);
    assert!(b.gradings[0].complete);
    let v = b.volume_against(&ground, None).unwrap();
    let expected = pad_volume(40.0, 30.0, 5.0, 3.0, 9);
    rel(v.fill, expected, 1e-9);
    close(v.cut, 0.0, 1e-6);
    // The discretised fan is within 1 % of the exact cone: w h d + r d (w + h) + pi r^2 d / 3.
    let cone = 40.0 * 30.0 * 5.0 + 15.0 * 5.0 * 70.0 + std::f64::consts::PI * 225.0 * 5.0 / 3.0;
    assert!((v.fill - cone).abs() / cone < 0.01);
    // Daylight line: closed, plan distance 15 m from the pad edges.
    let dl = &b.gradings[0].daylight_lines[0];
    assert!(dl.closed);
    for p in &dl.points {
        assert!(close_to_rect(p[0], p[1], 15.0), "daylight point ({}, {}) is not 15 m from the pad", p[0], p[1]);
    }
}

fn close_to_rect(x: f64, y: f64, d: f64) -> bool {
    let (lx, ly) = ((x - X0).clamp(0.0, 40.0), (y - Y0).clamp(0.0, 30.0));
    (((x - X0) - lx).hypot((y - Y0) - ly) - d).abs() < 1e-6
}

#[test]
fn rectangular_pad_in_cut_uses_the_cut_slope() {
    // Pad 5 m below the ground, cut 2:1: reach 10 m.
    let (b, ground) = flat_pad(40.0, 30.0, 95.0, 2.0, 3.0);
    let v = b.volume_against(&ground, None).unwrap();
    rel(v.cut, pad_volume(40.0, 30.0, 5.0, 2.0, 9), 1e-9);
    close(v.fill, 0.0, 1e-6);
    close(v.net, v.cut, 1e-9);
}

#[test]
fn pad_on_planar_ground_with_straight_edge_matches_the_integral() {
    // One straight edge of length L = 60 along +x at 105 m, grading to the left (+y) in fill 3:1 on
    // ground z = 100 + a (x - X0) + b (y - Y0). Along the ray the ground rises at b, the batter falls
    // at 1/F, so the batter meets the ground at t = h / (1/F + b) with h(s) = 105 - 100 - a s. The
    // section is a triangle of area t h / 2, so
    //   V = integral of h^2 / (2 (1/F + b)) ds = L (h1^2 + h1 h2 + h2^2) / (6 (1/F + b)),
    // since h is linear in s from h1 to h2.
    let (a, bb, l, f) = (0.01, 0.02, 60.0, 3.0);
    let ground = plane(100.0, a, bb);
    let edge = FeatureLine::from_points("edge", &[[X0, Y0, 105.0], [X0 + l, Y0, 105.0]]).unwrap();
    let pkg = layby_tie_in("lb", &edge, Side::Left, None, Slope::Ratio(2.0), Slope::Ratio(f), 100.0, params(7.0, 10.0)).unwrap();
    let b = pkg.build(Some(&ground)).unwrap();
    let (h1, h2) = (5.0, 5.0 - a * l);
    let expected = l * (h1 * h1 + h1 * h2 + h2 * h2) / (6.0 * (1.0 / f + bb));
    let v = b.volume_against(&ground, None).unwrap();
    rel(v.fill, expected, 1e-9);
}

#[test]
fn ground_above_the_footprint_switches_to_the_cut_slope_per_ray() {
    // Ground rises along x from 98 to 104 over 60 m; the footprint is level at 101: the batter is
    // fill where the ground is below 101 and cut where above. At the crossing the ray has zero length.
    let ground = plane(98.0, 0.1, 0.0);
    let edge = FeatureLine::from_points("e", &[[X0, Y0, 101.0], [X0 + 60.0, Y0, 101.0]]).unwrap();
    let g = Grading::uniform("g", "e", Side::Left, Criterion::TargetSurface { cut_slope: Slope::Ratio(2.0), fill_slope: Slope::Ratio(3.0), max_distance: 50.0 });
    let r = g.project(&edge, Some(&ground), &params(5.0, 10.0)).unwrap();
    let first = &r.rays[0];
    close(first.distance.unwrap(), 9.0, 1e-9); // fill: h = 3, t = 3 * 3
    let last = r.rays.last().unwrap();
    close(last.distance.unwrap(), 6.0, 1e-9); // cut: h = 3, t = 3 * 2
    assert!(last.daylight.unwrap()[2] > 101.0);
    assert!(r.rays.iter().any(|q| q.distance.unwrap() < 1e-6), "the level footprint meets the ground at x = 30");
}

#[test]
fn no_daylight_is_reported_not_hidden() {
    let ground = plane(100.0, 0.0, 0.0);
    let edge = FeatureLine::from_points("e", &[[X0, Y0, 105.0], [X0 + 20.0, Y0, 105.0]]).unwrap();
    // Max distance 5 m but 15 m are needed.
    let g = Grading::uniform("g", "e", Side::Left, Criterion::TargetSurface { cut_slope: Slope::Ratio(3.0), fill_slope: Slope::Ratio(3.0), max_distance: 5.0 });
    let r = g.project(&edge, Some(&ground), &params(5.0, 10.0)).unwrap();
    assert!(!r.complete);
    assert!(r.triangles.is_empty());
    assert!(!r.warnings.is_empty());
    assert!(r.rays.iter().all(|q| q.status == RayStatus::NoDaylight));
}

#[test]
fn transitions_blend_distance_and_elevation() {
    let edge = FeatureLine::from_points("e", &[[X0, Y0, 100.0], [X0 + 40.0, Y0, 100.0]]).unwrap();
    let d = |dist: f64| Criterion::TargetDistance { distance: dist, slope: Slope::Ratio(2.0), direction: Direction::Fall };
    let g = Grading::uniform("g", "e", Side::Left, d(5.0)).with_regions(vec![
        Region { from: Some(0.0), to: Some(10.0), kind: RegionKind::Uniform { criterion: d(5.0) } },
        Region { from: Some(10.0), to: Some(30.0), kind: RegionKind::Transition { start: d(5.0), end: d(15.0) } },
        Region { from: Some(30.0), to: Some(40.0), kind: RegionKind::Uniform { criterion: d(15.0) } },
    ]);
    let r = g.project(&edge, None, &params(5.0, 10.0)).unwrap();
    let at = |s: f64| r.rays.iter().find(|q| (q.station - s).abs() < 1e-9).unwrap();
    close(at(0.0).distance.unwrap(), 5.0, 1e-12);
    close(at(20.0).distance.unwrap(), 10.0, 1e-12);
    close(at(20.0).daylight.unwrap()[2], 100.0 - 5.0, 1e-12);
    close(at(40.0).distance.unwrap(), 15.0, 1e-12);
    // Boundary nodes are emitted once per region, so there are two rays at station 10.
    assert_eq!(r.rays.iter().filter(|q| (q.station - 10.0).abs() < 1e-9).count(), 2);
    assert!(r.complete);
    // A region gap is an error.
    let gap = Grading::uniform("g", "e", Side::Left, d(5.0)).with_regions(vec![Region { from: Some(0.0), to: Some(10.0), kind: RegionKind::Uniform { criterion: d(5.0) } }]);
    assert!(gap.project(&edge, None, &params(5.0, 10.0)).is_err());
}

#[test]
fn station_range_projects_part_of_the_footprint() {
    let edge = FeatureLine::from_points("e", &[[X0, Y0, 100.0], [X0 + 100.0, Y0, 100.0]]).unwrap();
    let g = Grading::uniform("g", "e", Side::Right, Criterion::TargetDistance { distance: 4.0, slope: Slope::Ratio(2.0), direction: Direction::Rise }).with_range(20.0, 50.0);
    let r = g.project(&edge, None, &params(10.0, 10.0)).unwrap();
    close(r.rays[0].foot[0], X0 + 20.0, 1e-9);
    close(r.rays.last().unwrap().foot[0], X0 + 50.0, 1e-9);
    // Right of +x is -y; the daylight rises 2 m.
    close(r.rays[0].daylight.unwrap()[1], Y0 - 4.0, 1e-9);
    close(r.rays[0].daylight.unwrap()[2], 102.0, 1e-9);
    let area: f64 = r.triangles.iter().map(|t| ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0])) / 2.0).sum();
    close(area, 30.0 * 4.0, 1e-6);
}

#[test]
fn arc_footprint_gives_concentric_daylight() {
    // A quarter circle of radius 20, graded outward (right of a counter-clockwise arc) 6 m at 2:1 fall.
    let arc = FeatureLine::from_points("arc", &[[X0 + 20.0, Y0, 100.0], [X0, Y0 + 20.0, 100.0]]).unwrap().with_bulges(&[(std::f64::consts::FRAC_PI_2 / 4.0).tan()]).unwrap();
    let g = Grading::uniform("g", "arc", Side::Right, Criterion::TargetDistance { distance: 6.0, slope: Slope::Ratio(2.0), direction: Direction::Fall });
    let r = g.project(&arc, None, &params(5.0, 5.0)).unwrap();
    for q in &r.rays {
        let d = q.daylight.unwrap();
        close((d[0] - X0).hypot(d[1] - Y0), 26.0, 1e-6);
        close((q.foot[0] - X0).hypot(q.foot[1] - Y0), 20.0, 1e-6);
    }
    assert!(r.rays.len() >= 19, "5 degree facets over 90 degrees");
}

#[test]
fn interior_corner_loops_are_removed() {
    // A U-shaped footprint with a narrow notch (2 m wide, 10 m deep); grading into the notch 5 m
    // makes the daylight lines of the two walls cross. The clean-up must trim them.
    let u = FeatureLine::closed_from_points(
        "u",
        &[[X0, Y0, 100.0], [X0 + 20.0, Y0, 100.0], [X0 + 20.0, Y0 + 20.0, 100.0], [X0 + 11.0, Y0 + 20.0, 100.0], [X0 + 11.0, Y0 + 10.0, 100.0], [X0 + 9.0, Y0 + 10.0, 100.0], [X0 + 9.0, Y0 + 20.0, 100.0], [X0, Y0 + 20.0, 100.0]],
    )
    .unwrap();
    let g = Grading::uniform("g", "u", Side::Right, Criterion::TargetDistance { distance: 5.0, slope: Slope::Ratio(2.0), direction: Direction::Fall });
    let r = g.project(&u, None, &params(5.0, 15.0)).unwrap();
    assert!(r.loops_removed > 0, "the notch daylight lines cross");
    assert!(r.rays.iter().any(|q| q.status == RayStatus::Trimmed));
    // Every face is counter-clockwise with plan area.
    for t in &r.triangles {
        assert!((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]) > 0.0);
    }
}

// ------------------------------------------------------------------ pond, ditch, ticks

#[test]
fn pond_matches_the_prismatoid_and_stage_storage() {
    // Square rim 20 x 20 at 100, floor 97, inside slope 2:1: floor side 20 - 2 * 6 = 8 and
    // V(h) = depth / 3 (A1 + A2 + sqrt(A1 A2)) = 3 / 3 (400 + 64 + 160) = 624 at the rim.
    // At level 98.5 (depth 1.5): side 8 + 2 * 2 * 1.5 = 14, V = 1.5 / 3 (64 + 196 + 112) = 186.
    let rim = FeatureLine::closed_from_points("rim", &rect(20.0, 20.0).iter().map(|p| [p[0], p[1], 100.0]).collect::<Vec<_>>()).unwrap();
    let pkg = pond("pond", &rim, 97.0, Slope::Ratio(2.0), params(5.0, 10.0)).unwrap();
    let b = pkg.build(None).unwrap();
    assert!(b.gradings[0].complete);
    let region = rim_region(&pkg).unwrap();
    let st = b.stage_storage(Some(&region), &[98.5, 100.0]).unwrap();
    rel(st[0].volume, 186.0, 1e-9);
    rel(st[1].volume, 624.0, 1e-9);
    close(st[0].water_area, 14.0 * 14.0, 1e-6);
    // Against a flat ground at the rim level the cut is the pond volume.
    let ground = plane(100.0, 0.0, 0.0);
    let v = b.volume_against(&ground, None).unwrap();
    rel(v.cut, 624.0, 1e-9);
    assert!(stage_storage_csv(&st).lines().count() == 3);
    // The floor above the rim is refused.
    assert!(pond("bad", &rim, 101.0, Slope::Ratio(2.0), params(5.0, 10.0)).is_err());
}

#[test]
fn slope_ticks_alternate_long_and_short() {
    let edge = FeatureLine::from_points("e", &[[X0, Y0, 100.0], [X0 + 20.0, Y0, 100.0]]).unwrap();
    let g = Grading::uniform("g", "e", Side::Left, Criterion::TargetDistance { distance: 6.0, slope: Slope::Ratio(2.0), direction: Direction::Fall });
    let r = g.project(&edge, None, &params(10.0, 10.0)).unwrap();
    let t = slope_ticks(&r, 4.0, 0.5).unwrap();
    // Ticks every 2 m from 0 to 20: 11 ticks.
    assert_eq!(t.len(), 11);
    assert!(t[0].long && !t[1].long && t[2].long);
    close(t[0].to[1] - t[0].from[1], 6.0, 1e-9);
    close(t[1].to[1] - t[1].from[1], 3.0, 1e-9);
    close(t[1].from[0], X0 + 2.0, 1e-9);
    assert!(slope_ticks(&r, 4.0, 1.5).is_err());
}

// ------------------------------------------------------------------ groups

#[test]
fn rebuild_data_reports_what_changed() {
    let ground = plane(100.0, 0.0, 0.0);
    let pkg = building_pad("pad", &rect(40.0, 30.0), &PlatformSurface::Level { level: 105.0 }, Slope::Ratio(2.0), Slope::Ratio(3.0), 100.0, params(10.0, 10.0)).unwrap();
    let b = pkg.build(Some(&ground)).unwrap();
    assert!(pkg.group.stale_reasons(&b.rebuild, &pkg.site, Some(&ground)).is_empty());
    let mut site2 = pkg.site.clone();
    site2.edit("pad-footprint", |l| l.raise_lower(0.5)).unwrap();
    assert_eq!(pkg.group.stale_reasons(&b.rebuild, &site2, Some(&ground)), vec![StaleReason::FootprintChanged("pad-footprint".into())]);
    let ground2 = plane(100.5, 0.0, 0.0);
    assert_eq!(pkg.group.stale_reasons(&b.rebuild, &pkg.site, Some(&ground2)), vec![StaleReason::GroundChanged]);
    let mut g2 = pkg.group.clone();
    g2.raise_lower(1.0).unwrap();
    assert_eq!(g2.stale_reasons(&b.rebuild, &pkg.site, Some(&ground)), vec![StaleReason::SettingsChanged]);
}

#[test]
fn raise_lower_moves_the_volumes() {
    let ground = plane(100.0, 0.0, 0.0);
    let mut pkg = building_pad("pad", &rect(40.0, 30.0), &PlatformSurface::Level { level: 105.0 }, Slope::Ratio(2.0), Slope::Ratio(3.0), 100.0, params(10.0, 10.0)).unwrap();
    pkg.group.raise_lower(-2.0).unwrap();
    let v = pkg.build(Some(&ground)).unwrap().volume_against(&ground, None).unwrap();
    rel(v.fill, pad_volume(40.0, 30.0, 3.0, 3.0, 9), 1e-9);
}

#[test]
fn auto_balance_finds_the_level_where_cut_equals_fill() {
    // Ground tilted 2 % along x about the pad centre, fill and cut slopes equal. The set-up is
    // symmetric under a half turn about the pad centre combined with z -> 200 - z, so the net is
    // exactly zero at the pad level 100 at the centre.
    let cx = X0 + 20.0;
    let ground = Tin::from_grid(X0 - 100.0, Y0 - 100.0, 10.0, 10.0, 31, 31, |x, _| 100.0 + 0.02 * (x - cx)).unwrap();
    let mut pkg = building_pad("pad", &rect(40.0, 30.0), &PlatformSurface::Level { level: 103.0 }, Slope::Ratio(2.0), Slope::Ratio(2.0), 200.0, params(10.0, 10.0)).unwrap();
    let opts = BalanceOptions { target_net: 0.0, factors: ShrinkSwell { cut_factor: 1.0, fill_factor: 1.0 }, min_shift: -6.0, max_shift: 0.0, tolerance: 0.01, max_iterations: 30 };
    let res = pkg.group.auto_balance(&pkg.site, Some(&ground), &ground, None, &opts).unwrap();
    assert!(res.converged);
    close(res.shift, -3.0, 1e-4);
    assert!(res.net.abs() <= 0.01);
    assert!(res.iterations > 0 && res.iterations < 30, "{} iterations", res.iterations);
    assert_eq!(res.history.len(), res.iterations + 2);
    close(pkg.group.shift, res.shift, 0.0);
    assert!(balance_csv(&res).lines().count() == res.history.len() + 1);
    // An interval that does not bracket the target fails and says so.
    let mut g2 = pkg.group.clone();
    g2.shift = 0.0;
    let bad = BalanceOptions { min_shift: 1.0, max_shift: 2.0, ..opts };
    assert!(matches!(g2.auto_balance(&pkg.site, Some(&ground), &ground, None, &bad), Err(GradingError::NotBracketed { .. })));
    // With shrink and swell factors the balance moves: fill demands 1.25 per unit, so less fill is wanted.
    let swell = BalanceOptions { factors: ShrinkSwell { cut_factor: 1.0, fill_factor: 1.25 }, ..opts };
    let mut g3 = pkg.group.clone();
    g3.shift = 0.0;
    let r3 = g3.auto_balance(&pkg.site, Some(&ground), &ground, None, &swell).unwrap();
    assert!(r3.shift < res.shift, "a bulkier fill needs a lower pad ({} vs {})", r3.shift, res.shift);
}

#[test]
fn group_definition_rebuilds_the_same_surface() {
    let (b, ground) = flat_pad(40.0, 30.0, 105.0, 2.0, 3.0);
    let d = b.to_definition();
    assert!(d.notes.is_empty(), "{:?}", d.notes);
    let rebuilt = d.definition.build().unwrap();
    let v1 = volume_between(&ground, &b.tin, None).unwrap();
    let v2 = volume_between(&ground, &rebuilt.tin, None).unwrap();
    rel(v2.fill, v1.fill, 1e-9);
    close(rebuilt.tin.plan_area(), b.tin.plan_area(), 1e-6);
}

#[test]
fn outputs_serialize_and_csv() {
    let (b, _) = flat_pad(40.0, 30.0, 105.0, 2.0, 3.0);
    let json = serde_json::to_string(&b).unwrap();
    let back: GroupBuild = serde_json::from_str(&json).unwrap();
    assert_eq!(back, b);
    let pkg = building_pad("pad", &rect(40.0, 30.0), &PlatformSurface::Level { level: 105.0 }, Slope::Ratio(2.0), Slope::Ratio(3.0), 100.0, params(10.0, 10.0)).unwrap();
    let j = serde_json::to_string(&pkg).unwrap();
    let p2: DesignPackage = serde_json::from_str(&j).unwrap();
    assert_eq!(p2, pkg);
    let geo = GroupGeometry::of(&b);
    assert_eq!(geo.triangles.len(), b.tin.triangles().len());
    assert!(rays_csv(&b.gradings[0]).lines().count() > 10);
    let v = b.volume_against(&plane(100.0, 0.0, 0.0), None).unwrap();
    assert!(volume_csv(&v).starts_with("cut,fill,net"));
    let fl = FeatureLine::from_points("f", &[[X0, Y0, 1.0], [X0 + 5.0, Y0, 2.0]]).unwrap();
    let back: FeatureLine = serde_json::from_str(&serde_json::to_string(&fl).unwrap()).unwrap();
    assert_eq!(back, fl);
    let site = Site::new("s");
    assert_eq!(serde_json::from_str::<Site>(&serde_json::to_string(&site).unwrap()).unwrap(), site);
}

#[test]
fn exterior_fans_respect_the_facet_angle() {
    let fl = FeatureLine::closed_from_points("sq", &rect(10.0, 10.0).iter().map(|p| [p[0], p[1], 100.0]).collect::<Vec<_>>()).unwrap();
    // Outward of a counter-clockwise ring is the right side: exterior corners, fans.
    let set = ray_geometry(&fl, None, Side::Right, &params(100.0, 30.0), &[]).unwrap();
    // 90 degrees at 30 degrees per ray: 3 steps, 4 rays per corner.
    assert_eq!(set.rays.len(), 16);
    assert!(set.ring);
    assert!(set.rays.iter().all(|r| r.kind == RayKind::Fan && r.miter == 1.0));
    // Inward: interior corners, one miter ray each, with factor sqrt 2.
    let set = ray_geometry(&fl, None, Side::Left, &params(100.0, 30.0), &[]).unwrap();
    assert_eq!(set.rays.len(), 4);
    close(set.rays[0].miter, 2f64.sqrt(), 1e-12);
    close(set.rays[0].dir[0], std::f64::consts::FRAC_1_SQRT_2, 1e-12);
    assert!(ray_geometry(&fl, None, Side::Left, &ProjectionParams { ray_spacing: 0.0, facet_angle: 0.1 }, &[]).is_err());
}

#[test]
fn l_shaped_pad_has_miter_at_the_reflex_corner() {
    // L-shape (counter-clockwise): 40 x 20 plus 20 x 20, area 1200, perimeter 160, five convex
    // corners and one reflex corner. Fill 3:1, 5 m high, so the batter reaches r = 15 m.
    // On flat ground the surface beside a wall is d (1 - y / r), and in the r x r square outside
    // the reflex corner it is the higher of the two walls' surfaces, d (1 - min(x, y) / r), whose
    // integral is (2/3) d r^2, whereas the two prisms (each d r^2 / 2 over the square) would count
    // d r^2. Hence
    //   V = A d + P r d / 2 + 5 cone_fan - d r^2 / 3,
    // with cone_fan = k (r^2 sin(da) / 2) (d / 3) for the discretised quarter cone (k = 9, da = 10 degrees).
    let outline = [[X0, Y0], [X0 + 40.0, Y0], [X0 + 40.0, Y0 + 20.0], [X0 + 20.0, Y0 + 20.0], [X0 + 20.0, Y0 + 40.0], [X0, Y0 + 40.0]];
    let ground = plane(100.0, 0.0, 0.0);
    let pkg = building_pad("l", &outline, &PlatformSurface::Level { level: 105.0 }, Slope::Ratio(2.0), Slope::Ratio(3.0), 100.0, params(10.0, 10.0)).unwrap();
    let b = pkg.build(Some(&ground)).unwrap();
    assert!(b.gradings[0].complete);
    assert!(b.gradings[0].rays_dropped > 0, "rays beside the reflex corner overshoot its miter ray");
    let (d, r) = (5.0, 15.0);
    let fan = 9.0 * r * r * 10f64.to_radians().sin() / 2.0 * d / 3.0;
    let expected = 1200.0 * d + 160.0 * r * d / 2.0 + 5.0 * fan - d * r * r / 3.0;
    let v = b.volume_against(&ground, None).unwrap();
    rel(v.fill, expected, 1e-9);
}

#[test]
fn stepped_offset_of_a_feature_line() {
    // Straight line along +x at 100 m, offset 3 m to the left with a 0.5 m rise.
    let fl = FeatureLine::from_points("s", &[[X0, Y0, 100.0], [X0 + 50.0, Y0, 102.0]]).unwrap();
    let o = fl.offset_stepped(3.0, ElevationStep::Difference(0.5), 0.1).unwrap();
    close(o.vertices[0].y, Y0 + 3.0, 1e-9);
    close(o.vertices[1].z, 102.5, 1e-9);
    // Right offset with a grade: 2 % over 4 m = 0.08 m.
    let o = fl.offset_stepped(-4.0, ElevationStep::GradePercent(2.0), 0.1).unwrap();
    close(o.vertices[0].y, Y0 - 4.0, 1e-9);
    close(o.vertices[0].z, 100.08, 1e-9);
    // A closed 10 x 10 ring offset 2 m inside (left of a counter-clockwise ring): an 6 x 6 square.
    let ring = FeatureLine::closed_from_points("r", &rect(10.0, 10.0).iter().map(|p| [p[0], p[1], 100.0]).collect::<Vec<_>>()).unwrap();
    let i = ring.offset_stepped(2.0, ElevationStep::Difference(-1.0), 0.1).unwrap();
    close(i.length().unwrap(), 24.0, 1e-9);
    close(i.vertices[0].x, X0 + 2.0, 1e-9);
    close(i.vertices[0].z, 99.0, 1e-12);
    // Outside: 14 x 14 with mitered corners.
    let e = ring.offset_stepped(-2.0, ElevationStep::Difference(0.0), 0.1).unwrap();
    close(e.length().unwrap(), 56.0, 1e-9);
    // An arc is flattened: the offset of a quarter circle (radius 10) by 2 m has length close to the
    // arc of radius 8 (inside, counter-clockwise, left) and no longer than it.
    let arc = FeatureLine::from_points("a", &[[X0, Y0, 0.0], [X0 + 10.0, Y0 + 10.0, 0.0]]).unwrap().with_bulges(&[(std::f64::consts::FRAC_PI_2 / 4.0).tan()]).unwrap();
    let ao = arc.offset_stepped(2.0, ElevationStep::Difference(0.0), 0.02).unwrap();
    let want = 8.0 * std::f64::consts::FRAC_PI_2;
    assert!((ao.length().unwrap() - want).abs() < want * 1e-4, "{} vs {}", ao.length().unwrap(), want);
}
