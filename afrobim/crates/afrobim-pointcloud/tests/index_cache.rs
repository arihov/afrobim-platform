//! Index queries against brute force, level of detail, nearest neighbours, cache and export.

mod common;
use afrobim_pointcloud::*;
use common::*;

fn brute_box(b: &PointBuffer, q: &Bounds3) -> Vec<[f64; 3]> {
    let mut v: Vec<[f64; 3]> = b.xyz.iter().copied().filter(|p| q.contains(*p)).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

fn coords(ix: &PointIndex, ids: &[u32]) -> Vec<[f64; 3]> {
    let mut v: Vec<[f64; 3]> = ids.iter().map(|&i| ix.points().xyz[i as usize]).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

#[test]
fn queries_match_brute_force() {
    let raw = synthetic(30_000, 7);
    let ix = PointIndex::build(raw.clone(), &IndexOptions::default()).unwrap();
    assert_eq!(ix.len(), 30_000);
    // Stored coordinates are the input coordinates, not recentred: same multiset.
    let mut a = raw.xyz.clone();
    a.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(coords(&ix, &(0..30_000).collect::<Vec<_>>()), a);

    for q in [
        Bounds3::new([500050.0, 9960060.0, 90.0], [500090.0, 9960100.0, 110.0]),
        Bounds3::plan([500000.0, 9960000.0], [500200.0, 9960200.0]),
        Bounds3::new([500010.0, 9960010.0, 103.0], [500060.0, 9960020.0, 104.0]),
        Bounds3::plan([400000.0, 9960000.0], [400010.0, 9960010.0]),
    ] {
        assert_eq!(coords(&ix, &ix.query_box(&q)), brute_box(&raw, &q));
    }

    // Polygon (a triangle) and polyline swath.
    let tri = [[500020.0, 9960020.0], [500120.0, 9960040.0], [500060.0, 9960140.0]];
    let in_poly = coords(&ix, &ix.query_polygon(&tri, Some((95.0, 106.0))));
    let o = tri[0];
    let ring: Vec<[f64; 2]> = tri.iter().map(|r| [r[0] - o[0], r[1] - o[1]]).collect();
    let mut want: Vec<[f64; 3]> = raw.xyz.iter().copied().filter(|p| p[2] >= 95.0 && p[2] <= 106.0 && inside(&ring, [p[0] - o[0], p[1] - o[1]])).collect();
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(in_poly, want);
    assert!(!want.is_empty());

    let line = [[500010.0, 9960010.0], [500100.0, 9960050.0], [500150.0, 9960180.0]];
    let d = 6.0;
    let swath = ix.query_polyline(&line, d);
    let mut dedup = swath.clone();
    dedup.dedup();
    assert_eq!(dedup.len(), swath.len());
    let mut want: Vec<[f64; 3]> = raw.xyz.iter().copied().filter(|p| line.windows(2).any(|w| seg_dist([p[0], p[1]], w[0], w[1]) <= d)).collect();
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(coords(&ix, &swath), want);
}

fn inside(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
        j = i;
    }
    c
}

fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (px, py, dx, dy) = (p[0] - a[0], p[1] - a[1], b[0] - a[0], b[1] - a[1]);
    let t = ((px * dx + py * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - t * dx).powi(2) + (py - t * dy).powi(2)).sqrt()
}

#[test]
fn nearest_neighbours_match_brute_force() {
    let raw = synthetic(20_000, 99);
    let ix = PointIndex::build(raw.clone(), &IndexOptions::default()).unwrap();
    for (q, plan) in [([500100.3, 9960100.7, 101.0], false), ([500000.0, 9960000.0, 90.0], true), ([500250.0, 9960250.0, 100.0], false)] {
        let got = ix.nearest(q, 7, plan);
        assert_eq!(got.len(), 7);
        let dist = |p: &[f64; 3]| if plan { ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt() } else { ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() };
        let mut all: Vec<f64> = raw.xyz.iter().map(dist).collect();
        all.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (k, n) in got.iter().enumerate() {
            assert!((n.distance - all[k]).abs() < 1e-9, "k {k}: {} vs {}", n.distance, all[k]);
            assert!((dist(&ix.points().xyz[n.index as usize]) - n.distance).abs() < 1e-9);
        }
    }
    assert!(ix.nearest([0.0; 3], 0, false).is_empty());
    // More neighbours than points.
    let small = PointIndex::build(synthetic(5, 1), &IndexOptions::default()).unwrap();
    assert_eq!(small.nearest([500100.0, 9960100.0, 100.0], 50, false).len(), 5);
}

