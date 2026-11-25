// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The result type of every check: a list of findings, each naming its elements.

use serde::{Deserialize, Serialize};

use crate::model::ElementRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Information: something was not checked, or a method note.
    Info,
    /// A rule is not met, or a result is unreliable.
    Warning,
    /// The input cannot be checked (missing data), or a rule is broken in a way that stops the check.
    Error,
}

/// One finding: which check, how serious, which elements, and a plain sentence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Short code of the check, for example `cover`, `slope`, `length`, `velocity`, `capacity`,
    /// `depth_ratio`, `topology`, `interference`, `data`.
    pub check: String,
    pub severity: Severity,
    pub elements: Vec<ElementRef>,
    pub message: String,
}

impl Finding {
    pub fn new(check: &str, severity: Severity, elements: Vec<ElementRef>, message: String) -> Finding {
        Finding { check: check.to_string(), severity, elements, message }
    }
}
