//! End to end through the public API: points to surface to contours, analysis and volumes, at
//! survey magnitudes.

use afrobim_terrain::*;

const OX: f64 = 512_345.0;
const OY: f64 = 9_876_543.0;

/// Deterministic pseudo-random numbers in [0, 1).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

fn scattered(n: usize, seed: u64, z: impl Fn(f64, f64) -> f64) -> Vec<SurfacePoint> {
    let mut r = Lcg(seed);
    let mut v = vec![
        SurfacePoint::new("c0", OX, OY, z(0.0, 0.0)),
        SurfacePoint::new("c1", OX + 1000.0, OY, z(1000.0, 0.0)),
        SurfacePoint::new("c2", OX + 1000.0, OY + 1000.0, z(1000.0, 1000.0)),
        SurfacePoint::new("c3", OX, OY + 1000.0, z(0.0, 1000.0)),
    ];
    for i in 0..n {
        let (x, y) = (r.next() * 1000.0, r.next() * 1000.0);
        v.push(SurfacePoint::new(format!("p{i}"), OX + x, OY + y, z(x, y)));
    }
    v
}

#[test]
fn scattered_survey_to_volumes() {
    let eg = SurfaceDefinition::new().with(Operation::AddPoints { group: "survey".into(), points: scattered(5000, 1, |x, y| 100.0 + 0.02 * x + 0.01 * y) }).build().unwrap();
    assert_eq!(eg.report.duplicates_merged, 0);
    let area = eg.tin.plan_area();
    assert!((area - 1.0e6).abs() < 1e-3, "{area}");
    // Euler: a triangulation of n points with h on the hull has 2n - 2 - h triangles.
    assert!(eg.report.triangles >= 2 * 5004 - 2 - 4 - 400);

    let design = SurfaceDefinition::new().with(Operation::AddPoints { group: "design".into(), points: scattered(300, 2, |x, y| 105.0 + 0.0 * x + 0.0 * y) }).build().unwrap();
    // Existing is a plane, design is flat at 105: d = 100 + 0.02 x + 0.01 y - 105.
    let v = volume_between(&eg.tin, &design.tin, None).unwrap();
    // Net = area * (value at centroid) = 1e6 * (100 + 10 + 5 - 105) = 1e7.
    assert!((v.net - 1.0e7).abs() < 1e-4 * 1.0e7 * 1e-3, "{}", v.net);
    assert!(v.cut > 0.0 && v.fill > 0.0);
    assert!((v.overlap_area - 1.0e6).abs() < 1e-3);
    let g = volume_between_grid(&eg.tin, &design.tin, 5.0, None).unwrap();
    assert!((g.net - v.net).abs() / v.net.abs() < 5e-3, "grid {} exact {}", g.net, v.net);

    // Contours of the plane: x = (level - 100 - 0.01 y) / 0.02, straight and open.
    let set = contours(&eg.tin, &ContourOptions::new(1.0, Some(5.0))).unwrap();
    assert!(set.contours.iter().all(|c| !c.closed));
    let major = set.of_kind(ContourKind::Major).count();
    assert!(major > 0 && major < set.contours.len());

    // Slope of the plane everywhere.
    let g = (0.02f64 * 0.02 + 0.01 * 0.01).sqrt();
    assert!(triangle_slopes(&eg.tin).iter().all(|t| (t.slope_percent - 100.0 * g).abs() < 1e-6));
    let f = flow_path(&eg.tin, OX + 700.0, OY + 300.0);
    assert_eq!(f.stop, FlowStop::LeftSurface);
}

#[test]
fn surface_object_round_trips_and_rebuilds() {
    let mut s = Surface::new("Existing ground, Block A", SurfaceRole::ExistingGround);
    s.source = SourceRecord { description: "total station survey".into(), reference: "SV-001".into(), date: "2026-11-01".into(), coordinate_system: "UTM 36S, MSL".into(), ..Default::default() };
    s.status = SurfaceStatus::ForReview;
    s.definition = SurfaceDefinition::new()
        .with(Operation::AddPoints { group: "g1".into(), points: scattered(200, 3, |x, y| 50.0 + 0.01 * (x + y)) })
        .with(Operation::AddBreaklines { lines: vec![Breakline { name: "kerb".into(), kind: BreaklineKind::Standard, vertices: vec![[OX + 100.0, OY + 100.0, 52.0], [OX + 900.0, OY + 120.0, 60.0]] }] })
        .with(Operation::MaxTriangleEdge { length: 400.0 });
    let text = s.to_json().unwrap();
    assert!(text.contains("\"existing_ground\"") && text.contains("\"for_review\""));
    let again = Surface::from_json(&text).unwrap();
    let (a, b) = (s.build().unwrap(), again.build().unwrap());
    assert_eq!(a.tin, b.tin);
    // Rebuilding is deterministic.
    assert_eq!(a.tin, s.build().unwrap().tin);
    assert_eq!(a.tin.elevation_at(OX + 500.0, OY + 110.0).map(|z| (z * 1e6).round()), b.tin.elevation_at(OX + 500.0, OY + 110.0).map(|z| (z * 1e6).round()));
}

#[test]
fn breakline_makes_a_sharp_crease_that_plain_points_smooth_over() {
    // A V-shaped road ditch sampled only at its ends and along the invert as a breakline.
    let mut pts = Vec::new();
    for i in 0..=10 {
        for &(dy, z) in &[(-10.0, 12.0), (10.0, 12.0)] {
            pts.push(SurfacePoint::new("e", OX + i as f64 * 10.0, OY + 50.0 + dy, z));
        }
    }
    let invert = Breakline { name: "invert".into(), kind: BreaklineKind::Standard, vertices: (0..=10).map(|i| [OX + i as f64 * 10.0, OY + 50.0, 10.0]).collect() };
    let with = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: pts }).with(Operation::AddBreaklines { lines: vec![invert] }).build().unwrap();
    assert!((with.tin.elevation_at(OX + 47.0, OY + 50.0).unwrap() - 10.0).abs() < 1e-9);
    assert!((with.tin.elevation_at(OX + 47.0, OY + 55.0).unwrap() - 11.0).abs() < 1e-9);
    // Volume of the ditch below the banks (12 m): a triangular section 20 m wide, 2 m deep, 100 m long.
    let r = volume_against_level(&with.tin, 12.0, None).unwrap();
    assert!((r.fill - 0.5 * 20.0 * 2.0 * 100.0).abs() < 1e-6, "{}", r.fill);
}
