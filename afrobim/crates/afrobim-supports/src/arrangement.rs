// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Where the posts of a sign stand and how long they are: plain geometry from entered values.

use serde::{Deserialize, Serialize};

use crate::SupportError;

/// How a sign is carried: every value is entered by the user.
///
/// This is bookkeeping, not design. It makes no claim that the number of posts, their spacing
/// or the mounting height is adequate for the sign or complies with any standard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupportArrangement {
    /// Height of the sign's lower edge above ground level, mm.
    pub mounting_height_mm: f64,
    /// Number of posts, at least one.
    pub post_count: u32,
    /// Distance from each side edge of the sign to the axis of the outermost post, mm. Used
    /// with two or more posts; a single post stands on the sign's centre line whatever this is.
    pub edge_inset_mm: f64,
    /// Level of the post top relative to the sign's upper edge, mm: positive above it, negative
    /// below it, zero flush with it.
    #[serde(default)]
    pub top_offset_mm: f64,
    /// Distance of the post axis behind the plane of the sign face (the sign's +Y), mm. Zero
    /// puts the axis in the plane of the face.
    #[serde(default)]
    pub setback_mm: f64,
    /// True for one foundation under all the posts, centred on the sign; false for one under
    /// each post.
    #[serde(default)]
    pub shared_foundation: bool,
}

impl SupportArrangement {
    /// Posts up to the top of the sign, axes in the plane of the face.
    pub fn new(mounting_height_mm: f64, post_count: u32, edge_inset_mm: f64) -> Self {
        SupportArrangement { mounting_height_mm, post_count, edge_inset_mm, top_offset_mm: 0.0, setback_mm: 0.0, shared_foundation: false }
    }

    /// Checks that the arrangement can be built on a sign of the given size.
    pub fn check(&self, sign_width_mm: f64, sign_height_mm: f64) -> Result<(), SupportError> {
        let bad = |m: String| Err(SupportError::InvalidArrangement(m));
        if !(sign_width_mm.is_finite() && sign_width_mm > 0.0 && sign_height_mm.is_finite() && sign_height_mm > 0.0) {
            return bad(format!("the sign size {sign_width_mm} × {sign_height_mm} mm is not positive"));
        }
        if self.post_count == 0 {
            return bad("there must be at least one post".into());
        }
        if !(self.mounting_height_mm.is_finite() && self.mounting_height_mm >= 0.0) {
            return bad(format!("the mounting height must not be negative (it is {})", self.mounting_height_mm));
        }
        if !self.top_offset_mm.is_finite() || !self.setback_mm.is_finite() {
            return bad("the top offset and the setback must be numbers".into());
        }
        if self.post_count >= 2 {
            if !(self.edge_inset_mm.is_finite() && self.edge_inset_mm >= 0.0) {
                return bad(format!("the edge inset must not be negative (it is {})", self.edge_inset_mm));
            }
            if 2.0 * self.edge_inset_mm >= sign_width_mm {
                return bad(format!("an edge inset of {} mm on each side leaves no room on a sign {} mm wide", self.edge_inset_mm, sign_width_mm));
            }
        }
        if self.post_top_above_ground_mm(sign_height_mm) <= 0.0 {
            return bad(format!("a top offset of {} mm puts the post top at or below ground", self.top_offset_mm));
        }
        Ok(())
    }

    /// The position of each post axis along the sign's width, measured from the sign's
    /// lower-left corner, mm, left to right.
    ///
    /// * one post: `width / 2`;
    /// * `n ≥ 2` posts: evenly spaced from `inset` to `width − inset`, i.e.
    ///   `x_i = inset + i · (width − 2·inset) / (n − 1)` for `i = 0 … n − 1`.
    pub fn post_positions(&self, sign_width_mm: f64) -> Vec<f64> {
        match self.post_count {
            0 => Vec::new(),
            1 => vec![sign_width_mm / 2.0],
            n => {
                let step = (sign_width_mm - 2.0 * self.edge_inset_mm) / f64::from(n - 1);
                (0..n).map(|i| self.edge_inset_mm + f64::from(i) * step).collect()
            }
        }
    }

    /// Height of the post top above ground, mm: mounting height + sign height + top offset.
    pub fn post_top_above_ground_mm(&self, sign_height_mm: f64) -> f64 {
        self.mounting_height_mm + sign_height_mm + self.top_offset_mm
    }

