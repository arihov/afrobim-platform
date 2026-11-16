//! Attribute filters: class set, elevation range, intensity range and return selection.
//!
//! Every limit is an input; nothing here has an engineering default. An empty [`PointFilter`]
//! keeps every point.

use crate::buffer::PointBuffer;
use serde::{Deserialize, Serialize};

/// Which returns to keep. Return numbers start at 1. A point whose number of returns is 0
/// (unknown: text files, some E57) matches only [`ReturnFilter::Any`] and [`ReturnFilter::Number`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReturnFilter {
    #[default]
    Any,
    /// Return number 1.
    First,
    /// Return number equals the number of returns.
    Last,
    /// A single return (number of returns 1).
    Only,
    /// First return of a pulse with more than one return.
    FirstOfMany,
    /// Last return of a pulse with more than one return.
    LastOfMany,
    /// Neither first nor last of a pulse.
    Intermediate,
    /// This return number.
    Number(u8),
}

/// A set of conditions; a point must meet all that are set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PointFilter {
    /// Keep only these class codes. `None`: any class.
    #[serde(default)]
    pub classes: Option<Vec<u8>>,
    /// Drop these class codes.
    #[serde(default)]
    pub exclude_classes: Vec<u8>,
    /// Elevation limits, inclusive.
    #[serde(default)]
    pub z_range: Option<(f64, f64)>,
    /// Intensity limits, inclusive.
    #[serde(default)]
    pub intensity_range: Option<(u16, u16)>,
    #[serde(default)]
    pub returns: ReturnFilter,
}

impl PointFilter {
    pub fn is_empty(&self) -> bool {
        *self == PointFilter::default()
    }

    pub fn matches(&self, buf: &PointBuffer, i: usize) -> bool {
        self.compile().matches(buf, i)
    }

    /// A faster form for testing many points.
    pub fn compile(&self) -> CompiledFilter {
        let mut mask = [true; 256];
        if let Some(keep) = &self.classes {
            mask = [false; 256];
            for &c in keep {
                mask[c as usize] = true;
            }
        }
        for &c in &self.exclude_classes {
            mask[c as usize] = false;
        }
        CompiledFilter { mask, z: self.z_range, intensity: self.intensity_range, returns: self.returns }
    }

    /// Indices of the points of `buf` that pass.
    pub fn apply(&self, buf: &PointBuffer) -> Vec<u32> {
        let f = self.compile();
        (0..buf.len()).filter(|&i| f.matches(buf, i)).map(|i| i as u32).collect()
    }

    /// Of the given indices, those that pass.
    pub fn apply_to(&self, buf: &PointBuffer, indices: &[u32]) -> Vec<u32> {
        let f = self.compile();
        indices.iter().copied().filter(|&i| f.matches(buf, i as usize)).collect()
    }
}

/// A [`PointFilter`] prepared for repeated tests.
#[derive(Debug, Clone)]
pub struct CompiledFilter {
    mask: [bool; 256],
    z: Option<(f64, f64)>,
    intensity: Option<(u16, u16)>,
    returns: ReturnFilter,
}

impl CompiledFilter {
    pub fn matches(&self, buf: &PointBuffer, i: usize) -> bool {
        if !self.mask[buf.class[i] as usize] {
            return false;
        }
        if let Some((lo, hi)) = self.z {
            let z = buf.xyz[i][2];
            if z < lo || z > hi {
                return false;
            }
        }
        if let Some((lo, hi)) = self.intensity {
            let v = buf.intensity[i];
            if v < lo || v > hi {
                return false;
            }
        }
        let (r, n) = (buf.return_number(i), buf.number_of_returns(i));
        match self.returns {
            ReturnFilter::Any => true,
            ReturnFilter::Number(k) => r == k,
            _ if n == 0 => false,
            ReturnFilter::First => r == 1,
            ReturnFilter::Last => r == n,
            ReturnFilter::Only => n == 1,
            ReturnFilter::FirstOfMany => r == 1 && n > 1,
            ReturnFilter::LastOfMany => r == n && n > 1,
            ReturnFilter::Intermediate => r > 1 && r < n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::PointRecord;

    fn sample() -> PointBuffer {
        let mut b = PointBuffer::new(false, false);
        for (z, c, i, r, n) in [(1.0, 2u8, 10u16, 1u8, 1u8), (5.0, 5, 200, 1, 3), (6.0, 5, 300, 2, 3), (7.0, 2, 50, 3, 3), (8.0, 1, 60, 0, 0)] {
            b.push(&PointRecord { xyz: [0.0, 0.0, z], class: c, intensity: i, return_number: r, number_of_returns: n, ..Default::default() });
        }
        b
    }

    #[test]
    fn filters_combine() {
        let b = sample();
        assert_eq!(PointFilter::default().apply(&b).len(), 5);
        assert_eq!(PointFilter { classes: Some(vec![2]), ..Default::default() }.apply(&b), vec![0, 3]);
        assert_eq!(PointFilter { exclude_classes: vec![5], ..Default::default() }.apply(&b), vec![0, 3, 4]);
        assert_eq!(PointFilter { z_range: Some((5.0, 7.0)), ..Default::default() }.apply(&b), vec![1, 2, 3]);
        assert_eq!(PointFilter { intensity_range: Some((50, 200)), ..Default::default() }.apply(&b), vec![1, 3, 4]);
        assert_eq!(PointFilter { returns: ReturnFilter::Last, ..Default::default() }.apply(&b), vec![0, 3]);
        assert_eq!(PointFilter { returns: ReturnFilter::Intermediate, ..Default::default() }.apply(&b), vec![2]);
        assert_eq!(PointFilter { returns: ReturnFilter::FirstOfMany, ..Default::default() }.apply(&b), vec![1]);
        // Unknown number of returns matches only Any and Number.
        assert_eq!(PointFilter { returns: ReturnFilter::Number(0), ..Default::default() }.apply(&b), vec![4]);
        let f = PointFilter { classes: Some(vec![5]), z_range: Some((0.0, 5.5)), returns: ReturnFilter::First, ..Default::default() };
        assert_eq!(f.apply(&b), vec![1]);
        assert_eq!(f.apply_to(&b, &[0, 1, 2]), vec![1]);
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<PointFilter>(&json).unwrap(), f);
    }
}
