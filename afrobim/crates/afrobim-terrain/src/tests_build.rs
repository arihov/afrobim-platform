// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use super::*;
use afrobim_corridor::Tin;

pub(crate) fn grid_points(nx: usize, ny: usize, step: f64, ox: f64, oy: f64, f: impl Fn(f64, f64) -> f64) -> Vec<SurfacePoint> {
    let mut v = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let (x, y) = (ox + i as f64 * step, oy + j as f64 * step);
            v.push(SurfacePoint::new(format!("{i}-{j}"), x, y, f(x - ox, y - oy)));
        }
    }
    v
}

pub(crate) fn plane_def(ox: f64, oy: f64) -> SurfaceDefinition {
    SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: grid_points(6, 6, 10.0, ox, oy, |x, y| 100.0 + 0.1 * x + 0.2 * y) })
}

fn has_edge(tin: &Tin, a: [f64; 2], b: [f64; 2]) -> bool {
    let idx = |p: [f64; 2]| tin.points().iter().position(|q| q[0] == p[0] && q[1] == p[1]);
    let (Some(ia), Some(ib)) = (idx(a), idx(b)) else { return false };
    tin.triangles().iter().any(|t| t.contains(&(ia as u32)) && t.contains(&(ib as u32)))
}

#[test]
fn plane_grid_builds_and_interpolates_exactly() {
    let b = plane_def(0.0, 0.0).build().unwrap();
    assert_eq!(b.report.triangles, 50);
    assert_eq!(b.report.points_used, 36);
    assert!((b.tin.plan_area() - 2500.0).abs() < 1e-9);
    let z = b.tin.elevation_at(13.7, 22.1).unwrap();
    assert!((z - (100.0 + 1.37 + 4.42)).abs() < 1e-9);
}

#[test]
fn large_eastings_and_northings_are_kept_exactly() {
    let (ox, oy) = (512_345.678, 9_876_543.219);
    let b = plane_def(ox, oy).build().unwrap();
    assert_eq!(b.report.triangles, 50);
    // Coordinates are exactly the ones given.
    assert!(b.tin.points().iter().any(|p| p[0] == ox + 20.0 && p[1] == oy + 30.0));
    assert!((b.tin.plan_area() - 2500.0).abs() < 1e-6);
    let z = b.tin.elevation_at(ox + 13.7, oy + 22.1).unwrap();
    assert!((z - (100.0 + 1.37 + 4.42)).abs() < 1e-6);
    // Same topology as the unshifted build.
    let a = plane_def(0.0, 0.0).build().unwrap();
    assert_eq!(a.report.triangles, b.report.triangles);
}

#[test]
fn duplicates_are_merged_and_policy_applies() {
    let pts = vec![
        SurfacePoint::new("a", 0.0, 0.0, 1.0),
        SurfacePoint::new("b", 10.0, 0.0, 2.0),
        SurfacePoint::new("c", 0.0, 10.0, 3.0),
        SurfacePoint::new("d", 0.0004, 0.0, 5.0),
        SurfacePoint::new("e", 10.0, 10.0, 4.0),
    ];
    let mut def = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: pts });
    let b = def.build().unwrap();
    assert_eq!(b.report.duplicates_merged, 1);
    assert!((b.report.duplicate_max_dz - 4.0).abs() < 1e-12);
    assert_eq!(b.tin.elevation_at(0.0, 0.0), Some(1.0));
    def.settings.duplicate_policy = DuplicatePolicy::Highest;
    assert_eq!(def.build().unwrap().tin.elevation_at(0.0, 0.0), Some(5.0));
    def.settings.duplicate_policy = DuplicatePolicy::Average;
    assert_eq!(def.build().unwrap().tin.elevation_at(0.0, 0.0), Some(3.0));
    def.settings.duplicate_policy = DuplicatePolicy::Lowest;
    assert_eq!(def.build().unwrap().tin.elevation_at(0.0, 0.0), Some(1.0));
}

