//! Uniform-flow hydraulics of a pipe: full-bore capacity, partial-flow depth and velocity.
//!
//! # Methods and sources
//!
//! * **Manning's equation**, `V = R^(2/3) S^(1/2) / n`, with the hydraulic radius `R = A / P`.
//!   Implemented by `afrobim_drainage::manning_velocity_m_s`, which cites the MoWT Drainage
//!   Design Manual (Example 4-1, Section 9.5.1 Step 12) and DMRB CD 521 Equation 5.25. Textbook:
//!   V. T. Chow, *Open-Channel Hydraulics*, McGraw-Hill, 1959.
//! * **Colebrook-White**, for a roughness given as an equivalent sand roughness `ks`:
//!   `V = -2 sqrt(2 g D S) log10( ks / (3.7 D) + 2.51 nu / (D sqrt(2 g D S)) )` with `D = 4R`
//!   (`D` is the diameter for a full circular pipe; `4R` is the usual extension to part-full and
//!   non-circular sections). C. F. Colebrook, "Turbulent flow in pipes, with particular reference
//!   to the transition region between the smooth and rough pipe laws", J. Inst. Civil Engineers
//!   11(4), 1939, pp. 133-156; see also Chadwick, Morfett and Borthwick, *Hydraulics in Civil and
//!   Environmental Engineering*. The kinematic viscosity `nu` is an input
//!   ([`HydraulicSettings`]); none is built in.
//! * **Continuity**, `Q = V A`.
//! * **Partial flow in a circular pipe**: the geometry of a circular segment (Chow 1959, geometric
//!   elements of a circular section): with `theta = 2 acos(1 - 2y/D)`, `A = D^2 (theta - sin theta)/8`,
//!   `P = D theta / 2`, `T = D sin(theta/2)`. The depth `y` that carries a given `Q` at the pipe's
//!   slope is found by bisection on `Q(y) = A(y) V(R(y), S)` (uniform flow, Manning or
//!   Colebrook-White, whichever the pipe's roughness states). The same bisection is used for other
//!   shapes, from the section functions of [`PipeShape::section_at`].
//! * **Froude number**, `F = V / sqrt(g A / T)`, from `afrobim_drainage::froude_number`.
//!
//! Chow's book is cited from knowledge of the textbook; no chapter, table or page number is given
//! because none was checked against a held copy.
//!
//! # Limits
//!
//! Uniform flow only: no gradually varied profile, no hydraulic jump, no air entrainment. At the
//! free-surface solution the lower of two possible depths is taken (a circular pipe carries its
//! largest free-surface discharge at a depth below the full depth, so two depths can carry the
//! same discharge near the top). A flow above the full-bore discharge is reported as exceeding
//! capacity, with the velocity `Q / A_full`.

use afrobim_drainage::design::{froude_number, manning_velocity_m_s, GRAVITY_M_S2};
use serde::{Deserialize, Serialize};

use crate::model::{PipeShape, Roughness};

/// Inputs the hydraulics need that belong to no pipe.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct HydraulicSettings {
    /// Kinematic viscosity of the fluid, m2/s. Needed only for Colebrook-White roughness.
    pub kinematic_viscosity_m2_s: Option<f64>,
}

/// Mean velocity of uniform flow for a hydraulic radius and a bed slope, m/s.
pub fn uniform_velocity_m_s(roughness: &Roughness, hydraulic_radius_m: f64, slope: f64, settings: &HydraulicSettings) -> Result<f64, String> {
    if !(hydraulic_radius_m > 0.0) || !(slope > 0.0) || !slope.is_finite() {
        return Err("uniform flow needs a positive hydraulic radius and a positive slope".to_string());
    }
    match *roughness {
        Roughness::Manning { n } => {
            if !(n > 0.0) || !n.is_finite() {
                return Err("Manning's n must be greater than zero".to_string());
            }
            Ok(manning_velocity_m_s(n, hydraulic_radius_m, slope))
        }
        Roughness::ColebrookWhite { ks_m } => {
            if !(ks_m >= 0.0) || !ks_m.is_finite() {
                return Err("the Colebrook-White roughness ks must not be negative".to_string());
            }
            let nu = settings.kinematic_viscosity_m2_s.filter(|v| *v > 0.0).ok_or_else(|| "Colebrook-White needs the kinematic viscosity as an input".to_string())?;
            let d = 4.0 * hydraulic_radius_m;
            let root = (2.0 * GRAVITY_M_S2 * d * slope).sqrt();
            Ok(-2.0 * root * (ks_m / (3.7 * d) + 2.51 * nu / (d * root)).log10())
        }
    }
}

/// Friction slope `Sf` (head lost per metre of pipe) for a given mean velocity and hydraulic
/// radius. Manning: `Sf = (n V / R^(2/3))^2`. Colebrook-White: the Darcy-Weisbach relation
/// `Sf = f V^2 / (2 g D)` with `D = 4R` and `f` from `1/sqrt(f) = -2 log10(ks/(3.7 D) + 2.51/(Re sqrt(f)))`,
/// `Re = V D / nu`, solved by iteration. These are the same laws as [`uniform_velocity_m_s`].
pub fn friction_slope(roughness: &Roughness, velocity_m_s: f64, hydraulic_radius_m: f64, settings: &HydraulicSettings) -> Result<f64, String> {
    if !(hydraulic_radius_m > 0.0) {
        return Err("the hydraulic radius must be greater than zero".to_string());
    }
    if velocity_m_s == 0.0 {
        return Ok(0.0);
    }
    match *roughness {
        Roughness::Manning { n } => {
            if !(n > 0.0) {
                return Err("Manning's n must be greater than zero".to_string());
            }
            Ok((n * velocity_m_s / hydraulic_radius_m.powf(2.0 / 3.0)).powi(2))
        }
        Roughness::ColebrookWhite { ks_m } => {
            let nu = settings.kinematic_viscosity_m2_s.filter(|v| *v > 0.0).ok_or_else(|| "Colebrook-White needs the kinematic viscosity as an input".to_string())?;
            let d = 4.0 * hydraulic_radius_m;
            let re = velocity_m_s.abs() * d / nu;
            let mut f: f64 = 0.02;
            for _ in 0..100 {
                let inv = -2.0 * (ks_m / (3.7 * d) + 2.51 / (re * f.sqrt())).log10();
                let next = 1.0 / (inv * inv);
                if (next - f).abs() < 1e-14 {
                    f = next;
                    break;
                }
                f = next;
            }
            Ok(f * velocity_m_s * velocity_m_s / (2.0 * GRAVITY_M_S2 * d))
        }
    }
}

