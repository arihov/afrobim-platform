//! Open-channel hydraulics that belong to no design basis: section geometry, Manning's
//! equation, normal depth, the Froude number and critical depth in a pipe.
//!
//! Every coefficient is an argument. The only constant is standard gravity. Each basis module
//! ([`super::mowt`], [`super::dmrb`]) says where its own documents state the same relations.

use std::f64::consts::PI;

use crate::{positive, DrainageError};

/// Acceleration due to gravity used in the Froude number and the critical depth, m/s².
/// (The MoWT manual's Appendix 10.4 uses 9.81; its culvert velocity heads use 9.8, see
/// [`super::mowt::GRAVITY_CULVERT_M_S2`].)
pub const GRAVITY_M_S2: f64 = 9.81;

/// The trapezoid family of open-channel sections, metres: bed width `b` and side slopes `zl`,
/// `zr` (horizontal to one vertical). `b = 0` is a V; `zl = zr = 0` is a rectangle.
///
/// At a flow depth `y`:
///
/// ```text
/// A = y·(b + (zl + zr)·y / 2)
/// P = b + y·(√(1 + zl²) + √(1 + zr²))
/// T = b + (zl + zr)·y
/// R = A / P            hydraulic depth = A / T
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelGeometry {
    pub bottom_width_m: f64,
    pub left_side_slope: f64,
    pub right_side_slope: f64,
}

impl ChannelGeometry {
    pub fn trapezoid(bottom_width_m: f64, side_slope: f64) -> Self {
        ChannelGeometry { bottom_width_m, left_side_slope: side_slope, right_side_slope: side_slope }
    }

    pub fn rectangle(width_m: f64) -> Self {
        ChannelGeometry { bottom_width_m: width_m, left_side_slope: 0.0, right_side_slope: 0.0 }
    }

    pub fn v(left_side_slope: f64, right_side_slope: f64) -> Self {
        ChannelGeometry { bottom_width_m: 0.0, left_side_slope, right_side_slope }
    }

    pub(crate) fn valid(&self) -> bool {
        let (b, zl, zr) = (self.bottom_width_m, self.left_side_slope, self.right_side_slope);
        b.is_finite() && zl.is_finite() && zr.is_finite() && b >= 0.0 && zl >= 0.0 && zr >= 0.0 && (b > 0.0 || zl + zr > 0.0)
    }

    /// Flow area, m².
    pub fn area_m2(&self, depth_m: f64) -> f64 {
        depth_m * (self.bottom_width_m + (self.left_side_slope + self.right_side_slope) * depth_m / 2.0)
    }

    /// Wetted perimeter, m.
    pub fn wetted_perimeter_m(&self, depth_m: f64) -> f64 {
        self.bottom_width_m + depth_m * (self.left_side_slope.hypot(1.0) + self.right_side_slope.hypot(1.0))
    }

    /// Width of the water surface, m.
    pub fn top_width_m(&self, depth_m: f64) -> f64 {
        self.bottom_width_m + (self.left_side_slope + self.right_side_slope) * depth_m
    }

    /// Hydraulic radius R = A / P, m.
    pub fn hydraulic_radius_m(&self, depth_m: f64) -> f64 {
        self.area_m2(depth_m) / self.wetted_perimeter_m(depth_m)
    }

    /// Hydraulic depth A / T, m.
    pub fn hydraulic_depth_m(&self, depth_m: f64) -> f64 {
        self.area_m2(depth_m) / self.top_width_m(depth_m)
    }

    /// Discharge at uniform flow of the given depth by Manning's equation, m³/s.
    pub fn discharge_m3_s(&self, depth_m: f64, manning_n: f64, gradient_m_per_m: f64) -> f64 {
        if depth_m <= 0.0 {
            return 0.0;
        }
        self.area_m2(depth_m) * manning_velocity_m_s(manning_n, self.hydraulic_radius_m(depth_m), gradient_m_per_m)
    }
}

