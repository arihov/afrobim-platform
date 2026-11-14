//! The TIN: elevation at a point, section cuts, bounds, the stored encoding.
//!
//! Every surface here is small enough that the expected values are worked by hand in the comments.

use afrobim_corridor::{Tin, TinEncoded, TinError, TIN_ENCODING};

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9, "{what}: got {got}, want {want}");
}

fn close_line(got: &[(f64, f64)], want: &[(f64, f64)]) {
    assert_eq!(got.len(), want.len(), "got {got:?}, want {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g.0 - w.0).abs() <= 1e-9 && (g.1 - w.1).abs() <= 1e-9, "got {got:?}, want {want:?}");
    }
}

/// A 10 m square folded along its diagonal from (0, 0) to (10, 10).
///
/// Triangle A (0,0,0) (10,0,10) (10,10,20) is the plane z = x + y, below the diagonal (y < x).
/// Triangle B (0,0,0) (10,10,20) (0,10,0) is the plane z = 2x, above it (y > x).
fn folded_square() -> Tin {
    Tin::new(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 10.0], [10.0, 10.0, 20.0], [0.0, 10.0, 0.0]], vec![[0, 1, 2], [0, 2, 3]]).unwrap()
}

#[test]
fn elevation_on_two_triangles() {
    let t = folded_square();
    close(t.elevation_at(6.0, 2.0).unwrap(), 8.0, "in A, z = x + y");
    close(t.elevation_at(2.0, 6.0).unwrap(), 4.0, "in B, z = 2x");
    close(t.elevation_at(5.0, 5.0).unwrap(), 10.0, "on the shared edge both planes give 10");
    close(t.elevation_at(0.0, 0.0).unwrap(), 0.0, "corner");
    close(t.elevation_at(10.0, 10.0).unwrap(), 20.0, "corner");
    close(t.elevation_at(10.0, 4.0).unwrap(), 14.0, "on the outer edge");
    assert_eq!(t.elevation_at(10.5, 4.0), None);
    assert_eq!(t.elevation_at(-0.001, 5.0), None);
    assert_eq!(t.elevation_at(5.0, 11.0), None);

    let b = t.bounds();
    assert_eq!((b.min_x, b.min_y, b.min_z, b.max_x, b.max_y, b.max_z), (0.0, 0.0, 0.0, 10.0, 10.0, 20.0));
    close(t.plan_area(), 100.0, "plan area");
    // The four sides of the square; the diagonal belongs to two triangles.
    assert_eq!(t.boundary_edges(), vec![[0, 1], [0, 3], [1, 2], [2, 3]]);
}

#[test]
fn section_cut_on_two_triangles() {
    let t = folded_square();
    // Along y = 5 from x = 0: in B (z = 2x) up to the diagonal at x = 5, then in A (z = x + 5).
    close_line(&t.section((0.0, 5.0), (10.0, 5.0))[0], &[(0.0, 0.0), (5.0, 10.0), (10.0, 15.0)]);
    // The same line started 5 m before the surface and ended 5 m after: distances are from the start.
    let runs = t.section((-5.0, 5.0), (15.0, 5.0));
    assert_eq!(runs.len(), 1);
    close_line(&runs[0], &[(5.0, 0.0), (10.0, 10.0), (15.0, 15.0)]);
    // Reversed, the distances run the other way.
    close_line(&t.section((10.0, 5.0), (0.0, 5.0))[0], &[(0.0, 15.0), (5.0, 10.0), (10.0, 0.0)]);
    // Along the shared edge itself: one straight from 0 to 20 over 10√2 m, not two copies.
    let runs = t.section((0.0, 0.0), (10.0, 10.0));
    assert_eq!(runs.len(), 1);
    close_line(&runs[0], &[(0.0, 0.0), (200f64.sqrt(), 20.0)]);
    // A vertical line (constant x) at x = 4: A for y < 4 (z = 4 + y), then B (z = 8).
    close_line(&t.section((4.0, 0.0), (4.0, 10.0))[0], &[(0.0, 4.0), (4.0, 8.0), (10.0, 8.0)]);
    // Lines that miss the surface, and a line with no length.
    assert!(t.section((20.0, 0.0), (20.0, 10.0)).is_empty());
    assert!(t.section((-5.0, -1.0), (15.0, -1.0)).is_empty());
    assert!(t.section((3.0, 3.0), (3.0, 3.0)).is_empty());
}

/// A 5 × 3 grid of points 10 m apart with z = x y / 10 at the nodes; 16 triangles.
fn warped_grid() -> Tin {
    Tin::from_grid(0.0, 0.0, 10.0, 10.0, 5, 3, |x, y| x * y / 10.0).unwrap()
}

