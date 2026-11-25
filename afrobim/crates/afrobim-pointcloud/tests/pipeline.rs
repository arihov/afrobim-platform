// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Ground extraction, surface output, comparison and section slices on a built scene.

mod common;
use afrobim_corridor::Tin;
use afrobim_pointcloud::*;

/// 100 m x 100 m at northing 9 960 000, 0.5 m spacing. Ground is a 3 percent cross fall plus a
/// gentle crown; a 10 x 10 m building (roof 6 m above ground) and a line of tree-like points
/// (3 to 8 m above ground, every 7th point of a column) stand on it. Class is 1 everywhere.
fn scene() -> (PointBuffer, Vec<bool>, impl Fn(f64, f64) -> f64) {
    let ground = |x: f64, y: f64| 100.0 + 0.03 * (x - 500000.0) + 0.5 * ((y - 9960000.0) / 20.0).sin();
    let mut b = PointBuffer::new(false, false);
    let mut nonground = Vec::new();
    for ix in 0..200 {
        for iy in 0..200 {
            let (x, y) = (500000.0 + ix as f64 * 0.5, 9960000.0 + iy as f64 * 0.5);
            let g = ground(x, y);
            let in_building = (30.0..40.0).contains(&(x - 500000.0)) && (30.0..40.0).contains(&(y - 9960000.0));
            let tree = (x - 500000.0) > 60.0 && (x - 500000.0) < 62.0 && iy % 7 == 0;
            let z = if in_building { g + 6.0 } else if tree { g + 3.0 + (iy % 5) as f64 } else { g };
            b.push(&PointRecord { xyz: [x, y, z], class: 1, intensity: 100, return_number: 1, number_of_returns: 1, ..Default::default() });
            nonground.push(in_building || tree);
        }
    }
    (b, nonground, ground)
}

#[test]
fn unclassified_cloud_to_ground_surface_with_fit_report() {
    let (b, nonground, ground) = scene();
    let pmf = PmfParams::from_series(1.0, vec![3, 5, 9, 15, 25], 0.15, 0.1, 1.0);
    let r = progressive_morphological_filter(&b, None, &pmf).unwrap();
    assert_eq!(r.ground.len() + r.non_ground.len(), b.len());
    let wrong_kept = r.ground.iter().filter(|&&i| nonground[i as usize]).count();
    let lost = r.non_ground.iter().filter(|&&i| !nonground[i as usize]).count();
    let total_non = nonground.iter().filter(|&&n| n).count();
    eprintln!("pmf: ground {} nonground {} wrong_kept {wrong_kept}/{total_non} lost {lost}", r.ground.len(), r.non_ground.len());
    // Tolerance for this scene: nearly all building and tree points removed, little real ground lost.
    assert!(wrong_kept * 100 <= total_non * 2, "{wrong_kept} of {total_non} nonground kept");
    assert!(lost * 100 <= b.len() * 2, "{lost} ground points lost");

    // The simple method (this crate's own) on the same scene.
    let gm = grid_minimum_slope_filter(&b, None, &GridMinSlopeParams { cell_size: 1.0, max_slope: 0.5, search_cells: 8, tolerance: 0.2 }).unwrap();
    assert_eq!(gm.ground.iter().filter(|&&i| nonground[i as usize]).count(), 0);

    // Thin to a 2 m grid (lowest point), build the TIN, report the fit to all ground points.
    let s = ground_surface(&b, &r.ground, &SurfaceSpec { grid: Some((2.0, CellStat::Min)), tolerance: Some(0.02) }).unwrap();
    assert_eq!(s.ground_points_in, r.ground.len());
    assert!(s.after_grid < s.ground_points_in / 3);
    assert!(s.after_decimation < s.after_grid);
    eprintln!("surface: {} -> {} -> {} points, fit {:?}", s.ground_points_in, s.after_grid, s.after_decimation, s.fit);
    assert!(s.fit.compared > 0);
    // The fit is against the true ground within the stated noise-free scene: tens of millimetres.
    assert!(s.fit.rms < 0.05 && s.fit.max_abs < 0.2, "{:?}", s.fit);
    // The surface follows the true ground away from the objects.
    for (x, y) in [(500010.0, 9960010.0), (500080.0, 9960090.0), (500050.0, 9960020.0)] {
        let z = s.tin.elevation_at(x, y).unwrap();
        assert!((z - ground(x, y)).abs() < 0.05, "{x} {y}: {z} vs {}", ground(x, y));
    }
    // Output for the terrain crate, coordinates untouched.
    let op = surface_operation("ground", &s.points);
    let def = afrobim_terrain::SurfaceDefinition::new().with(op);
    let rebuilt = def.build().unwrap();
    assert_eq!(rebuilt.tin.points().len(), s.tin.points().len());
    assert!(s.points.iter().all(|p| p[1] >= 9960000.0 && p[1] <= 9960100.0));
}