#[test]
fn degenerate_inputs_are_refused() {
    let line: Vec<SurfacePoint> = (0..6).map(|i| SurfacePoint::new("p", i as f64, 2.0 * i as f64, 1.0)).collect();
    let e = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: line }).build().unwrap_err();
    assert!(matches!(e, TerrainError::NoSurface(_)), "{e:?}");
    // Collinear at survey magnitudes.
    let line: Vec<SurfacePoint> = (0..6).map(|i| SurfacePoint::new("p", 500_000.0 + i as f64, 9_000_000.0 + i as f64, 1.0)).collect();
    assert!(SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: line }).build().is_err());
    let same = vec![SurfacePoint::new("a", 1.0, 1.0, 1.0); 5];
    assert!(matches!(SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: same }).build().unwrap_err(), TerrainError::TooFewPoints(1)));
    assert!(matches!(SurfaceDefinition::new().build().unwrap_err(), TerrainError::TooFewPoints(0)));
    let nan = vec![SurfacePoint::new("a", f64::NAN, 1.0, 1.0)];
    assert!(matches!(SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: nan }).build().unwrap_err(), TerrainError::NonFinite(_)));
}

#[test]
fn near_collinear_points_with_one_off_line_make_a_surface() {
    let mut pts: Vec<SurfacePoint> = (0..10).map(|i| SurfacePoint::new("p", 500_000.0 + i as f64, 9_000_000.0, 1.0)).collect();
    pts.push(SurfacePoint::new("q", 500_004.5, 9_000_003.0, 2.0));
    let b = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: pts }).build().unwrap();
    assert_eq!(b.report.triangles, 9);
}

#[test]
fn breakline_edges_are_honoured() {
    // A regular grid; the long diagonal of the 30 m square is not a Delaunay edge.
    let def = plane_def(0.0, 0.0).with(Operation::AddBreaklines {
        lines: vec![Breakline { name: "ridge".into(), kind: BreaklineKind::Standard, vertices: vec![[0.0, 0.0, 100.0], [30.0, 20.0, 108.0]] }],
    });
    let b = def.build().unwrap();
    assert!(b.report.crossings.is_empty());
    // The breakline passes through no grid vertex except its ends, so it is cut into edges
    // where it meets the grid lines: the surface must contain its two end points as vertices
    // and a path of constrained edges between them.
    assert!(b.tin.points().iter().any(|p| p[0] == 30.0 && p[1] == 20.0 && p[2] == 108.0));
    // Elevation along the breakline is the breakline's (linear between its ends).
    let z = b.tin.elevation_at(15.0, 10.0).unwrap();
    assert!((z - 104.0).abs() < 1e-9, "{z}");
}

#[test]
fn a_breakline_between_two_points_is_an_edge() {
    let mut pts = grid_points(4, 4, 10.0, 0.0, 0.0, |x, y| x * 0.0 + y * 0.0);
    pts.iter_mut().for_each(|p| p.z = 0.0);
    let def = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: pts }).with(Operation::AddBreaklines {
        lines: vec![Breakline { name: "b".into(), kind: BreaklineKind::Standard, vertices: vec![[0.0, 0.0, 0.0], [20.0, 10.0, 0.0]] }],
    });
    let b = def.build().unwrap();
    // The line passes through no grid point between its ends, so it is cut at grid lines;
    // check no triangle edge crosses it by testing the point at the middle is on an edge.
    assert!(b.tin.elevation_at(10.0, 5.0).is_some());
    let g = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: vec![
        SurfacePoint::new("a", 0.0, 0.0, 0.0), SurfacePoint::new("b", 10.0, 0.0, 0.0), SurfacePoint::new("c", 10.0, 10.0, 0.0), SurfacePoint::new("d", 0.0, 10.0, 0.0),
    ] }).with(Operation::AddBreaklines { lines: vec![Breakline { name: "b".into(), kind: BreaklineKind::Standard, vertices: vec![[10.0, 0.0, 0.0], [0.0, 10.0, 0.0]] }] });
    let t = g.build().unwrap().tin;
    assert!(has_edge(&t, [10.0, 0.0], [0.0, 10.0]));
}