#[test]
fn level_of_detail_returns_at_most_n_inside_the_view() {
    let ix = PointIndex::build(synthetic(60_000, 3), &IndexOptions::default()).unwrap();
    let view = Bounds3::plan([500040.0, 9960040.0], [500140.0, 9960140.0]);
    let all = ix.query_box(&view).len();
    assert!(all > 10_000);
    for n in [100usize, 1000, 5000] {
        let lod = ix.lod(&view, n);
        assert!(lod.indices.len() <= n && lod.indices.len() >= n / 2, "n {n}: {}", lod.indices.len());
        assert!(!lod.complete);
        assert!(lod.indices.iter().all(|&i| view.contains(ix.points().xyz[i as usize])));
        let mut d = lod.indices.clone();
        d.sort_unstable();
        d.dedup();
        assert_eq!(d.len(), lod.indices.len());
        // Spread over the view: every quarter of the view has points.
        let mut quarters = [0; 4];
        for &i in &lod.indices {
            let p = ix.points().xyz[i as usize];
            quarters[usize::from(p[0] > 500090.0) * 2 + usize::from(p[1] > 9960090.0)] += 1;
        }
        assert!(quarters.iter().all(|&q| q > n / 10), "{quarters:?}");
    }
    // A small view returns everything.
    let tiny = Bounds3::plan([500040.0, 9960040.0], [500042.0, 9960042.0]);
    let lod = ix.lod(&tiny, 10_000);
    assert!(lod.complete);
    assert_eq!(lod.indices.len(), ix.query_box(&tiny).len());
    // Display buffer: positions are relative to a returned local origin; the stored data is unchanged.
    let lod = ix.lod(&view, 500);
    let colors = colorize(ix.points(), Some(&lod.indices), &ColorMode::Class { palette: ClassPalette::standard() });
    let db = ix.display_buffer(&lod.indices, &colors);
    assert_eq!(db.positions.len(), lod.indices.len());
    assert_eq!(db.colors.len(), lod.indices.len());
    assert!(db.origin[1] > 9960000.0);
    for (k, &i) in lod.indices.iter().enumerate() {
        let p = ix.points().xyz[i as usize];
        assert!(db.positions[k][1].abs() < 100.0);
        assert!((db.origin[1] + db.positions[k][1] as f64 - p[1]).abs() < 1e-3);
    }
}

#[test]
fn point_limit_policies() {
    let dir = temp_dir("limit");
    let p = dir.join("a.las");
    write_las(&p, 2, 0, false, &known_points(1000), None);
    let mut src = open_source(&p, &SourceOptions::default()).unwrap();
    let err = PointIndex::build_from_source(src.as_mut(), &IndexOptions { max_points: Some(300), ..Default::default() }).unwrap_err();
    assert!(matches!(err, PointCloudError::TooManyPoints { count: 1000, limit: 300 }));
    let mut src = open_source(&p, &SourceOptions::default()).unwrap();
    let ix = PointIndex::build_from_source(src.as_mut(), &IndexOptions { max_points: Some(300), overflow: Overflow::KeepEveryNth, ..Default::default() }).unwrap();
    assert_eq!(ix.thinning_stride(), 4);
    assert_eq!(ix.len(), 250);
    assert_eq!(ix.source_point_count(), 1000);
}

