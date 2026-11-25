// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The one model both file formats are read into.

use serde::{Deserialize, Serialize};

/// Which format a [`Photometry`] was read from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SourceFormat {
    /// IES LM-63. `version` is what the first line says: `LM-63-1986 (no version line)`,
    /// `IESNA91`, `IESNA:LM-63-1995`, `IESNA:LM-63-2002`, or the unrecognised tag as found.
    Ies { version: String },
    /// EULUMDAT (.ldt).
    Eulumdat,
}

impl SourceFormat {
    pub fn label(&self) -> String {
        match self {
            SourceFormat::Ies { version } => format!("IES LM-63 ({version})"),
            SourceFormat::Eulumdat => "EULUMDAT".to_string(),
        }
    }
}

/// A luminaire's luminous intensity distribution in the C-gamma system, expanded to the full
/// turn of C-planes, in candela.
///
/// - `c_deg`: the C-plane angles, strictly increasing, all in [0, 360). One plane means the
///   distribution is the same in every C-plane (rotationally symmetric).
/// - `gamma_deg`: the gamma angles, strictly increasing, in [0, 180]. Gamma is measured from the
///   nadir (0 is straight down, 90 horizontal, 180 straight up). Outside the tabulated range the
///   intensity is taken as zero: a file that stops at 90 degrees emits nothing upward.
/// - `candela`: absolute luminous intensity, plane after plane (`candela[ic * gamma_deg.len() +
///   ig]`), after applying the format's own scaling (see [`crate::photometry`]).
///
/// Intensity between the tabulated angles is interpolated linearly in C and in gamma (bilinear).
/// That is this crate's choice: neither format prescribes an interpolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Photometry {
    pub format: SourceFormat,
    pub manufacturer: String,
    pub luminaire: String,
    pub catalogue: String,
    pub test_report: String,
    pub c_deg: Vec<f64>,
    pub gamma_deg: Vec<f64>,
    pub candela: Vec<f64>,
    /// Luminous flux of the lamps (IES: number of lamps times lumens per lamp), lm. For
    /// EULUMDAT with absolute photometry it is the stated flux of the luminaire. `None` for IES
    /// absolute photometry that states no flux.
    pub lamp_flux_lm: Option<f64>,
    /// The file states the intensities as absolute (IES lumens per lamp of -1, EULUMDAT negative
    /// number of lamps).
    pub absolute_photometry: bool,
    /// Light output ratio of the luminaire as the file states it (EULUMDAT only), per cent.
    pub lor_percent: Option<f64>,
    /// Downward flux fraction as the file states it (EULUMDAT only), per cent.
    pub dff_percent: Option<f64>,
    /// Input power, W, as stated (IES input watts; EULUMDAT wattage including ballast).
    pub input_power_w: Option<f64>,
    /// IES ballast factor and ballast-lamp photometric factor as stated.
    pub ballast_factor: Option<f64>,
    pub ballast_lamp_photometric_factor: Option<f64>,
    /// Tilt of the luminaire during measurement, degrees (EULUMDAT line 25); 0 for IES.
    pub measured_tilt_deg: f64,
    /// Length, width, height in metres of the luminous opening (IES) or of the luminaire
    /// (EULUMDAT lines 13 to 15), when stated.
    pub dimensions_m: Option<[f64; 3]>,
    /// Length, width of the luminous area in metres (EULUMDAT lines 16 and 17), when stated.
    pub luminous_area_m: Option<[f64; 2]>,
    /// The symmetry the file declared, in words, for the report.
    pub symmetry: String,
    /// Things the reader noticed that the user should know (assumptions made, fields ignored).
    pub warnings: Vec<String>,
}

