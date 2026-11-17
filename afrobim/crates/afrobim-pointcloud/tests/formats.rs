//! Readers: files with known content written by the tests, read back exactly.

mod common;
use afrobim_pointcloud::*;
use common::*;
use std::io::Write;

const WKT: &str = r#"PROJCS["WGS 84 / UTM zone 36N",GEOGCS["WGS 84",DATUM["WGS_1984",SPHEROID["WGS 84",6378137,298.257223563,AUTHORITY["EPSG","7030"]],AUTHORITY["EPSG","6326"]],PRIMEM["Greenwich",0],UNIT["degree",0.0174532925199433]],PROJECTION["Transverse_Mercator"],UNIT["metre",1],AUTHORITY["EPSG","32636"]]"#;

#[test]
fn las_12_format_3_read_back_exactly() {
    let dir = temp_dir("las12");
    let p = dir.join("a.las");
    let pts = known_points(2000);
    write_las(&p, 2, 3, false, &pts, None);
    // Header facts without loading points.
    let h = read_header(&p, &SourceOptions::default()).unwrap();
    assert_eq!(h.kind, SourceKind::Las);
    assert_eq!(h.version, "1.2");
    assert_eq!(h.point_format, Some(3));
    assert_eq!(h.point_count, 2000);
    assert_eq!(h.scale, Some([SCALE; 3]));
    assert_eq!(h.offset, Some(OFFSET));
    assert!(h.has_rgb && h.has_gps_time && !h.compressed);
    assert_eq!(h.class_table, ClassTable::Legacy);
    let b = h.bounds.unwrap();
    assert!(b.min[1] >= 9960000.0 && b.max[1] <= 9962000.0 + 1e-6);
    // Points, as the file gives them.
    let buf = read_all(&p, &SourceOptions::default()).unwrap();
    assert_matches_known(&buf, &pts, true, true);
    // The header bounds are what the points are.
    let real = Bounds3::of(&buf.xyz);
    assert_eq!(real.min, b.min);
    assert_eq!(real.max, b.max);
}

#[test]
fn las_14_laz_with_wkt_and_epsg() {
    let dir = temp_dir("laz14");
    let pts = known_points(1500);
    for (name, laz) in [("a.las", false), ("a.laz", true)] {
        let p = dir.join(name);
        write_las(&p, 4, 7, laz, &pts, Some(WKT));
        let h = read_header(&p, &SourceOptions::default()).unwrap();
        assert_eq!(h.version, "1.4");
        assert_eq!(h.point_format, Some(if laz { 7 } else { 7 }));
        assert_eq!(h.compressed, laz);
        assert_eq!(h.kind, if laz { SourceKind::Laz } else { SourceKind::Las });
        assert_eq!(h.class_table, ClassTable::Extended);
        assert_eq!(h.crs.epsg, Some(32636));
        assert!(h.crs.epsg_source.as_deref().unwrap().contains("WKT"));
        assert_eq!(h.crs.wkt.as_deref(), Some(WKT));
        let buf = read_all(&p, &SourceOptions::default()).unwrap();
        assert_matches_known(&buf, &pts, true, true);
    }
    // LAZ is smaller than LAS for this data.
    assert!(std::fs::metadata(dir.join("a.laz")).unwrap().len() < std::fs::metadata(dir.join("a.las")).unwrap().len());
}

#[test]
fn geotiff_keys_give_an_epsg_code() {
    use las::{Builder, Vlr, Writer};
    let dir = temp_dir("geotiff");
    let p = dir.join("g.las");
    let mut b = Builder::from(las::Version::new(1, 2));
    // GeoKeyDirectory: header (1, 1, 0, 2 keys), then GTModelType = 1 (projected), ProjectedCRSGeoKey (3072) = 32736.
    let words: [u16; 12] = [1, 1, 0, 2, 1024, 0, 1, 1, 3072, 0, 1, 32736];
    b.vlrs.push(Vlr { user_id: "LASF_Projection".into(), record_id: 34735, description: "GeoKeyDirectoryTag".into(), data: words.iter().flat_map(|w| w.to_le_bytes()).collect() });
    let h = b.into_header().unwrap();
    let mut w = Writer::from_path(&p, h).unwrap();
    w.write_point(las::Point { x: 1.0, y: 2.0, z: 3.0, ..Default::default() }).unwrap();
    w.close().unwrap();
    let h = read_header(&p, &SourceOptions::default()).unwrap();
    assert_eq!(h.crs.epsg, Some(32736));
    assert!(h.crs.epsg_source.unwrap().contains("3072"));
    assert!(h.crs.geotiff_keys.iter().any(|k| k.id == 3072 && k.value == GeoKeyValue::Short(32736)));
    assert!(h.crs.wkt.is_none());
}

