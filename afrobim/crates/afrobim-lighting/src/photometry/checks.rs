// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Checks on a loaded distribution: flux by integration over the sphere, flux fractions, peak.

use serde::{Deserialize, Serialize};

use super::model::Photometry;

/// Result of integrating a distribution over the sphere and comparing it with what the file says.
///
/// Every field is a computed or stated number. No tolerance is applied: whether a difference is
/// acceptable is the user's call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluxCheck {
    /// The angular step of the integration, degrees.
    pub step_deg: f64,
    /// Luminous flux of the luminaire by integrating the intensities over the full sphere, lm.
    pub integrated_flux_lm: f64,
    /// Flux of the lower hemisphere (gamma 0 to 90) and of the upper one (90 to 180), lm.
    pub downward_flux_lm: f64,
    pub upward_flux_lm: f64,
    /// Fractions of the integrated luminaire flux.
    pub downward_fraction: f64,
    pub upward_fraction: f64,
    /// The lamp flux the file states, lm, if any.
    pub stated_lamp_flux_lm: Option<f64>,
    /// Integrated flux over the stated lamp flux, per cent: the light output ratio the
    /// distribution implies. `None` without a stated flux.
    pub computed_lor_percent: Option<f64>,
    /// The light output ratio the file states (EULUMDAT), per cent.
    pub stated_lor_percent: Option<f64>,
    /// Integrated flux minus the flux the file's own light output ratio implies
    /// (`stated_lor_percent / 100 * stated_lamp_flux_lm`), lm, and as per cent of that flux. For
    /// a file that states no light output ratio the comparison is with the lamp flux itself
    /// (a light output ratio of 100 %).
    pub difference_lm: Option<f64>,
    pub difference_percent: Option<f64>,
    /// The downward flux fraction the file states (EULUMDAT), per cent, for comparison with
    /// `downward_fraction` (the stated one may be defined against the lamp flux).
    pub stated_downward_fraction_percent: Option<f64>,
}

/// Where the highest intensity is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PeakIntensity {
    pub candela: f64,
    pub c_deg: f64,
    pub gamma_deg: f64,
}

impl Photometry {
    /// Flux over the angular range `gamma_from_deg` to `gamma_to_deg` (all C-planes), lm, by the
    /// midpoint rule with cells of about `step_deg` in C and gamma. Each cell is weighted by its
    /// exact solid angle (`dC * (cos g_lo - cos g_hi)`) and takes the intensity at its centre, so a
    /// uniform source integrates exactly. For smooth distributions the error shrinks with the
    /// square of the step.
    pub fn flux_between_lm(&self, gamma_from_deg: f64, gamma_to_deg: f64, step_deg: f64) -> f64 {
        let step = step_deg.clamp(0.05, 45.0);
        let span = (gamma_to_deg - gamma_from_deg).max(0.0);
        let ng = ((span / step).ceil() as usize).max(1);
        let nc = ((360.0 / step).ceil() as usize).max(1);
        let dg = span / ng as f64;
        let dc = 360.0 / nc as f64;
        let mut sum = 0.0;
        for j in 0..ng {
            let lo = gamma_from_deg + j as f64 * dg;
            let hi = lo + dg;
            let weight = (lo.to_radians().cos() - hi.to_radians().cos()) * dc.to_radians();
            let g = 0.5 * (lo + hi);
            for i in 0..nc {
                let c = (i as f64 + 0.5) * dc;
                sum += self.intensity(c, g) * weight;
            }
        }
        sum
    }

    /// Flux of the whole sphere by [`Self::flux_between_lm`].
    pub fn integrated_flux_lm(&self, step_deg: f64) -> f64 {
        self.flux_between_lm(0.0, 180.0, step_deg)
    }

    /// The flux check with the given angular step (1 degree is a reasonable value).
    pub fn flux_check(&self, step_deg: f64) -> FluxCheck {
        let down = self.flux_between_lm(0.0, 90.0, step_deg);
        let up = self.flux_between_lm(90.0, 180.0, step_deg);
        let total = down + up;
        let (down_fraction, up_fraction) = if total > 0.0 { (down / total, up / total) } else { (0.0, 0.0) };
        let computed_lor = self.lamp_flux_lm.filter(|f| *f > 0.0).map(|f| 100.0 * total / f);
        let reference = self.lamp_flux_lm.filter(|f| *f > 0.0).map(|f| f * self.lor_percent.unwrap_or(100.0) / 100.0);
        let difference_lm = reference.map(|r| total - r);
        let difference_percent = reference.filter(|r| *r > 0.0).map(|r| 100.0 * (total - r) / r);
        FluxCheck {
            step_deg,
            integrated_flux_lm: total,
            downward_flux_lm: down,
            upward_flux_lm: up,
            downward_fraction: down_fraction,
            upward_fraction: up_fraction,
            stated_lamp_flux_lm: self.lamp_flux_lm,
            computed_lor_percent: computed_lor,
            stated_lor_percent: self.lor_percent,
            difference_lm,
            difference_percent,
            stated_downward_fraction_percent: self.dff_percent,
        }
    }

    /// The highest tabulated intensity and its direction.
    pub fn peak_intensity(&self) -> PeakIntensity {
        let ng = self.gamma_deg.len();
        let (mut best, mut at) = (f64::NEG_INFINITY, 0usize);
        for (i, v) in self.candela.iter().enumerate() {
            if *v > best {
                best = *v;
                at = i;
            }
        }
        PeakIntensity { candela: best, c_deg: self.c_deg[at / ng], gamma_deg: self.gamma_deg[at % ng] }
    }

    /// The share of the integrated flux that falls in the half-space whose C-planes lie within 90
    /// degrees of `c_toward_deg` (for a road luminaire, the street side when `c_toward_deg` is
    /// the C-plane that faces the carriageway). A computed number to help judge the
    /// orientation setting; the two sides of a symmetric luminaire each give 0.5.
    pub fn flux_fraction_towards_c(&self, c_toward_deg: f64, step_deg: f64) -> f64 {
        let step = step_deg.clamp(0.05, 45.0);
        let ng = ((180.0 / step).ceil() as usize).max(1);
        let nc = ((360.0 / step).ceil() as usize).max(1);
        let dg = 180.0 / ng as f64;
        let dc = 360.0 / nc as f64;
        let (mut towards, mut all) = (0.0, 0.0);
        for j in 0..ng {
            let lo = j as f64 * dg;
            let hi = lo + dg;
            let weight = (lo.to_radians().cos() - hi.to_radians().cos()) * dc.to_radians();
            for i in 0..nc {
                let c = (i as f64 + 0.5) * dc;
                let f = self.intensity(c, 0.5 * (lo + hi)) * weight;
                all += f;
                let d = (c - c_toward_deg).rem_euclid(360.0);
                if !(90.0..=270.0).contains(&d) {
                    towards += f;
                }
            }
        }
        if all > 0.0 {
            towards / all
        } else {
            0.0
        }
    }
}