#[test]
fn cache_hit_rebuild_and_hash_check() {
    let dir = temp_dir("cache");
    let p = dir.join("a.laz");
    let pts = known_points(4000);
    write_las(&p, 4, 7, true, &pts, Some("PROJCS[\"x\",AUTHORITY[\"EPSG\",\"32636\"]]"));
    let opts = SourceOptions::default();
    let idx = IndexOptions::default();
    let first = open_indexed(&p, &opts, &idx).unwrap();
    assert_eq!(first.cache, CacheStatus::Built);
    assert!(first.cache_path.exists() && first.cache_path.to_string_lossy().ends_with("a.laz.afrocloud"));
    assert!(first.cache_write_error.is_none());
    let second = open_indexed(&p, &opts, &idx).unwrap();
    assert_eq!(second.cache, CacheStatus::Hit);
    assert_eq!(second.source_hash, first.source_hash);
    assert_eq!(second.header, first.header);
    assert_eq!(second.header.crs.epsg, Some(32636));
    // The cached index is the same index, bit for bit.
    assert_eq!(second.index.points(), first.index.points());
    let q = Bounds3::plan([500500.0, 9960500.0], [500900.0, 9960900.0]);
    assert_eq!(second.index.query_box(&q), first.index.query_box(&q));
    assert!(!second.index.query_box(&q).is_empty());
    // Different options invalidate.
    let third = open_indexed(&p, &opts, &IndexOptions { leaf_size: 8, ..Default::default() }).unwrap();
    assert!(matches!(third.cache, CacheStatus::Rebuilt { .. }));
    // A changed source invalidates.
    write_las(&p, 4, 7, true, &known_points(4100), None);
    let fourth = open_indexed(&p, &opts, &IndexOptions { leaf_size: 8, ..Default::default() }).unwrap();
    assert!(matches!(fourth.cache, CacheStatus::Rebuilt { .. }));
    assert_ne!(fourth.source_hash, first.source_hash);
    assert_eq!(fourth.index.len(), 4100);
    // A damaged cache is rebuilt, not trusted.
    let bytes = std::fs::read(&fourth.cache_path).unwrap();
    std::fs::write(&fourth.cache_path, &bytes[..bytes.len() / 2]).unwrap();
    let fifth = open_indexed(&p, &opts, &IndexOptions { leaf_size: 8, ..Default::default() }).unwrap();
    assert!(matches!(fifth.cache, CacheStatus::Rebuilt { .. }));
    assert_eq!(fifth.index.len(), 4100);
    // And the rewritten cache hits again.
    assert_eq!(open_indexed(&p, &opts, &IndexOptions { leaf_size: 8, ..Default::default() }).unwrap().cache, CacheStatus::Hit);
}

#[test]
fn reference_stores_no_points_and_checks_the_source() {
    let dir = temp_dir("ref");
    let p = dir.join("r.las");
    write_las(&p, 2, 1, false, &known_points(500), None);
    let mut r = PointCloudReference::from_file("Survey 2026-11", &p, &SourceOptions::default()).unwrap();
    r.filter = PointFilter { classes: Some(vec![2]), z_range: Some((100.0, 130.0)), ..Default::default() };
    r.display = DisplaySettings::by_class();
    let json = r.to_json().unwrap();
    assert!(json.len() < 20_000, "{}", json.len());
    let back = PointCloudReference::from_json(&json).unwrap();
    assert_eq!(back, r);
    assert_eq!(back.header.point_count, 500);
    assert_eq!(back.check_source().unwrap(), SourceCheck::Unchanged);
    let opened = back.open(&IndexOptions::default()).unwrap();
    assert_eq!(opened.index.len(), 500);
    write_las(&p, 2, 1, false, &known_points(501), None);
    assert_eq!(back.check_source().unwrap(), SourceCheck::Changed);
    assert!(back.open(&IndexOptions::default()).is_err());
    std::fs::remove_file(&p).unwrap();
    assert_eq!(back.check_source().unwrap(), SourceCheck::Missing);
}

#[test]
fn export_las_laz_and_text_keep_coordinates_exact() {
    let dir = temp_dir("export");
    let src = dir.join("s.las");
    let pts = known_points(3000);
    write_las(&src, 2, 3, false, &pts, None);
    let opened = open_indexed(&src, &SourceOptions::default(), &IndexOptions::default()).unwrap();
    let ix = &opened.index;
    // Crop with a polygon and filter to class 2, then export through the same scale and offset.
    let ring = [[500200.0, 9960200.0], [500900.0, 9960300.0], [500600.0, 9961100.0]];
    let in_poly = ix.query_polygon(&ring, None);
    let sel = PointFilter { classes: Some(vec![2]), ..Default::default() }.apply_to(ix.points(), &in_poly);
    assert!(sel.len() > 20);
    for (name, version, laz) in [("o12.las", LasVersion::V1_2, false), ("o14.las", LasVersion::V1_4, false), ("o14.laz", LasVersion::V1_4, true), ("o12.laz", LasVersion::V1_2, true)] {
        let out = dir.join(name);
        let mut o = ExportOptions::like_source(&opened.header, version, laz);
        o.copy_crs_from = None;
        let rep = export_las(&out, ix.points(), Some(&sel), &o).unwrap();
        assert_eq!(rep.points_written as usize, sel.len());
        assert_eq!(rep.max_rounding_error, 0.0, "{name}");
        let back = read_all(&out, &SourceOptions::default()).unwrap();
        let mut want: Vec<[f64; 3]> = sel.iter().map(|&i| ix.points().xyz[i as usize]).collect();
        let mut got = back.xyz.clone();
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, want, "{name}");
        assert!(back.class.iter().all(|&c| c == 2));
        let h = read_header(&out, &SourceOptions::default()).unwrap();
        assert_eq!(h.compressed, laz);
        assert_eq!(h.version, if version == LasVersion::V1_2 { "1.2" } else { "1.4" });
    }
    // An explicit scale: rounding is reported and bounded by half a scale unit.
    let out = dir.join("scaled.las");
    let rep = export_las(&out, ix.points(), Some(&sel), &ExportOptions::with_scale([0.01, 0.01, 0.01], LasVersion::V1_2, false)).unwrap();
    assert!(rep.max_rounding_error <= 0.005 + 1e-9 && rep.max_rounding_error > 0.0);
    assert_eq!(rep.offset[1].fract(), 0.0);
    // No default scale.
    let mut none = ExportOptions::with_scale([1.0; 3], LasVersion::V1_2, false);
    none.scale = None;
    assert!(export_las(&out, ix.points(), Some(&sel), &none).is_err());
    // Text.
    let t = dir.join("o.xyz");
    let n = export_text(&t, ix.points(), Some(&sel), &TextExportOptions { delimiter: " ".into(), decimals: 3, columns: vec![TextRole::X, TextRole::Y, TextRole::Z, TextRole::Intensity], header_line: false, rgb_max: 255.0 }).unwrap();
    assert_eq!(n as usize, sel.len());
    let back = read_all(&t, &SourceOptions::default()).unwrap();
    assert_eq!(back.len(), sel.len());
    let mut a = back.xyz.clone();
    let mut b: Vec<[f64; 3]> = sel.iter().map(|&i| ix.points().xyz[i as usize]).collect();
    a.sort_by(|a, b| a.partial_cmp(b).unwrap());
    b.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (p, q) in a.iter().zip(&b) {
        for k in 0..3 {
            assert!((p[k] - q[k]).abs() < 5e-4);
        }
    }
}