/// Full-bore results of a pipe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FullFlow {
    pub area_m2: f64,
    pub wetted_perimeter_m: f64,
    pub hydraulic_radius_m: f64,
    pub velocity_m_s: f64,
    pub discharge_m3_s: f64,
}

/// Full-bore uniform flow at a slope.
pub fn full_flow(shape: &PipeShape, roughness: &Roughness, slope: f64, settings: &HydraulicSettings) -> Result<FullFlow, String> {
    let s = shape.section_at(shape.height_m()).ok_or_else(|| "the section has no usable profile".to_string())?;
    let r = s.area_m2 / s.wetted_perimeter_m;
    let v = uniform_velocity_m_s(roughness, r, slope, settings)?;
    Ok(FullFlow { area_m2: s.area_m2, wetted_perimeter_m: s.wetted_perimeter_m, hydraulic_radius_m: r, velocity_m_s: v, discharge_m3_s: v * s.area_m2 })
}

/// Result of uniform flow of a given discharge in a pipe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalFlow {
    pub depth_m: f64,
    /// Depth over inside height.
    pub depth_ratio: f64,
    pub area_m2: f64,
    pub velocity_m_s: f64,
    /// `None` when the pipe is full or the flow is zero.
    pub froude: Option<f64>,
    pub full_discharge_m3_s: f64,
    pub full_velocity_m_s: f64,
    /// The discharge exceeds the full-bore discharge: depth is the full height and the velocity
    /// is `Q / A_full`, not a uniform-flow value.
    pub exceeds_capacity: bool,
}

fn discharge_at(shape: &PipeShape, roughness: &Roughness, slope: f64, y: f64, settings: &HydraulicSettings) -> Result<f64, String> {
    let s = shape.section_at(y).ok_or_else(|| "the section has no usable profile".to_string())?;
    if s.area_m2 <= 0.0 {
        return Ok(0.0);
    }
    let v = uniform_velocity_m_s(roughness, s.area_m2 / s.wetted_perimeter_m, slope, settings)?;
    Ok(v * s.area_m2)
}

/// The uniform-flow depth and velocity for a discharge. See the module documentation.
pub fn normal_flow(shape: &PipeShape, roughness: &Roughness, slope: f64, discharge_m3_s: f64, settings: &HydraulicSettings) -> Result<NormalFlow, String> {
    if !(discharge_m3_s >= 0.0) || !discharge_m3_s.is_finite() {
        return Err("the flow must not be negative".to_string());
    }
    let full = full_flow(shape, roughness, slope, settings)?;
    let h = shape.height_m();
    let base = NormalFlow {
        depth_m: 0.0,
        depth_ratio: 0.0,
        area_m2: 0.0,
        velocity_m_s: 0.0,
        froude: None,
        full_discharge_m3_s: full.discharge_m3_s,
        full_velocity_m_s: full.velocity_m_s,
        exceeds_capacity: false,
    };
    if discharge_m3_s == 0.0 {
        return Ok(base);
    }
    if discharge_m3_s > full.discharge_m3_s {
        return Ok(NormalFlow { depth_m: h, depth_ratio: 1.0, area_m2: full.area_m2, velocity_m_s: discharge_m3_s / full.area_m2, exceeds_capacity: true, ..base });
    }
    // find the depth of largest free-surface discharge below the full height
    let top = h * (1.0 - 1e-9);
    let samples = 400;
    let mut y_peak = top;
    let mut q_peak = discharge_at(shape, roughness, slope, top, settings)?;
    for k in 1..samples {
        let y = top * k as f64 / samples as f64;
        let q = discharge_at(shape, roughness, slope, y, settings)?;
        if q > q_peak {
            q_peak = q;
            y_peak = y;
        }
    }
    let (mut lo, mut hi) = (0.0, y_peak);
    if q_peak < discharge_m3_s {
        // numerically the same as full: take the full section
        return Ok(NormalFlow { depth_m: h, depth_ratio: 1.0, area_m2: full.area_m2, velocity_m_s: discharge_m3_s / full.area_m2, ..base });
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if discharge_at(shape, roughness, slope, mid, settings)? < discharge_m3_s {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-13 * h {
            break;
        }
    }
    let y = 0.5 * (lo + hi);
    let s = shape.section_at(y).ok_or_else(|| "the section has no usable profile".to_string())?;
    let v = discharge_m3_s / s.area_m2;
    let froude = if s.top_width_m > 0.0 { Some(froude_number(v, s.area_m2 / s.top_width_m)) } else { None };
    Ok(NormalFlow { depth_m: y, depth_ratio: y / h, area_m2: s.area_m2, velocity_m_s: v, froude, ..base })
}