/// Mean velocity of uniform flow by Manning's equation, m/s: `V = (1/n)·R^(2/3)·S^(1/2)`.
///
/// MoWT: the form used in the Drainage Design Manual's Example 4-1 (which prints the exponent
/// as 0.67), Section 9.5.1 Step 12 and Appendix 10.4 (Col. 8). DMRB: CD 521 Equation 5.25.
pub fn manning_velocity_m_s(manning_n: f64, hydraulic_radius_m: f64, gradient_m_per_m: f64) -> f64 {
    hydraulic_radius_m.powf(2.0 / 3.0) * gradient_m_per_m.sqrt() / manning_n
}

/// Froude number `F = V / √(g·D)` on the hydraulic depth D = A/T, with g = 9.81.
///
/// MoWT Appendix 10.4 (Col. 11) writes `F = V / √(g·y)` with `y` the water depth of a river
/// section; the hydraulic depth is the water depth for a rectangle and the usual generalisation
/// for other shapes.
pub fn froude_number(velocity_m_s: f64, hydraulic_depth_m: f64) -> f64 {
    velocity_m_s / (GRAVITY_M_S2 * hydraulic_depth_m).sqrt()
}

/// The depth of uniform flow for a discharge, m, by bisection on Manning's equation. Discharge
/// rises with depth for every section of the family, so the root is unique. The section is
/// taken to continue upward as far as needed; compare the result with the drain's depth.
pub fn normal_depth_m(geometry: &ChannelGeometry, manning_n: f64, gradient_m_per_m: f64, flow_m3_s: f64) -> Result<f64, DrainageError> {
    let invalid = |m: String| Err(DrainageError::InvalidInput(m));
    if !geometry.valid() {
        return invalid("the channel section needs a bed width or a side slope, none of them negative".to_string());
    }
    if !positive(manning_n) || !positive(gradient_m_per_m) {
        return invalid(format!("Manning's n and the gradient must be greater than zero (got n = {manning_n}, S = {gradient_m_per_m})"));
    }
    if !(flow_m3_s.is_finite() && flow_m3_s >= 0.0) {
        return invalid(format!("the flow must not be negative (got {flow_m3_s})"));
    }
    if flow_m3_s == 0.0 {
        return Ok(0.0);
    }
    let q = |y: f64| geometry.discharge_m3_s(y, manning_n, gradient_m_per_m);
    let (mut lo, mut hi) = (0.0, 1.0);
    while q(hi) < flow_m3_s {
        lo = hi;
        hi *= 2.0;
        if hi > 1.0e6 {
            return invalid("no normal depth was found below 1000 km; check the inputs".to_string());
        }
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if q(mid) < flow_m3_s {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-12 * hi.max(1.0) {
            break;
        }
    }
    Ok(0.5 * (lo + hi))
}

/// Critical depth in a circular pipe, m, from `Q²·T / (g·A³) = 1` by bisection, with
/// `A = (D²/8)(θ − sin θ)` and `T = D·sin(θ/2)` for the wetted angle θ, and g = 9.81.
///
/// The MoWT manual reads this from the chart of its Appendix 9.10; the equation is what such a
/// chart plots, but the two will differ by reading accuracy. The result is always below the
/// diameter.
pub fn circular_critical_depth_m(flow_m3_s: f64, diameter_m: f64) -> f64 {
    if !(flow_m3_s > 0.0) || !(diameter_m > 0.0) {
        return 0.0;
    }
    let d = diameter_m;
    // f(θ) = A³/T − Q²/g rises from −Q²/g at θ = 0 without limit as θ → 2π.
    let f = |theta: f64| {
        let a = d * d / 8.0 * (theta - theta.sin());
        let t = d * (theta / 2.0).sin();
        a.powi(3) / t - flow_m3_s * flow_m3_s / GRAVITY_M_S2
    };
    let (mut lo, mut hi) = (1e-9, 2.0 * PI - 1e-9);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    d / 2.0 * (1.0 - (0.25 * (lo + hi)).cos())
}
