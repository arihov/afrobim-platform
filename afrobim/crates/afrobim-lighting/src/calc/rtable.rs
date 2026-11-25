// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Road surface reduction coefficients (r-tables), read from a file the user supplies.
//!
//! **No r-table ships with this crate.** CIE r-tables are standard content; the only tables
//! held are those reproduced in the superseded BS 5489-1:2003 (Annex C, Tables C.2 and C.3,
//! pp. 44 and 45), a copyrighted document whose values are not copied here. The user supplies
//! a file in the plain format below.
//!
//! # What the numbers mean
//!
//! The CIE reduced luminance coefficient is `r = 10 000 * q * cos^3(gamma)`, with `q = L / E`
//! the luminance coefficient (sr^-1), `gamma` the angle of incidence, tabulated against the
//! angle `beta` between the vertical plane through the point and the luminaire and the vertical
//! plane through the point and the observer (0 to 180 degrees, symmetry about the plane 0/180
//! assumed) and against `tan(gamma)` (Corell and Sorensen, "An instrument for the measurement of
//! road surface reflection properties", DTU Fotonik / Nordic Road Association, section 2,
//! summarising CIE 144:2001). With `E = I cos(gamma) / d^2` and `d = h / cos(gamma)` this gives
//! the luminance at a point from one luminaire: `L = I(C, gamma) * r_real / h^2` (cd/m2), with
//! `r_real = 10^-4 * table value`, `h` the height of the luminaire above the point. The
//! relationship to CIE 144 is taken from that paper, not from the standard.
//!
//! # File format
//!
//! A text file. Blank lines and anything after `#` are ignored.
//!
//! ```text
//! name: Test surface            # free text
//! unit: 1e-4                    # sr^-1 per table unit; required. CIE r-tables are in units of 1e-4
//! q0_table: 0.07                # optional: Q0 of the table as published
//! q0_surface: 0.10              # optional: Q0 wanted; r is multiplied by q0_surface / q0_table
//! beta: 0 2 5 10 15 20 ... 180  # degrees, increasing, first 0, last 180
//! row: 0     329 329 329 ...    # tan(gamma), then one value per beta; "-" for no value
//! row: 0.25  362 358 371 ...
//! ```
//!
//! `q0_table` and `q0_surface` must both be given or both omitted. The Q0 scaling is the one
//! BS 5489-1:2003 describes in the note to Table C.3 (p. 45): the table of the concrete surface
//! is "scaled for Q0 = 0.10" from the standard table for Q0 = 0.07. Rows must have increasing
//! `tan(gamma)` starting at 0. Cells marked `-` are taken as zero and counted in a warning. A
//! lookup beyond the last row contributes zero and is counted in the calculation warnings.

use super::CalcError;

/// A parsed r-table.
#[derive(Debug, Clone, PartialEq)]
pub struct RTable {
    pub name: String,
    /// sr^-1 per table unit.
    pub unit: f64,
    pub q0_table: Option<f64>,
    pub q0_surface: Option<f64>,
    pub beta_deg: Vec<f64>,
    pub tan_gamma: Vec<f64>,
    /// Table values (not yet multiplied by `unit` or the Q0 scale), row per tan(gamma).
    pub values: Vec<Vec<f64>>,
    /// Number of cells marked `-`.
    pub missing_cells: usize,
}

