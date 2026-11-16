//! Errors and findings.

use serde::{Deserialize, Serialize};

/// Why a build or a construction could not be completed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum JunctionError {
    #[error("there is no alignment named '{0}'")]
    NoAlignment(String),
    #[error("alignment '{0}' has no vertical profile")]
    NoProfile(String),
    #[error("the alignments '{a}' and '{b}' cross {found} time(s); crossing index {index} does not exist")]
    NoCrossing { a: String, b: String, found: usize, index: usize },
    #[error("invalid input: {0}")]
    Input(String),
    #[error("{0}")]
    Geometry(String),
    #[error("return {quadrant} does not fit: {message}")]
    ReturnDoesNotFit { quadrant: String, message: String },
    #[error("profile: {0}")]
    Profile(String),
    #[error("corridor: {0}")]
    Corridor(String),
}

impl JunctionError {
    pub(crate) fn input(m: impl Into<String>) -> Self {
        JunctionError::Input(m.into())
    }
    pub(crate) fn geometry(m: impl Into<String>) -> Self {
        JunctionError::Geometry(m.into())
    }
}

/// How serious a finding is. `Error` means a part of the result could not be built; `Warning`
/// means a figure the engineer should look at that this crate can state without a limit (for
/// example a return that cannot be built, or two returns that overlap); `Info` is a reported
/// value. **No finding compares a value with a design limit**: no limit is built in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One reported fact about the result: a code (stable, for filtering), what it is about, a
/// sentence, and the numbers behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub subject: String,
    pub message: String,
    /// Named numbers, in metres, radians, ratios or percent as the name says.
    pub values: Vec<(String, f64)>,
}

impl Finding {
    pub(crate) fn new(code: &str, severity: Severity, subject: &str, message: String, values: &[(&str, f64)]) -> Finding {
        Finding {
            code: code.to_string(),
            severity,
            subject: subject.to_string(),
            message,
            // Non-finite numbers are left out: JSON has no way to hold them.
            values: values.iter().filter(|(_, v)| v.is_finite()).map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }
}