#[test]
fn existing_classification_is_used_when_present() {
    let (mut b, nonground, _) = scene();
    for (i, ng) in nonground.iter().enumerate() {
        b.class[i] = if *ng { 6 } else { 2 };
    }
    let g = ground_from_classes(&b, None, &[2]);
    assert_eq!(g.len(), nonground.iter().filter(|&&n| !n).count());
    let s = ground_surface(&b, &g, &SurfaceSpec { grid: Some((1.0, CellStat::Mean)), tolerance: None }).unwrap();
    assert!(s.fit.rms < 0.05, "{:?}", s.fit);
}

#[test]
fn compare_to_design_surface_with_bands_and_sections() {
    let (b, _, ground) = scene();
    // An "as-built check": the cloud against a design surface 0.1 m below the true ground, with
    // the building's roof far above it.
    let design = Tin::from_grid(500000.0, 9960000.0, 5.0, 5.0, 21, 21, |x, y| ground(x, y) - 0.1).unwrap();
    let bands = DifferenceBands::new(vec![-0.05, 0.05, 0.15, 1.0]).unwrap();
    let c = compare_to_tin(&b, None, &design, -1.0, 0.1, 80, &bands).unwrap();
    assert_eq!(c.indices.len(), b.len());
    assert!(c.stats.compared > 39_000);
    // The grid is coarse (5 m) and the crown curves, so the difference is the design offset plus a small chord error.
    assert!((c.stats.mean - 0.1).abs() < 0.2, "{:?}", c.stats);
    assert!(c.stats.max > 5.0, "the roof stands out: {:?}", c.stats);
    assert_eq!(c.band_counts.iter().sum::<u64>() as usize, c.stats.compared);
    assert!(c.band_counts[4] > 0, "roof points in the top band");
    let colors = c.colors(&BandColors::blue_white_red(bands.band_count()));
    assert_eq!(colors.len(), b.len());
    assert_eq!(c.histogram.total() as usize, c.stats.compared);

    // Section across the building at easting 500035, with the true ground and the roof visible.
    let ix = PointIndex::build(b, &IndexOptions::default()).unwrap();
    let slice = section_slice(&ix, &[[500035.0, 9960000.0], [500035.0, 9960100.0]], 1.0, None).unwrap();
    assert!((slice.length - 100.0).abs() < 1e-9);
    assert!(slice.points.len() >= 190);
    let zmax = slice.points.iter().map(|p| p.z).fold(f64::MIN, f64::max);
    assert!(zmax > ground(500035.0, 9960035.0) + 5.9);
    // Offsets are along the line from the start: the building spans offset 30 to 40.
    let roof: Vec<f64> = slice.points.iter().filter(|p| p.z > ground(500035.0, 9960035.0) + 5.0 && p.z < ground(500035.0, 9960035.0) + 7.0).map(|p| p.offset).collect();
    assert!(roof.iter().all(|&o| (29.5..=40.5).contains(&o)), "{roof:?}");
}