#[test]
fn elevation_and_section_on_a_small_grid() {
    let t = warped_grid();
    assert_eq!((t.points().len(), t.triangles().len()), (15, 16));
    close(t.plan_area(), 800.0, "40 m by 20 m");
    close(t.elevation_at(30.0, 20.0).unwrap(), 60.0, "a node");
    // Cell (0..10, 0..10) is split from (0,0,0) to (10,10,10): the middle of that diagonal is 5,
    // not the 2.5 of the formula the nodes came from. The surface is the triangles.
    close(t.elevation_at(5.0, 5.0).unwrap(), 5.0, "on a diagonal");
    // (25, 3) is in the lower triangle of cell (20..30, 0..10): corners (20,0,0) (30,0,0)
    // (30,10,30). The plane through those three corners is z = 3y. At y = 3: 9.
    close(t.elevation_at(25.0, 3.0).unwrap(), 9.0, "inside a triangle");

    // Along y = 5 from x = 0 to 40. On the grid line x = 10k the edge runs from z = 0 (y = 0) to
    // z = 10k (y = 10), so z = 5k at y = 5. On the diagonal of cell i, from (10i, 0, 0) to
    // (10i + 10, 10, 10i + 10), the midpoint is at x = 10i + 5 with z = 5i + 5.
    let runs = t.section((0.0, 5.0), (40.0, 5.0));
    assert_eq!(runs.len(), 1);
    close_line(
        &runs[0],
        &[(0.0, 0.0), (5.0, 5.0), (10.0, 5.0), (15.0, 10.0), (20.0, 10.0), (25.0, 15.0), (30.0, 15.0), (35.0, 20.0), (40.0, 20.0)],
    );
    // Every point of the cut agrees with elevation-at-point.
    for &(d, z) in &runs[0] {
        close(t.elevation_at(d, 5.0).unwrap(), z, "cut against point query");
    }
}

#[test]
fn a_plane_cuts_to_a_single_straight() {
    // z = 100 + 0.1 x − 0.05 y on an 11 × 11 grid: 200 triangles, one plane.
    let t = Tin::from_grid(-50.0, -50.0, 10.0, 10.0, 11, 11, |x, y| 100.0 + 0.1 * x - 0.05 * y).unwrap();
    let runs = t.section((-40.0, -37.0), (45.0, 22.0));
    assert_eq!(runs.len(), 1);
    let length = (85.0f64 * 85.0 + 59.0 * 59.0).sqrt();
    assert_eq!(runs[0].len(), 2, "collinear points are merged: {:?}", runs[0]);
    assert!((runs[0][0].0).abs() < 1e-9 && (runs[0][1].0 - length).abs() < 1e-9);
    assert!((runs[0][0].1 - (100.0 - 4.0 + 1.85)).abs() < 1e-9 && (runs[0][1].1 - (100.0 + 4.5 - 1.1)).abs() < 1e-9);
}

#[test]
fn a_hole_breaks_the_section_into_runs() {
    // Three 10 m squares in a row, level at 7, with the middle one left out.
    let points: Vec<[f64; 3]> = (0..8).map(|i| [10.0 * (i % 4) as f64, 10.0 * (i / 4) as f64, 7.0]).collect();
    let t = Tin::new(points, vec![[0, 1, 5], [0, 5, 4], [2, 3, 7], [2, 7, 6]]).unwrap();
    let runs = t.section((-2.0, 4.0), (32.0, 4.0));
    assert_eq!(runs.len(), 2);
    close_line(&runs[0], &[(2.0, 7.0), (12.0, 7.0)]);
    close_line(&runs[1], &[(22.0, 7.0), (32.0, 7.0)]);
    assert_eq!(t.elevation_at(15.0, 4.0), None);
    assert_eq!(t.boundary_edges().len(), 8);
}

#[test]
fn bad_input_is_refused() {
    let square = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 2.0, 0.0]];
    assert_eq!(Tin::new(square.clone(), vec![]), Err(TinError::Empty));
    assert_eq!(Tin::new(square.clone(), vec![[0, 1, 4]]), Err(TinError::BadIndex { triangle: 0, point: 4, count: 4 }));
    assert_eq!(Tin::new(square.clone(), vec![[0, 1, 2], [0, 2, 3]]), Err(TinError::Degenerate(1)));
    assert_eq!(Tin::new(vec![[0.0, f64::NAN, 0.0]], vec![[0, 0, 0]]), Err(TinError::BadPoint(0)));
}

#[test]
fn the_stored_form_is_compact_and_exact() {
    let t = warped_grid();
    let json = serde_json::to_string(&t).unwrap();
    let back: Tin = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t);
    close(back.elevation_at(25.0, 3.0).unwrap(), 9.0, "after the round trip");

    let stored: TinEncoded = serde_json::from_str(&json).unwrap();
    assert_eq!(stored.encoding, TIN_ENCODING);
    assert_eq!((stored.point_count, stored.triangle_count), (15, 16));
    // 24 bytes a point and 12 a triangle, 4 characters for every 3 bytes.
    assert_eq!((stored.points.len(), stored.triangles.len()), (15 * 32, 16 * 16));

    // A value that decimal text would have to round survives bit for bit.
    let odd = Tin::new(vec![[0.1 + 0.2, 1.0 / 3.0, 2f64.sqrt()], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![[0, 1, 2]]).unwrap();
    let back: Tin = serde_json::from_value(serde_json::to_value(&odd).unwrap()).unwrap();
    assert_eq!(back.points()[0], [0.1 + 0.2, 1.0 / 3.0, 2f64.sqrt()]);

    // Damaged data is refused, not half read.
    let mut bad = stored.clone();
    bad.point_count = 14;
    assert!(serde_json::from_value::<Tin>(serde_json::to_value(&bad).unwrap()).is_err());
    let mut bad = stored.clone();
    bad.encoding = "something else".to_string();
    assert!(serde_json::from_value::<Tin>(serde_json::to_value(&bad).unwrap()).is_err());
    let mut bad = stored;
    bad.triangles.replace_range(0..1, "*");
    assert!(serde_json::from_value::<Tin>(serde_json::to_value(&bad).unwrap()).is_err());
}