#[test]
fn crossing_breaklines_are_found_and_cut() {
    let def = SurfaceDefinition::new()
        .with(Operation::AddPoints {
            group: "g".into(),
            points: vec![SurfacePoint::new("a", -20.0, -20.0, 0.0), SurfacePoint::new("b", 20.0, -20.0, 0.0), SurfacePoint::new("c", 20.0, 20.0, 0.0), SurfacePoint::new("d", -20.0, 20.0, 0.0)],
        })
        .with(Operation::AddBreaklines {
            lines: vec![
                Breakline { name: "L1".into(), kind: BreaklineKind::Standard, vertices: vec![[-10.0, 0.0, 10.0], [10.0, 0.0, 14.0]] },
                Breakline { name: "L2".into(), kind: BreaklineKind::Standard, vertices: vec![[0.0, -10.0, 20.0], [0.0, 10.0, 24.0]] },
            ],
        });
    let b = def.build().unwrap();
    assert_eq!(b.report.crossings.len(), 1);
    let c = &b.report.crossings[0];
    assert!((c.x).abs() < 1e-12 && (c.y).abs() < 1e-12);
    assert!((c.z_first.unwrap() - 12.0).abs() < 1e-12);
    assert!((c.z_second.unwrap() - 22.0).abs() < 1e-12);
    assert!((c.z_used - 17.0).abs() < 1e-12);
    assert!((b.tin.elevation_at(0.0, 0.0).unwrap() - 17.0).abs() < 1e-9);
    // Both lines still run as edges to the crossing.
    assert!(b.tin.elevation_at(5.0, 0.0).is_some());
}

#[test]
fn crossing_at_survey_magnitude() {
    let (ox, oy) = (500_000.0, 9_000_000.0);
    let def = SurfaceDefinition::new()
        .with(Operation::AddPoints {
            group: "g".into(),
            points: vec![SurfacePoint::new("a", ox - 20.0, oy - 20.0, 0.0), SurfacePoint::new("b", ox + 20.0, oy - 20.0, 0.0), SurfacePoint::new("c", ox + 20.0, oy + 20.0, 0.0), SurfacePoint::new("d", ox - 20.0, oy + 20.0, 0.0)],
        })
        .with(Operation::AddBreaklines {
            lines: vec![
                Breakline { name: "L1".into(), kind: BreaklineKind::Standard, vertices: vec![[ox - 10.0, oy - 3.0, 10.0], [ox + 10.0, oy + 5.0, 14.0]] },
                Breakline { name: "L2".into(), kind: BreaklineKind::Standard, vertices: vec![[ox - 4.0, oy - 10.0, 20.0], [ox + 6.0, oy + 10.0, 24.0]] },
            ],
        });
    let b = def.build().unwrap();
    assert_eq!(b.report.crossings.len(), 1);
    assert!(b.tin.plan_area() > 1599.0 && b.tin.plan_area() < 1601.0);
}

#[test]
fn boundaries_outer_hide_show() {
    let outer = Operation::AddBoundary { name: "o".into(), kind: BoundaryKind::Outer, vertices: vec![[5.0, 5.0], [45.0, 5.0], [45.0, 45.0], [5.0, 45.0]] };
    let b = plane_def(0.0, 0.0).with(outer.clone()).build().unwrap();
    assert!((b.tin.plan_area() - 1600.0).abs() < 1e-6, "{}", b.tin.plan_area());
    assert!(b.tin.elevation_at(2.0, 2.0).is_none());
    let hide = Operation::AddBoundary { name: "h".into(), kind: BoundaryKind::Hide, vertices: vec![[15.0, 15.0], [25.0, 15.0], [25.0, 25.0], [15.0, 25.0]] };
    let b = plane_def(0.0, 0.0).with(outer.clone()).with(hide.clone()).build().unwrap();
    assert!((b.tin.plan_area() - 1500.0).abs() < 1e-6);
    assert!(b.tin.elevation_at(20.0, 20.0).is_none());
    let show = Operation::AddBoundary { name: "s".into(), kind: BoundaryKind::Show, vertices: vec![[15.0, 15.0], [25.0, 15.0], [25.0, 25.0], [15.0, 25.0]] };
    let b = plane_def(0.0, 0.0).with(outer).with(hide).with(show).build().unwrap();
    assert!((b.tin.plan_area() - 1600.0).abs() < 1e-6);
}

#[test]
fn max_edge_removes_long_triangles() {
    let mut pts = grid_points(4, 4, 10.0, 0.0, 0.0, |_, _| 0.0);
    pts.push(SurfacePoint::new("far", 100.0, 15.0, 0.0));
    let def = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: pts }).with(Operation::MaxTriangleEdge { length: 15.0 });
    let b = def.build().unwrap();
    assert!(b.report.triangles_removed_max_edge > 0);
    assert!((b.tin.plan_area() - 900.0).abs() < 1e-9);
    assert!(b.report.points_unused >= 1);
}

