// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Errors of the crate.

use thiserror::Error;

/// Everything that can go wrong. Messages are plain language and name the file or the input.
#[derive(Debug, Error)]
pub enum PointCloudError {
    #[error("input or output error: {0}")]
    Io(String),
    #[error("LAS or LAZ error: {0}")]
    Las(String),
    #[error("E57 error: {0}")]
    E57(String),
    #[error("text point file, line {line}: {message}")]
    Text { line: usize, message: String },
    #[error("unsupported or unrecognised file: {0}")]
    Format(String),
    #[error("cache file: {0}")]
    Cache(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("the file holds {count} points and the limit is {limit}; raise the limit or choose thinning")]
    TooManyPoints { count: u64, limit: u64 },
    #[error("surface: {0}")]
    Surface(String),
}

pub type Result<T> = std::result::Result<T, PointCloudError>;

impl From<std::io::Error> for PointCloudError {
    fn from(e: std::io::Error) -> Self {
        PointCloudError::Io(e.to_string())
    }
}

impl From<las::Error> for PointCloudError {
    fn from(e: las::Error) -> Self {
        PointCloudError::Las(e.to_string())
    }
}

impl From<e57::Error> for PointCloudError {
    fn from(e: e57::Error) -> Self {
        PointCloudError::E57(e.to_string())
    }
}

impl From<serde_json::Error> for PointCloudError {
    fn from(e: serde_json::Error) -> Self {
        PointCloudError::Cache(e.to_string())
    }
}

impl From<afrobim_terrain::TerrainError> for PointCloudError {
    fn from(e: afrobim_terrain::TerrainError) -> Self {
        PointCloudError::Surface(e.to_string())
    }
}