impl RTable {
    /// Parses the file text (see the module documentation).
    pub fn parse(text: &str) -> Result<RTable, CalcError> {
        let err = |m: String| CalcError::RTable(m);
        let mut name = String::new();
        let (mut unit, mut q0t, mut q0s): (Option<f64>, Option<f64>, Option<f64>) = (None, None, None);
        let mut beta: Vec<f64> = Vec::new();
        let mut tan_gamma = Vec::new();
        let mut values: Vec<Vec<f64>> = Vec::new();
        let mut missing = 0usize;
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, rest) = line.split_once(':').ok_or_else(|| err(format!("line {}: expected 'key: value'", n + 1)))?;
            let key = key.trim().to_ascii_lowercase();
            let rest = rest.trim();
            let num = |s: &str| s.parse::<f64>().map_err(|_| err(format!("line {}: '{s}' is not a number", n + 1)));
            match key.as_str() {
                "name" => name = rest.to_string(),
                "unit" => unit = Some(num(rest)?),
                "q0_table" => q0t = Some(num(rest)?),
                "q0_surface" => q0s = Some(num(rest)?),
                "beta" => beta = rest.split_whitespace().map(num).collect::<Result<_, _>>()?,
                "row" => {
                    let mut it = rest.split_whitespace();
                    let tg = num(it.next().ok_or_else(|| err(format!("line {}: empty row", n + 1)))?)?;
                    let mut row = Vec::new();
                    for cell in it {
                        if cell == "-" {
                            missing += 1;
                            row.push(0.0);
                        } else {
                            row.push(num(cell)?);
                        }
                    }
                    tan_gamma.push(tg);
                    values.push(row);
                }
                other => return Err(err(format!("line {}: unknown key '{other}'", n + 1))),
            }
        }
        let unit = unit.ok_or_else(|| err("'unit:' is required (sr^-1 per table unit; CIE r-tables use 1e-4)".to_string()))?;
        if !(unit.is_finite() && unit > 0.0) {
            return Err(err("'unit:' must be greater than zero".to_string()));
        }
        if q0t.is_some() != q0s.is_some() {
            return Err(err("give both q0_table and q0_surface, or neither".to_string()));
        }
        if let (Some(a), Some(b)) = (q0t, q0s) {
            if !(a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()) {
                return Err(err("q0_table and q0_surface must be greater than zero".to_string()));
            }
        }
        if beta.len() < 2 || beta[0] != 0.0 || *beta.last().unwrap() != 180.0 || beta.windows(2).any(|w| w[1] <= w[0]) {
            return Err(err("'beta:' must list increasing angles from 0 to 180 degrees".to_string()));
        }
        if tan_gamma.len() < 2 || tan_gamma[0] != 0.0 || tan_gamma.windows(2).any(|w| w[1] <= w[0]) {
            return Err(err("'row:' lines must have increasing tan(gamma) starting at 0, at least two rows".to_string()));
        }
        for (k, r) in values.iter().enumerate() {
            if r.len() != beta.len() {
                return Err(err(format!("row for tan(gamma) = {} has {} values; {} beta angles are listed", tan_gamma[k], r.len(), beta.len())));
            }
            if r.iter().any(|v| *v < 0.0 || !v.is_finite()) {
                return Err(err(format!("row for tan(gamma) = {} has a negative or non-finite value", tan_gamma[k])));
            }
        }
        Ok(RTable { name, unit, q0_table: q0t, q0_surface: q0s, beta_deg: beta, tan_gamma, values, missing_cells: missing })
    }

    /// Reads the table from a file.
    pub fn load(path: &std::path::Path) -> Result<RTable, CalcError> {
        let text = std::fs::read_to_string(path).map_err(|e| CalcError::RTable(format!("{}: {e}", path.display())))?;
        RTable::parse(&text)
    }

    /// Multiplier from table units to sr^-1, including the Q0 scaling.
    pub fn scale(&self) -> f64 {
        let q = match (self.q0_table, self.q0_surface) {
            (Some(t), Some(s)) => s / t,
            _ => 1.0,
        };
        self.unit * q
    }

    /// The reduced luminance coefficient in sr^-1 at `beta_deg` and `tan_gamma`, bilinear in both,
    /// or `None` beyond the last tabulated tan(gamma).
    pub fn r(&self, beta_deg: f64, tan_gamma: f64) -> Option<f64> {
        if tan_gamma > *self.tan_gamma.last().unwrap() || tan_gamma.is_nan() {
            return None;
        }
        let b = beta_deg.clamp(0.0, 180.0);
        let nb = self.beta_deg.len();
        let ib = self.beta_deg.partition_point(|a| *a <= b).clamp(1, nb - 1);
        let tb = (b - self.beta_deg[ib - 1]) / (self.beta_deg[ib] - self.beta_deg[ib - 1]);
        let nt = self.tan_gamma.len();
        let it = self.tan_gamma.partition_point(|a| *a <= tan_gamma.max(0.0)).clamp(1, nt - 1);
        let tt = (tan_gamma.max(0.0) - self.tan_gamma[it - 1]) / (self.tan_gamma[it] - self.tan_gamma[it - 1]);
        let row = |i: usize| self.values[i][ib - 1] * (1.0 - tb) + self.values[i][ib] * tb;
        Some((row(it - 1) * (1.0 - tt) + row(it) * tt) * self.scale())
    }
}