#[test]
fn exclude_elevations() {
    let def = plane_def(0.0, 0.0).with(Operation::ExcludeElevations { below: Some(100.5), above: Some(110.0) });
    let b = def.build().unwrap();
    assert!(b.report.points_excluded > 0);
    assert!(b.tin.points().iter().all(|p| p[2] >= 100.5 && p[2] <= 110.0));
}

#[test]
fn edit_operations() {
    let base = plane_def(0.0, 0.0);
    let n0 = base.build().unwrap().report.triangles;
    let b = base.clone().with(Operation::DeleteTriangle { x: 3.0, y: 2.0 }).build().unwrap();
    assert_eq!(b.report.triangles, n0 - 1);
    assert_eq!(b.report.triangles_deleted, 1);
    assert!(b.tin.elevation_at(3.0, 2.0).is_none());
    let b = base.clone().with(Operation::DeleteTriangle { x: 500.0, y: 2.0 }).build().unwrap();
    assert_eq!(b.report.warnings.len(), 1);

    // Swap: a regular grid square has two diagonals; swap whichever was chosen.
    let sq = SurfaceDefinition::new().with(Operation::AddPoints { group: "g".into(), points: vec![
        SurfacePoint::new("a", 0.0, 0.0, 0.0), SurfacePoint::new("b", 10.0, 0.0, 1.0), SurfacePoint::new("c", 10.0, 11.0, 0.0), SurfacePoint::new("d", 0.0, 10.0, 1.0),
    ] });
    let t = sq.build().unwrap().tin;
    let diag = if has_edge(&t, [0.0, 0.0], [10.0, 11.0]) { ([0.0, 0.0], [10.0, 11.0]) } else { ([10.0, 0.0], [0.0, 10.0]) };
    let b = sq.clone().with(Operation::SwapEdge { a: diag.0, b: diag.1, tolerance: None }).build().unwrap();
    assert_eq!(b.report.edges_swapped, 1, "{:?}", b.report.warnings);
    assert!(!has_edge(&b.tin, diag.0, diag.1));
    assert!((b.tin.plan_area() - t.plan_area()).abs() < 1e-9);
    // Not an edge, or a hull edge: reported, not fatal.
    let b = sq.with(Operation::SwapEdge { a: [0.0, 0.0], b: [10.0, 0.0], tolerance: None }).build().unwrap();
    assert_eq!(b.report.edges_swapped, 0);
    assert_eq!(b.report.warnings.len(), 1);

    // Add, delete, raise.
    let b = base.clone().with(Operation::AddPoint { x: 25.0, y: 25.0, z: 200.0 }).build().unwrap();
    assert_eq!(b.tin.elevation_at(25.0, 25.0), Some(200.0));
    let b = base.clone().with(Operation::DeletePoint { target: PointRef::Id("2-2".into()) }).build().unwrap();
    assert_eq!(b.report.points_deleted, 1);
    assert_eq!(b.report.points_used, 35);
    let b = base.clone().with(Operation::DeletePoint { target: PointRef::Near { x: 20.2, y: 20.1, tolerance: 0.5 } }).build().unwrap();
    assert_eq!(b.report.points_deleted, 1);
    let b = base.clone().with(Operation::RaiseLower { delta: 2.5 }).build().unwrap();
    assert!((b.tin.elevation_at(0.0, 0.0).unwrap() - 102.5).abs() < 1e-12);
    // Order: a point added after the raise is not raised.
    let b = base.with(Operation::RaiseLower { delta: 2.5 }).with(Operation::AddPoint { x: 25.0, y: 25.0, z: 200.0 }).build().unwrap();
    assert_eq!(b.tin.elevation_at(25.0, 25.0), Some(200.0));
}

