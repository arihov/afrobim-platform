// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The in-memory point store: parallel columns (structure of arrays).
//!
//! Coordinates are `f64` exactly as the file gives them after its scale and offset (true UTM
//! magnitudes, never recentred). Memory per point: 24 bytes of position, 2 intensity, 1 class,
//! 1 return byte (return number in the low nibble, number of returns in the high nibble), plus
//! 6 bytes when colour is present and 8 when GPS time is present.

use serde::{Deserialize, Serialize};

/// One point as a value, for pushing and reading single points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct PointRecord {
    pub xyz: [f64; 3],
    pub intensity: u16,
    /// Classification byte as stored (0 to 255). See [`crate::classes`] for the meanings.
    pub class: u8,
    pub return_number: u8,
    pub number_of_returns: u8,
    /// 16-bit red, green, blue.
    pub rgb: Option<[u16; 3]>,
    pub gps_time: Option<f64>,
}

/// Columns of points. All columns have the same length; `rgb` and `gps_time` are `None` when the
/// source has none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PointBuffer {
    pub xyz: Vec<[f64; 3]>,
    pub intensity: Vec<u16>,
    pub class: Vec<u8>,
    /// Return number (low nibble) and number of returns (high nibble).
    pub returns: Vec<u8>,
    pub rgb: Option<Vec<[u16; 3]>>,
    pub gps_time: Option<Vec<f64>>,
}

impl PointBuffer {
    pub fn new(with_rgb: bool, with_gps_time: bool) -> PointBuffer {
        PointBuffer { rgb: with_rgb.then(Vec::new), gps_time: with_gps_time.then(Vec::new), ..Default::default() }
    }

    pub fn with_capacity(n: usize, with_rgb: bool, with_gps_time: bool) -> PointBuffer {
        PointBuffer {
            xyz: Vec::with_capacity(n),
            intensity: Vec::with_capacity(n),
            class: Vec::with_capacity(n),
            returns: Vec::with_capacity(n),
            rgb: with_rgb.then(|| Vec::with_capacity(n)),
            gps_time: with_gps_time.then(|| Vec::with_capacity(n)),
        }
    }

    pub fn len(&self) -> usize {
        self.xyz.len()
    }

    pub fn is_empty(&self) -> bool {
        self.xyz.is_empty()
    }

    pub fn clear(&mut self) {
        self.xyz.clear();
        self.intensity.clear();
        self.class.clear();
        self.returns.clear();
        if let Some(v) = &mut self.rgb {
            v.clear();
        }
        if let Some(v) = &mut self.gps_time {
            v.clear();
        }
    }

    /// Appends a point. Colour and GPS time are dropped if the buffer has no such column and
    /// filled with zero if the buffer has the column and the record has no value.
    pub fn push(&mut self, p: &PointRecord) {
        self.xyz.push(p.xyz);
        self.intensity.push(p.intensity);
        self.class.push(p.class);
        self.returns.push((p.return_number & 15) | ((p.number_of_returns & 15) << 4));
        if let Some(v) = &mut self.rgb {
            v.push(p.rgb.unwrap_or([0; 3]));
        }
        if let Some(v) = &mut self.gps_time {
            v.push(p.gps_time.unwrap_or(0.0));
        }
    }

    /// Copies point `i` of `other` (same column layout is not required; see [`PointBuffer::push`]).
    pub fn push_from(&mut self, other: &PointBuffer, i: usize) {
        self.push(&other.get(i));
    }

    /// Point `i` as a value. Panics if out of range.
    pub fn get(&self, i: usize) -> PointRecord {
        PointRecord {
            xyz: self.xyz[i],
            intensity: self.intensity[i],
            class: self.class[i],
            return_number: self.returns[i] & 15,
            number_of_returns: self.returns[i] >> 4,
            rgb: self.rgb.as_ref().map(|v| v[i]),
            gps_time: self.gps_time.as_ref().map(|v| v[i]),
        }
    }

    pub fn return_number(&self, i: usize) -> u8 {
        self.returns[i] & 15
    }

    pub fn number_of_returns(&self, i: usize) -> u8 {
        self.returns[i] >> 4
    }

    /// A new buffer with the points at `indices`, in that order.
    pub fn gather(&self, indices: &[u32]) -> PointBuffer {
        PointBuffer {
            xyz: indices.iter().map(|&i| self.xyz[i as usize]).collect(),
            intensity: indices.iter().map(|&i| self.intensity[i as usize]).collect(),
            class: indices.iter().map(|&i| self.class[i as usize]).collect(),
            returns: indices.iter().map(|&i| self.returns[i as usize]).collect(),
            rgb: self.rgb.as_ref().map(|v| indices.iter().map(|&i| v[i as usize]).collect()),
            gps_time: self.gps_time.as_ref().map(|v| indices.iter().map(|&i| v[i as usize]).collect()),
        }
    }

    /// Appends all points of `other` (columns missing here are not created).
    pub fn extend_from(&mut self, other: &PointBuffer) {
        self.xyz.extend_from_slice(&other.xyz);
        self.intensity.extend_from_slice(&other.intensity);
        self.class.extend_from_slice(&other.class);
        self.returns.extend_from_slice(&other.returns);
        if let Some(v) = &mut self.rgb {
            match &other.rgb {
                Some(o) => v.extend_from_slice(o),
                None => v.resize(self.xyz.len(), [0; 3]),
            }
        }
        if let Some(v) = &mut self.gps_time {
            match &other.gps_time {
                Some(o) => v.extend_from_slice(o),
                None => v.resize(self.xyz.len(), 0.0),
            }
        }
    }

    /// Bytes of heap used by the columns (by length, not capacity).
    pub fn bytes(&self) -> usize {
        self.len() * (24 + 2 + 1 + 1) + self.rgb.as_ref().map_or(0, |v| v.len() * 6) + self.gps_time.as_ref().map_or(0, |v| v.len() * 8)
    }

    /// Checks that all columns have the same length.
    pub fn is_consistent(&self) -> bool {
        let n = self.xyz.len();
        self.intensity.len() == n
            && self.class.len() == n
            && self.returns.len() == n
            && self.rgb.as_ref().is_none_or(|v| v.len() == n)
            && self.gps_time.as_ref().is_none_or(|v| v.len() == n)
    }
}

/// A box in three dimensions, true coordinates, metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds3 {
    pub const EMPTY: Bounds3 = Bounds3 { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] };

    pub fn new(min: [f64; 3], max: [f64; 3]) -> Bounds3 {
        Bounds3 { min, max }
    }

    /// A plan box with no limit in z.
    pub fn plan(min: [f64; 2], max: [f64; 2]) -> Bounds3 {
        Bounds3 { min: [min[0], min[1], f64::NEG_INFINITY], max: [max[0], max[1], f64::INFINITY] }
    }

    pub fn is_empty(&self) -> bool {
        (0..3).any(|k| self.min[k] > self.max[k])
    }

    pub fn include(&mut self, p: [f64; 3]) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(p[k]);
            self.max[k] = self.max[k].max(p[k]);
        }
    }

    pub fn contains(&self, p: [f64; 3]) -> bool {
        (0..3).all(|k| p[k] >= self.min[k] && p[k] <= self.max[k])
    }

    pub fn intersects(&self, o: &Bounds3) -> bool {
        (0..3).all(|k| self.min[k] <= o.max[k] && self.max[k] >= o.min[k])
    }

    pub fn of(points: &[[f64; 3]]) -> Bounds3 {
        let mut b = Bounds3::EMPTY;
        for &p in points {
            b.include(p);
        }
        b
    }
}