    /// Total post length, mm: embedment + mounting height + sign height + top offset.
    pub fn post_length_mm(&self, sign_height_mm: f64, embedment_mm: f64) -> f64 {
        embedment_mm + self.post_top_above_ground_mm(sign_height_mm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_post_stands_on_the_centre_line() {
        // Sign 900 wide: 900 / 2 = 450, whatever the inset.
        assert_eq!(SupportArrangement::new(2000.0, 1, 0.0).post_positions(900.0), [450.0]);
        assert_eq!(SupportArrangement::new(2000.0, 1, 300.0).post_positions(900.0), [450.0]);
    }

    #[test]
    fn two_posts_stand_at_the_insets() {
        // Sign 2400 wide, inset 300: 300 and 2400 − 300 = 2100.
        assert_eq!(SupportArrangement::new(2000.0, 2, 300.0).post_positions(2400.0), [300.0, 2100.0]);
        // Inset zero puts the axes on the edges.
        assert_eq!(SupportArrangement::new(2000.0, 2, 0.0).post_positions(2400.0), [0.0, 2400.0]);
    }

    #[test]
    fn three_posts_are_evenly_spaced_between_the_insets() {
        // Sign 3600 wide, inset 300: span 3000, step 1500 → 300, 1800, 3300.
        assert_eq!(SupportArrangement::new(2000.0, 3, 300.0).post_positions(3600.0), [300.0, 1800.0, 3300.0]);
        // Four posts on 4000 with inset 500: span 3000, step 1000 → 500, 1500, 2500, 3500.
        assert_eq!(SupportArrangement::new(2000.0, 4, 500.0).post_positions(4000.0), [500.0, 1500.0, 2500.0, 3500.0]);
    }

    #[test]
    fn post_length_builds_up_from_its_parts() {
        let mut a = SupportArrangement::new(2100.0, 2, 300.0);
        // 750 embedment + 2100 mounting + 1200 sign = 4050.
        assert_eq!(a.post_length_mm(1200.0, 750.0), 4050.0);
        assert_eq!(a.post_top_above_ground_mm(1200.0), 3300.0);
        // Post stopping 100 below the top of the sign: 4050 − 100 = 3950.
        a.top_offset_mm = -100.0;
        assert_eq!(a.post_length_mm(1200.0, 750.0), 3950.0);
        // Post 250 proud of the sign: 4050 + 250 = 4300.
        a.top_offset_mm = 250.0;
        assert_eq!(a.post_length_mm(1200.0, 750.0), 4300.0);
    }

    #[test]
    fn impossible_arrangements_are_refused() {
        let ok = SupportArrangement::new(2000.0, 2, 300.0);
        assert!(ok.check(2400.0, 1200.0).is_ok());
        let bad = |a: &SupportArrangement, w: f64, h: f64| matches!(a.check(w, h), Err(SupportError::InvalidArrangement(_)));
        assert!(bad(&ok, 0.0, 1200.0));
        assert!(bad(&ok, 2400.0, f64::NAN));
        assert!(bad(&SupportArrangement { post_count: 0, ..ok.clone() }, 2400.0, 1200.0));
        assert!(bad(&SupportArrangement { mounting_height_mm: -1.0, ..ok.clone() }, 2400.0, 1200.0));
        assert!(bad(&SupportArrangement { edge_inset_mm: -1.0, ..ok.clone() }, 2400.0, 1200.0));
        // Insets meeting in the middle: 2 × 1200 = 2400.
        assert!(bad(&SupportArrangement { edge_inset_mm: 1200.0, ..ok.clone() }, 2400.0, 1200.0));
        // Top offset that takes the post top to ground: 2000 + 1200 − 3200 = 0.
        assert!(bad(&SupportArrangement { top_offset_mm: -3200.0, ..ok.clone() }, 2400.0, 1200.0));
        assert!(bad(&SupportArrangement { setback_mm: f64::INFINITY, ..ok.clone() }, 2400.0, 1200.0));
        // The inset is not used by a single post, so a silly one does not matter.
        assert!(SupportArrangement::new(2000.0, 1, 99_999.0).check(900.0, 900.0).is_ok());
        assert!(SupportArrangement { post_count: 0, ..ok }.post_positions(2400.0).is_empty());
    }

    #[test]
    fn serde_round_trip_with_defaults() {
        let a: SupportArrangement = serde_json::from_str(r#"{"mounting_height_mm":2000,"post_count":2,"edge_inset_mm":300}"#).unwrap();
        assert_eq!(a, SupportArrangement::new(2000.0, 2, 300.0));
        let b = SupportArrangement { top_offset_mm: -50.0, setback_mm: 80.0, ..a };
        assert_eq!(serde_json::from_str::<SupportArrangement>(&serde_json::to_string(&b).unwrap()).unwrap(), b);
    }
}