#[test]
fn chunked_reading_equals_whole_reading() {
    let dir = temp_dir("chunks");
    let p = dir.join("c.laz");
    let pts = known_points(5000);
    write_las(&p, 2, 1, true, &pts, None);
    let mut src = open_source(&p, &SourceOptions::default()).unwrap();
    let mut got = src.new_buffer(0);
    let mut sizes = Vec::new();
    loop {
        let n = src.read_chunk(777, &mut got).unwrap();
        if n == 0 {
            break;
        }
        sizes.push(n);
    }
    assert_eq!(sizes.iter().sum::<usize>(), 5000);
    assert!(sizes.len() >= 7 && sizes[0] == 777);
    assert_matches_known(&got, &pts, true, false);
    assert!(got.rgb.is_none());
}

#[test]
fn e57_roundtrip_with_colour_and_intensity() {
    let dir = temp_dir("e57");
    let p = dir.join("a.e57");
    let n = 3000;
    {
        let mut w = e57::E57Writer::from_file(&p, "file-guid-1").unwrap();
        w.set_coordinate_metadata(Some(WKT.to_string()));
        let proto = vec![e57::Record::CARTESIAN_X_F64, e57::Record::CARTESIAN_Y_F64, e57::Record::CARTESIAN_Z_F64, e57::Record::INTENSITY_U16, e57::Record::COLOR_RED_U8, e57::Record::COLOR_GREEN_U8, e57::Record::COLOR_BLUE_U8];
        let mut pc = w.add_pointcloud("pc-guid-1", proto).unwrap();
        for i in 0..n {
            let x = 500000.0 + (i % 100) as f64 * 0.125 + 0.001;
            let y = 9960000.0 + (i / 100) as f64 * 0.25 + 0.002;
            let z = 120.0 + (i % 17) as f64 * 0.01;
            pc.add_point(vec![e57::RecordValue::Double(x), e57::RecordValue::Double(y), e57::RecordValue::Double(z), e57::RecordValue::Integer((i * 20) as i64), e57::RecordValue::Integer((i % 256) as i64), e57::RecordValue::Integer(255 - (i % 256) as i64), e57::RecordValue::Integer(7)]).unwrap();
        }
        pc.finalize().unwrap();
        w.finalize().unwrap();
    }
    let h = read_header(&p, &SourceOptions::default()).unwrap();
    assert_eq!(h.kind, SourceKind::E57);
    assert_eq!(h.point_count, n as u64);
    assert!(h.has_rgb && h.has_intensity);
    assert_eq!(h.crs.epsg, Some(32636));
    let b = h.bounds.unwrap();
    assert!((b.min[0] - 500000.001).abs() < 1e-9 && b.min[1] >= 9960000.0);
    let buf = read_all(&p, &SourceOptions::default()).unwrap();
    assert_eq!(buf.len(), n);
    for i in 0..n {
        assert_eq!(buf.xyz[i][0], 500000.0 + (i % 100) as f64 * 0.125 + 0.001, "x {i}");
        assert_eq!(buf.xyz[i][1], 9960000.0 + (i / 100) as f64 * 0.25 + 0.002);
        assert_eq!(buf.xyz[i][2], 120.0 + (i % 17) as f64 * 0.01);
    }
    // Intensity and colour are normalised by the e57 crate and stored as 16-bit fractions.
    let rgb = buf.rgb.as_ref().unwrap();
    assert_eq!(rgb[1][0], (1.0f64 / 255.0 * 65535.0).round() as u16);
    assert_eq!(rgb[0][2], (7.0f64 / 255.0 * 65535.0).round() as u16);
    assert!(buf.intensity[100] > buf.intensity[10]);
}

fn write_text(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::File::create(&p).unwrap().write_all(content.as_bytes()).unwrap();
    p
}

