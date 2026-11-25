// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Lookup in the superelevation tables 6-4 to 6-9 (data in `se_data.rs`).

use serde::Serialize;

use super::se_data::{T6_4, T6_5, T6_6, T6_7, T6_8, T6_9};
use super::{refuse, speed_index, Cited, Source};
use crate::error::DesignError;

/// One cell of a superelevation table, as printed: a normal crown, a removal of the adverse crown, or a
/// superelevation rate in percent; each (but the normal crown) with the minimum runoff length L in m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SeCell {
    /// `NC`: normal crown section.
    Nc,
    /// `RC`, runoff length L (m): remove adverse crown, superelevation at the normal crown slope.
    Rc(u16),
    /// Rate (%) and runoff length L (m).
    E(f64, u16),
}

pub(crate) struct SeTable {
    pub name: &'static str,
    pub emax_percent: f64,
    pub normal_crown_percent: f64,
    pub printed_page: u16,
    pub pdf_page: u16,
    /// The `Rmin` cell printed for each design speed 30 to 120 km/h, m.
    pub rmin_m: [u16; 10],
    pub rows: &'static [(u32, [Option<SeCell>; 10])],
}

/// What the table prints for a radius and a design speed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum SuperelevationRate {
    /// `NC`: normal crown section, no superelevation work.
    NormalCrown,
    /// `RC`: remove the adverse crown; superelevation at the normal crown slope.
    RemoveAdverseCrown,
    /// Superelevation rate, percent.
    Rate(f64),
}

/// A cell of Tables 6-4 to 6-9.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuperelevationCell {
    pub table: &'static str,
    pub radius_m: f64,
    pub speed_kmh: f64,
    pub emax_percent: f64,
    pub normal_crown_percent: f64,
    pub rate: SuperelevationRate,
    /// Minimum length of runoff L, m, as printed (0 for `NC`). The manual says it does not include the tangent runout.
    pub min_runoff_length_m: f64,
    /// The `Rmin` the same table prints for this speed, m.
    pub rmin_m: f64,
}

fn all_tables() -> [&'static SeTable; 6] {
    [&T6_4, &T6_5, &T6_6, &T6_7, &T6_8, &T6_9]
}

fn table_for(emax_percent: f64, normal_crown_percent: f64) -> Result<&'static SeTable, DesignError> {
    all_tables().into_iter().find(|t| (t.emax_percent - emax_percent).abs() < 1e-9 && (t.normal_crown_percent - normal_crown_percent).abs() < 1e-9).ok_or_else(|| {
        DesignError::Input(format!(
            "The manual has superelevation tables (6-4 to 6-9) for a maximum superelevation of 4, 7 or 8 % with a normal crown slope of 2.5 or 4 %; emax {emax_percent} % with crown {normal_crown_percent} % is not tabulated (a 6 % maximum appears in Table 6-1 only)."
        ))
    })
}

fn cell_of(t: &'static SeTable, speed_idx: usize, radius_m: f64, row_idx: usize) -> Cited<SuperelevationCell> {
    let cells = &t.rows[row_idx].1;
    let (rate, l) = match cells[speed_idx] {
        Some(SeCell::Nc) => (SuperelevationRate::NormalCrown, 0.0),
        Some(SeCell::Rc(l)) => (SuperelevationRate::RemoveAdverseCrown, l as f64),
        Some(SeCell::E(e, l)) => (SuperelevationRate::Rate(e), l as f64),
        None => unreachable!("caller checks the cell exists"),
    };
    Cited::new(
        SuperelevationCell {
            table: t.name,
            radius_m,
            speed_kmh: super::SPEEDS_KMH[speed_idx],
            emax_percent: t.emax_percent,
            normal_crown_percent: t.normal_crown_percent,
            rate,
            min_runoff_length_m: l,
            rmin_m: t.rmin_m[speed_idx] as f64,
        },
        "% and m",
        Source::new(t.name, "6.2.3 vii", t.printed_page, t.pdf_page),
    )
}

/// The radii (rows) the table prints, largest first.
pub fn tabulated_radii() -> Vec<f64> {
    T6_4.rows.iter().map(|(r, _)| *r as f64).collect()
}