impl Photometry {
    /// Luminous intensity in candela towards (`c_deg`, `gamma_deg`): bilinear interpolation in the
    /// tabulated planes, periodic in C, zero outside the tabulated gamma range.
    pub fn intensity(&self, c_deg: f64, gamma_deg: f64) -> f64 {
        let g = &self.gamma_deg;
        let ng = g.len();
        let first = g[0];
        let last = g[ng - 1];
        const EDGE: f64 = 1e-9;
        if gamma_deg < first - EDGE || gamma_deg > last + EDGE || gamma_deg.is_nan() || c_deg.is_nan() {
            return 0.0;
        }
        let gamma = gamma_deg.clamp(first, last);
        let (j0, j1, tg) = if ng == 1 {
            (0, 0, 0.0)
        } else {
            let j1 = g.partition_point(|a| *a <= gamma).clamp(1, ng - 1);
            let j0 = j1 - 1;
            (j0, j1, (gamma - g[j0]) / (g[j1] - g[j0]))
        };
        let nc = self.c_deg.len();
        let (i0, i1, tc) = if nc == 1 {
            (0, 0, 0.0)
        } else {
            let c = c_deg.rem_euclid(360.0);
            let k = self.c_deg.partition_point(|a| *a <= c);
            let (i0, c0) = if k == 0 { (nc - 1, self.c_deg[nc - 1] - 360.0) } else { (k - 1, self.c_deg[k - 1]) };
            let (i1, c1) = if k == nc { (0, self.c_deg[0] + 360.0) } else { (k, self.c_deg[k]) };
            (i0, i1, (c - c0) / (c1 - c0))
        };
        let at = |ic: usize, ig: usize| self.candela[ic * ng + ig];
        let low = at(i0, j0) * (1.0 - tg) + at(i0, j1) * tg;
        let high = at(i1, j0) * (1.0 - tg) + at(i1, j1) * tg;
        low * (1.0 - tc) + high * tc
    }

    /// Checks the shape invariants (used by the readers and when a stored distribution is read
    /// back from a document).
    pub fn validate(&self) -> Result<(), String> {
        if self.c_deg.is_empty() || self.gamma_deg.is_empty() {
            return Err("the distribution has no C-planes or no gamma angles".to_string());
        }
        if self.candela.len() != self.c_deg.len() * self.gamma_deg.len() {
            return Err(format!("{} intensities for {} C-planes and {} gamma angles", self.candela.len(), self.c_deg.len(), self.gamma_deg.len()));
        }
        let increasing = |v: &[f64]| v.windows(2).all(|w| w[1] > w[0]);
        if !increasing(&self.c_deg) || !increasing(&self.gamma_deg) {
            return Err("angles are not strictly increasing".to_string());
        }
        if self.c_deg[0] < 0.0 || *self.c_deg.last().unwrap() >= 360.0 {
            return Err("C angles must be in [0, 360)".to_string());
        }
        if self.gamma_deg[0] < 0.0 || *self.gamma_deg.last().unwrap() > 180.0 {
            return Err("gamma angles must be in [0, 180]".to_string());
        }
        if self.candela.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("intensities must be finite and not negative".to_string());
        }
        Ok(())
    }
}

/// How the C-planes stored in a file are completed to the full turn. `a` is a stored C angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Completion {
    /// Every plane stored: nothing is added.
    None,
    /// A single plane stands for all of them.
    Axial,
    /// Symmetric about the C0-C180 plane: C also stands for 360 - C.
    About0And180,
    /// Symmetric about the C90-C270 plane: C also stands for 180 - C.
    About90And270,
    /// Symmetric about both planes: C also stands for 180 - C, 180 + C and 360 - C.
    Quadrants,
}

/// Completes `planes` (stored C angle, intensities over gamma) to a sorted list of planes in
/// [0, 360). Planes that land on the same angle are merged (the first kept).
pub(crate) fn complete_planes(planes: Vec<(f64, Vec<f64>)>, how: Completion) -> (Vec<f64>, Vec<Vec<f64>>) {
    let norm = |a: f64| {
        let r = a.rem_euclid(360.0);
        if (360.0 - r).abs() < 1e-9 {
            0.0
        } else {
            r
        }
    };
    let mut all: Vec<(f64, Vec<f64>)> = Vec::new();
    for (a, row) in planes {
        let mut images = vec![norm(a)];
        match how {
            Completion::None => {}
            Completion::Axial => {
                images = vec![0.0];
            }
            Completion::About0And180 => images.push(norm(360.0 - a)),
            Completion::About90And270 => images.push(norm(180.0 - a)),
            Completion::Quadrants => images.extend([norm(180.0 - a), norm(180.0 + a), norm(360.0 - a)]),
        }
        for img in images {
            all.push((img, row.clone()));
        }
    }
    all.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut angles: Vec<f64> = Vec::new();
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for (a, row) in all {
        if angles.last().is_some_and(|l| (a - l).abs() < 1e-6) {
            continue;
        }
        angles.push(a);
        rows.push(row);
    }
    (angles, rows)
}
