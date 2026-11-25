// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Generators for small files with known content.
#![allow(dead_code)]

use afrobim_pointcloud::PointBuffer;
use las::point::{Classification, Format};
use las::{Builder, Color, Point, Transform, Vector, Version, Writer};
use std::path::{Path, PathBuf};

pub const SCALE: f64 = 0.001;
pub const OFFSET: [f64; 3] = [500000.0, 9960000.0, 0.0];

pub fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("afrobim-pc-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The coordinate a LAS reader gives for an integer: `raw * scale + offset`.
pub fn coord(raw: i64, axis: usize) -> f64 {
    raw as f64 * SCALE + OFFSET[axis]
}

/// A known point: raw integers in a 2 km x 2 km area at true UTM-size coordinates.
pub struct Known {
    pub raw: [i64; 3],
    pub intensity: u16,
    pub class: u8,
    pub rn: u8,
    pub nr: u8,
    pub gps: f64,
    pub rgb: [u16; 3],
}

impl Known {
    pub fn xyz(&self) -> [f64; 3] {
        [coord(self.raw[0], 0), coord(self.raw[1], 1), coord(self.raw[2], 2)]
    }
}

/// `n` points on a deterministic pattern.
pub fn known_points(n: usize) -> Vec<Known> {
    let mut s = 12345u64;
    let mut next = move || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) as i64
    };
    (0..n)
        .map(|i| {
            let (a, b) = (next() % 2_000_000, next() % 2_000_000);
            Known {
                raw: [a, b, 100_000 + (a / 40) % 5_000 + i as i64 % 7],
                intensity: (i * 37 % 60000) as u16,
                class: [2u8, 2, 5, 6, 1][i % 5],
                rn: (i % 3) as u8 + 1,
                nr: 3,
                gps: 1000.5 + i as f64 * 0.25,
                rgb: [(i * 3 % 65536) as u16, (i * 5 % 65536) as u16, (i * 11 % 65536) as u16],
            }
        })
        .collect()
}

pub fn write_las(path: &Path, minor: u8, format: u8, laz: bool, pts: &[Known], wkt: Option<&str>) {
    let mut b = Builder::from(Version::new(1, minor));
    b.point_format = Format::new(format).unwrap();
    b.point_format.is_compressed = laz;
    b.transforms = Vector { x: Transform { scale: SCALE, offset: OFFSET[0] }, y: Transform { scale: SCALE, offset: OFFSET[1] }, z: Transform { scale: SCALE, offset: OFFSET[2] } };
    let mut h = b.into_header().unwrap();
    if let Some(w) = wkt {
        h.set_wkt_crs(w.as_bytes().to_vec()).unwrap();
    }
    let f = h.point_format().clone();
    let mut w = Writer::from_path(path, h).unwrap();
    for k in pts {
        let xyz = k.xyz();
        w.write_point(Point {
            x: xyz[0],
            y: xyz[1],
            z: xyz[2],
            intensity: k.intensity,
            return_number: k.rn,
            number_of_returns: k.nr,
            classification: Classification::new(k.class).unwrap(),
            gps_time: f.has_gps_time.then_some(k.gps),
            color: f.has_color.then(|| Color::new(k.rgb[0], k.rgb[1], k.rgb[2])),
            ..Default::default()
        })
        .unwrap();
    }
    w.close().unwrap();
}

pub fn assert_matches_known(buf: &PointBuffer, pts: &[Known], with_gps: bool, with_rgb: bool) {
    assert_eq!(buf.len(), pts.len());
    for (i, k) in pts.iter().enumerate() {
        assert_eq!(buf.xyz[i], k.xyz(), "point {i}");
        assert_eq!(buf.intensity[i], k.intensity);
        assert_eq!(buf.class[i], k.class);
        assert_eq!(buf.return_number(i), k.rn);
        assert_eq!(buf.number_of_returns(i), k.nr);
        if with_gps {
            assert_eq!(buf.gps_time.as_ref().unwrap()[i], k.gps);
        }
        if with_rgb {
            assert_eq!(buf.rgb.as_ref().unwrap()[i], k.rgb);
        }
    }
}

/// Deterministic cloud for index tests: a rough surface over 200 m x 200 m at UTM-size coordinates.
pub fn synthetic(n: usize, seed: u64) -> PointBuffer {
    use afrobim_pointcloud::PointRecord;
    let mut s = seed;
    let mut next = move || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut b = PointBuffer::with_capacity(n, false, false);
    for _ in 0..n {
        let (x, y) = (500000.0 + next() * 200.0, 9960000.0 + next() * 200.0);
        let z = 100.0 + 5.0 * ((x - 500000.0) / 30.0).sin() + 0.05 * next();
        b.push(&PointRecord { xyz: [x, y, z], class: (next() * 5.0) as u8, intensity: (next() * 1000.0) as u16, return_number: 1, number_of_returns: 1, ..Default::default() });
    }
    b
}
