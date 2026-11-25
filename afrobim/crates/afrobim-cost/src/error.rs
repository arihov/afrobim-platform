// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Errors of the cost crate.

/// Why a cost operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CostError {
    /// An input is not acceptable (the message says which and why).
    #[error("{0}")]
    Invalid(String),
    /// Text that should be a decimal number is not one, or has more than 6 decimals.
    #[error("'{0}' is not a decimal number with at most 6 decimals")]
    Number(String),
    /// Text that should be a date `YYYY-MM-DD` is not one.
    #[error("'{0}' is not a valid date (expected YYYY-MM-DD)")]
    Date(String),
    /// A CSV file could not be read.
    #[error("csv: {0}")]
    Csv(String),
    /// No exchange rate was entered between two currencies.
    #[error("no exchange rate has been entered between {from} and {to}{when}")]
    NoExchangeRate { from: String, to: String, when: String },
    /// The bill structure has problems; each is one string.
    #[error("the bill has problems: {}", .0.join("; "))]
    Structure(Vec<String>),
}

pub type Result<T> = std::result::Result<T, CostError>;