#[test]
fn export_stream_filters_and_clips_without_loading_everything() {
    let dir = temp_dir("stream");
    let src = dir.join("s.laz");
    let pts = known_points(6000);
    write_las(&src, 2, 1, true, &pts, None);
    let mut s = open_source(&src, &SourceOptions::default()).unwrap();
    let header = s.header().clone();
    let clip = ClipRegion::Box(Bounds3::plan([500000.0, 9960000.0], [501000.0, 9961000.0]));
    let filter = PointFilter { classes: Some(vec![2, 5]), ..Default::default() };
    let out = dir.join("o.laz");
    let rep = export_stream(s.as_mut(), &out, &filter, Some(&clip), &ExportOptions::like_source(&header, LasVersion::V1_2, true)).unwrap();
    assert_eq!(rep.points_read, 6000);
    let want: Vec<[f64; 3]> = pts.iter().filter(|k| [2, 5].contains(&k.class) && clip.contains(k.xyz())).map(|k| k.xyz()).collect();
    assert_eq!(rep.points_written as usize, want.len());
    assert!(!want.is_empty());
    let back = read_all(&out, &SourceOptions::default()).unwrap();
    assert_eq!(back.xyz, want);
    assert_eq!(rep.max_rounding_error, 0.0);
    // A text source has no scale: the export refuses without one.
    let t = dir.join("t.xyz");
    std::fs::write(&t, "1 2 3\n4 5 6\n").unwrap();
    let mut ts = open_source(&t, &SourceOptions::default()).unwrap();
    let th = ts.header().clone();
    assert!(export_stream(ts.as_mut(), &dir.join("x.las"), &PointFilter::default(), None, &ExportOptions::like_source(&th, LasVersion::V1_2, false)).is_err());
}

#[test]
fn memory_per_point_is_as_documented() {
    let n = 200_000;
    let ix = PointIndex::build(synthetic(n, 5), &IndexOptions::default()).unwrap();
    // 24 position + 2 intensity + 1 class + 1 returns + 8 key.
    assert_eq!(ix.memory_bytes(), n * 36);
}

/// Ten million points: run with `cargo test --release -p afrobim-pointcloud -- --ignored`. The
/// figures in the README come from the `bench` example, which also reports peak memory.
#[test]
#[ignore]
fn ten_million_points_index() {
    let n = 10_000_000;
    let t = std::time::Instant::now();
    let ix = PointIndex::build(synthetic(n, 11), &IndexOptions::default()).unwrap();
    eprintln!("indexed {n} points in {:.2} s (generation included), {} MB held", t.elapsed().as_secs_f64(), ix.memory_bytes() / 1_000_000);
    assert_eq!(ix.memory_bytes(), n * 36);
    let lod = ix.lod(&Bounds3::plan([500000.0, 9960000.0], [500200.0, 9960200.0]), 100_000);
    assert_eq!(lod.indices.len(), 100_000);
}
