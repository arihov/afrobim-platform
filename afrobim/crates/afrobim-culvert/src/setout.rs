// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Setting-out points of the end structures in the project coordinate system.
//!
//! Points 1 and 2 are the centres of the outer faces of the left and right headwalls on the
//! culvert axis. Points 3 and 4 are the outer corners of the left wing ends (3 on the left of the
//! axis looking from point 1 to point 2, 4 on the right); points 5 and 6 are the outer corners of
//! the right wing ends (5 on the right, 6 on the left). This reproduces the numbering of the
//! Km 39+136 drawing.

use serde::{Deserialize, Serialize};

use crate::layout::Layout;
use crate::model::SettingOutInput;

/// One setting-out point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingOutPoint {
    pub number: u8,
    pub easting_m: f64,
    pub northing_m: f64,
    pub description: String,
}

/// Bearing of the culvert axis from the left end to the right end, degrees clockwise from north:
/// the road bearing plus the skew (90 degrees for a square crossing).
pub fn axis_bearing_deg(road_bearing_deg: f64, skew_deg: f64) -> f64 {
    (road_bearing_deg + skew_deg).rem_euclid(360.0)
}

/// Coordinates of a point `s_mm` along the axis and `t_mm` across it (positive to the left of the
/// direction left end to right end), from the culvert centreline point.
pub fn axis_point(origin: &SettingOutInput, skew_deg: f64, s_mm: f64, t_mm: f64) -> (f64, f64) {
    let a = axis_bearing_deg(origin.road_bearing_deg, skew_deg).to_radians();
    let (ue, un) = (a.sin(), a.cos());
    let (ne, nn) = (-un, ue);
    let (s, t) = (s_mm / 1000.0, t_mm / 1000.0);
    (
        origin.cl_easting_m + s * ue + t * ne,
        origin.cl_northing_m + s * un + t * nn,
    )
}

/// Points 1 to 6.
pub fn setting_out(layout: &Layout, origin: &SettingOutInput) -> Vec<SettingOutPoint> {
    let pt = |number: u8, s: f64, t: f64, description: &str| {
        let (easting_m, northing_m) = axis_point(origin, layout.skew_deg, s, t);
        SettingOutPoint {
            number,
            easting_m,
            northing_m,
            description: description.to_string(),
        }
    };
    let (l, r) = (&layout.left, &layout.right);
    let (sl, sr) = (-layout.left_len_mm, layout.right_len_mm);
    vec![
        pt(1, sl, 0.0, "centre of the outer face of the left headwall"),
        pt(2, sr, 0.0, "centre of the outer face of the right headwall"),
        pt(
            3,
            sl - l.wing_outer_corner_o_mm,
            l.wing_outer_corner_t_mm,
            "outer corner of the left wing end, left of the axis",
        ),
        pt(
            4,
            sl - l.wing_outer_corner_o_mm,
            -l.wing_outer_corner_t_mm,
            "outer corner of the left wing end, right of the axis",
        ),
        pt(
            5,
            sr + r.wing_outer_corner_o_mm,
            -r.wing_outer_corner_t_mm,
            "outer corner of the right wing end, right of the axis",
        ),
        pt(
            6,
            sr + r.wing_outer_corner_o_mm,
            r.wing_outer_corner_t_mm,
            "outer corner of the right wing end, left of the axis",
        ),
    ]
}
