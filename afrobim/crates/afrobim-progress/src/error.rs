// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Errors of this crate.

#[derive(Debug, thiserror::Error)]
pub enum ProgressError {
    /// Input the crate refuses, with the reason.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Cost(#[from] afrobim_cost::CostError),
    #[error("{0}")]
    Schedule(String),
}

pub type Result<T> = std::result::Result<T, ProgressError>;

pub(crate) fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(ProgressError::Invalid(msg.into()))
}