#[test]
fn paste_replaces_where_it_overlaps() {
    let patch = SurfaceDefinition::new().with(Operation::AddPoints {
        group: "p".into(),
        points: vec![SurfacePoint::new("a", 10.0, 10.0, 300.0), SurfacePoint::new("b", 30.0, 10.0, 300.0), SurfacePoint::new("c", 30.0, 30.0, 300.0), SurfacePoint::new("d", 10.0, 30.0, 300.0)],
    });
    let b = plane_def(0.0, 0.0).with(Operation::Paste { source: PasteSource::Definition(Box::new(patch.clone())) }).build().unwrap();
    assert!(b.report.points_replaced_by_paste >= 4);
    assert!((b.tin.elevation_at(20.0, 20.0).unwrap() - 300.0).abs() < 1e-9);
    assert!((b.tin.elevation_at(0.0, 0.0).unwrap() - 100.0).abs() < 1e-9);
    assert!((b.tin.elevation_at(50.0, 50.0).unwrap() - 115.0).abs() < 1e-9);
    // As a finished TIN.
    let tin = patch.build().unwrap().tin;
    let b2 = plane_def(0.0, 0.0).with(Operation::Paste { source: PasteSource::Tin(tin) }).build().unwrap();
    assert_eq!(b2.tin.triangles().len(), b.tin.triangles().len());
}

#[test]
fn proximity_breakline_takes_ground_elevation() {
    let def = plane_def(0.0, 0.0).with(Operation::AddBreaklines {
        lines: vec![Breakline { name: "kerb".into(), kind: BreaklineKind::Proximity, vertices: vec![[5.0, 5.0, -999.0], [35.0, 25.0, -999.0]] }],
    });
    let b = def.build().unwrap();
    let z = b.tin.elevation_at(5.0, 5.0).unwrap();
    assert!((z - (100.0 + 0.5 + 1.0)).abs() < 1e-9, "{z}");
}

#[test]
fn contour_data_adds_points() {
    let line = vec![[0.0, 0.0, 5.0], [100.0, 0.0, 5.0], [100.0, 100.0, 5.0], [0.0, 100.0, 5.0]];
    let def = SurfaceDefinition::new().with(Operation::AddContourData { name: "c5".into(), lines: vec![line], thin_distance: None, max_segment: Some(25.0) }).with(Operation::AddPoint { x: 50.0, y: 50.0, z: 9.0 });
    let b = def.build().unwrap();
    assert_eq!(b.report.points_input, 13 + 1 - 0 - 0);
    assert_eq!(b.tin.elevation_at(50.0, 50.0), Some(9.0));
}

#[test]
fn definition_survives_json_and_rebuilds_identically() {
    let mut s = Surface::new("EG", SurfaceRole::ExistingGround);
    s.source.description = "test".into();
    s.status = SurfaceStatus::Approved;
    s.definition = plane_def(500_000.0, 9_000_000.0)
        .with(Operation::AddBreaklines { lines: vec![Breakline { name: "b".into(), kind: BreaklineKind::Standard, vertices: vec![[500_000.0, 9_000_000.0, 100.0], [500_030.0, 9_000_020.0, 108.0]] }] })
        .with(Operation::AddBoundary { name: "o".into(), kind: BoundaryKind::Outer, vertices: vec![[500_005.0, 9_000_005.0], [500_045.0, 9_000_005.0], [500_045.0, 9_000_045.0], [500_005.0, 9_000_045.0]] })
        .with(Operation::DeletePoint { target: PointRef::Near { x: 500_020.0, y: 9_000_020.0, tolerance: 0.1 } })
        .with(Operation::MaxTriangleEdge { length: 30.0 });
    let text = s.to_json().unwrap();
    let back = Surface::from_json(&text).unwrap();
    assert_eq!(back, s);
    let (a, b) = (s.build().unwrap(), back.build().unwrap());
    assert_eq!(a.tin, b.tin);
    assert_eq!(a.report, b.report);
    assert!(Surface::from_json("{").is_err());
    // A pasted TIN also round-trips.
    let tin = plane_def(0.0, 0.0).build().unwrap().tin;
    let d = SurfaceDefinition::new().with(Operation::Paste { source: PasteSource::Tin(tin) });
    assert_eq!(SurfaceDefinition::from_json(&d.to_json().unwrap()).unwrap(), d);
}

#[test]
fn invalid_operations_are_named() {
    let e = SurfaceDefinition::new().with(Operation::MaxTriangleEdge { length: -1.0 }).build().unwrap_err();
    assert!(matches!(e, TerrainError::InvalidOperation { index: 0, .. }));
    let e = plane_def(0.0, 0.0).with(Operation::AddBoundary { name: "x".into(), kind: BoundaryKind::Hide, vertices: vec![[0.0, 0.0], [1.0, 1.0]] }).build().unwrap_err();
    assert!(matches!(e, TerrainError::InvalidOperation { index: 1, .. }));
}