/// The `Rmin` cells printed in one of Tables 6-4 to 6-9 for design speeds 30 to 120 km/h, m.
pub fn table_rmin(emax_percent: f64, normal_crown_percent: f64) -> Result<Cited<[f64; 10]>, DesignError> {
    let t = table_for(emax_percent, normal_crown_percent)?;
    let mut out = [0.0; 10];
    for (o, r) in out.iter_mut().zip(t.rmin_m.iter()) {
        *o = *r as f64;
    }
    Ok(Cited::new(out, "m", Source::new(t.name, "6.2.3 vii", t.printed_page, t.pdf_page)))
}

/// The superelevation rate and minimum runoff length printed for a design speed and a radius that is
/// one of the table's rows. Radii between rows are refused (no interpolation rule is given); see
/// [`superelevation_neighbours`]. A radius below the table's `Rmin` for the speed is refused.
pub fn superelevation(emax_percent: f64, normal_crown_percent: f64, speed_kmh: f64, radius_m: f64) -> Result<Cited<SuperelevationCell>, DesignError> {
    let t = table_for(emax_percent, normal_crown_percent)?;
    let si = speed_index(speed_kmh)?;
    let rmin = t.rmin_m[si] as f64;
    if radius_m < rmin {
        return refuse(format!("{}: a radius of {radius_m} m is below the minimum radius Rmin = {rmin} m that the table prints for {speed_kmh} km/h; the manual gives no superelevation there.", t.name));
    }
    let Some(row) = t.rows.iter().position(|(r, _)| (*r as f64 - radius_m).abs() < 1e-9) else {
        let rows: Vec<String> = t.rows.iter().map(|(r, _)| r.to_string()).collect();
        return refuse(format!(
            "{}: a radius of {radius_m} m is not a row of the table (rows: {} m). The manual gives no rule for interpolating between rows, so no value is returned; `superelevation_neighbours` gives the rows either side.",
            t.name,
            rows.join(", ")
        ));
    };
    if t.rows[row].1[si].is_none() {
        return refuse(format!("{}: the cell for {radius_m} m at {speed_kmh} km/h is empty (below Rmin = {rmin} m).", t.name));
    }
    Ok(cell_of(t, si, radius_m, row))
}

/// The two printed rows either side of a radius that is not itself a row: `(flatter, sharper)`, the
/// flatter being the smallest tabulated radius above `radius_m` and the sharper the largest below it
/// that has a value at this speed. No value is chosen between them. Refused above the largest row
/// (7000 m) and below the table's `Rmin` for the speed.
pub fn superelevation_neighbours(emax_percent: f64, normal_crown_percent: f64, speed_kmh: f64, radius_m: f64) -> Result<(Cited<SuperelevationCell>, Cited<SuperelevationCell>), DesignError> {
    let t = table_for(emax_percent, normal_crown_percent)?;
    let si = speed_index(speed_kmh)?;
    let rmin = t.rmin_m[si] as f64;
    if radius_m < rmin {
        return refuse(format!("{}: a radius of {radius_m} m is below the table's Rmin = {rmin} m for {speed_kmh} km/h.", t.name));
    }
    if radius_m > t.rows[0].0 as f64 {
        return refuse(format!("{}: a radius of {radius_m} m is above the largest row of the table ({} m); nothing is extrapolated.", t.name, t.rows[0].0));
    }
    let Some(fi) = t.rows.iter().rposition(|(r, _)| (*r as f64) >= radius_m) else { return refuse("radius out of range") };
    let Some(si_row) = (fi + 1..t.rows.len()).find(|&i| t.rows[i].1[si].is_some()) else {
        return refuse(format!("{}: there is no printed row below {radius_m} m at {speed_kmh} km/h.", t.name));
    };
    Ok((cell_of(t, si, t.rows[fi].0 as f64, fi), cell_of(t, si, t.rows[si_row].0 as f64, si_row)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_in_descending_order_and_the_same_in_every_table() {
        let radii = tabulated_radii();
        assert_eq!(radii.len(), 34);
        for t in all_tables() {
            let r: Vec<f64> = t.rows.iter().map(|(r, _)| *r as f64).collect();
            assert_eq!(r, radii, "{}", t.name);
            assert!(r.windows(2).all(|w| w[0] > w[1]));
        }
    }
}