#[test]
fn text_xyz_xyzi_xyzrgb_pts_with_detection() {
    let dir = temp_dir("text");
    // XYZ with a header line and commas.
    let p = write_text(&dir, "a.csv", "X,Y,Z\n500123.456,9960234.567,101.25\n500124.5,9960235.5,101.5\n");
    let det = detect_text_format(&p, 5).unwrap();
    assert_eq!(det.layout, TextLayout::Named);
    assert_eq!(det.preview.len(), 2);
    let buf = read_all(&p, &SourceOptions::default()).unwrap();
    assert_eq!(buf.xyz, vec![[500123.456, 9960234.567, 101.25], [500124.5, 9960235.5, 101.5]]);
    let h = read_header(&p, &SourceOptions::default()).unwrap();
    assert!(h.from_scan && h.point_count == 2);
    assert_eq!(h.bounds.unwrap().max, [500124.5, 9960235.5, 101.5]);

    // XYZ + intensity, spaces, comment line.
    let p = write_text(&dir, "b.xyz", "# scan 1\n1 2 3 100\n4 5 6 200\n");
    let det = detect_text_format(&p, 5).unwrap();
    assert_eq!(det.layout, TextLayout::XyzIntensity);
    let buf = read_all(&p, &SourceOptions::default()).unwrap();
    assert_eq!(buf.intensity, vec![100, 200]);

    // XYZRGB, 8-bit colour, semicolons.
    let p = write_text(&dir, "c.txt", "1;2;3;255;128;0\n4;5;6;0;0;255\n");
    let buf = read_all(&p, &SourceOptions::default()).unwrap();
    assert_eq!(buf.rgb.as_ref().unwrap()[0], [65535, (128.0f64 / 255.0 * 65535.0).round() as u16, 0]);

    // PTS: count line, then x y z i r g b, intensity in -2048..2047 mapped by the caller.
    let p = write_text(&dir, "d.pts", "2\n500000.5 9960000.25 12.5 -2048 10 20 30\n500001.5 9960001.25 12.75 2047 40 50 60\n");
    let det = detect_text_format(&p, 5).unwrap();
    assert_eq!(det.layout, TextLayout::Pts);
    let mut fmt = det.format.clone();
    fmt.intensity_offset = 2048.0;
    let opts = SourceOptions { text: Some(fmt), ..Default::default() };
    let buf = read_all(&p, &opts).unwrap();
    assert_eq!(buf.len(), 2);
    assert_eq!(buf.intensity, vec![0, 4095]);
    assert_eq!(buf.xyz[1], [500001.5, 9960001.25, 12.75]);

    // A bad line names its line number.
    let p = write_text(&dir, "e.xyz", "1 2 3\n4 five 6\n");
    let err = read_all(&p, &SourceOptions { text: Some(TextFormat::xyz()), ..Default::default() }).unwrap_err();
    assert!(err.to_string().contains("line 2"), "{err}");
}

#[test]
fn unknown_files_are_refused() {
    let dir = temp_dir("unknown");
    let p = write_text(&dir, "x.bin", "not a point file");
    assert!(read_header(&p, &SourceOptions::default()).is_err());
}

/// Public sample files from the `las` crate's test data (4.9 kB LAS, 32 kB LAZ), kept in the repository.
#[test]
fn public_sample_files_read() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    for (name, laz) in [("autzen.las", false), ("32-1-472-150-76.laz", true)] {
        let p = dir.join(name);
        let h = read_header(&p, &SourceOptions::default()).unwrap();
        assert_eq!(h.compressed, laz, "{name}");
        assert!(h.point_count > 0);
        let buf = read_all(&p, &SourceOptions::default()).unwrap();
        assert_eq!(buf.len() as u64, h.point_count, "{name}");
        // Every point lies in the stated bounds (with a rounding allowance of one scale unit).
        let b = h.bounds.unwrap();
        let s = h.scale.unwrap();
        for q in &buf.xyz {
            for k in 0..3 {
                assert!(q[k] >= b.min[k] - s[k] && q[k] <= b.max[k] + s[k], "{name} axis {k}");
            }
        }
        // Coordinates are raw * scale + offset, so they sit on the scale lattice.
        let o = h.offset.unwrap();
        for q in buf.xyz.iter().take(50) {
            for k in 0..3 {
                let raw = (q[k] - o[k]) / s[k];
                assert!((raw - raw.round()).abs() < 1e-3, "{name}");
            }
        }
        // And they index.
        let ix = PointIndex::build(buf, &IndexOptions::default()).unwrap();
        assert_eq!(ix.len() as u64, h.point_count);
    }
}

#[test]
fn statistics_accumulate_over_chunks() {
    let dir = temp_dir("statschunks");
    let p = dir.join("s.las");
    let pts = known_points(4321);
    write_las(&p, 2, 1, false, &pts, None);
    let mut src = open_source(&p, &SourceOptions::default()).unwrap();
    let b = src.header().bounds.unwrap();
    let mut grid = DensityGrid::new(b.min[0], b.min[1], b.max[0], b.max[1], 100.0).unwrap();
    let mut classes = ClassHistogram::default();
    let mut elev = Histogram::new(100.0, 1.0, 10).unwrap();
    let mut chunk = src.new_buffer(500);
    loop {
        chunk.clear();
        if src.read_chunk(500, &mut chunk).unwrap() == 0 {
            break;
        }
        grid.add_buffer(&chunk);
        classes.add_buffer(&chunk);
        elev.add_elevations(&chunk);
    }
    assert_eq!(grid.counts.iter().map(|&c| c as u64).sum::<u64>(), 4321);
    assert_eq!(grid.outside, 0);
    assert_eq!(classes.total(), 4321);
    assert_eq!(classes.counts[&2], pts.iter().filter(|k| k.class == 2).count() as u64);
    assert_eq!(elev.total(), 4321);
    assert!(grid.summary().max > 0.0);
}
