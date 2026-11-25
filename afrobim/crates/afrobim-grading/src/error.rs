// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Errors of the grading engine.

use afrobim_corridor::TinError;
use afrobim_terrain::TerrainError;

/// Why a feature line, grading or group operation was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GradingError {
    /// An argument is not valid (not finite, out of range, empty, and so on).
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The geometry does not allow the operation (a fillet that does not fit, a join that is too far).
    #[error("geometry: {0}")]
    Geometry(String),
    /// A named object does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// The ground surface does not cover a point that needs an elevation.
    #[error("the surface does not cover the point ({0:.3}, {1:.3})")]
    OutsideSurface(f64, f64),
    /// The balance search interval does not bracket the target.
    #[error("the search interval does not bracket the target: net is {low_net} at the low end and {high_net} at the high end (target {target})")]
    NotBracketed { low_net: f64, high_net: f64, target: f64 },
    /// The terrain engine refused an input or a build.
    #[error("terrain: {0}")]
    Terrain(#[from] TerrainError),
    /// The TIN type refused the resulting triangles.
    #[error("surface: {0}")]
    Tin(#[from] TinError),
}

pub type Result<T> = std::result::Result<T, GradingError>;

pub(crate) fn finite(name: &str, v: f64) -> Result<()> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(GradingError::Invalid(format!("{name} is not a finite number")))
    }
}
