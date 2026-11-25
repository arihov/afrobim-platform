//! Errors. Impossible inputs are refused with a message that says what to change.

/// Why a culvert cannot be designed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CulvertError {
    /// An input is impossible or inconsistent; the text says which and why.
    #[error("{0}")]
    Invalid(String),
    /// The requested feature has a hook but no engine yet.
    #[error("not implemented: {0}")]
    NotImplemented(String),
    /// The drainage crate's hydraulics refused the inputs.
    #[error("hydraulics: {0}")]
    Hydraulics(String),
}

/// `Err(CulvertError::Invalid(message))`.
pub(crate) fn invalid<T>(message: impl Into<String>) -> Result<T, CulvertError> {
    Err(CulvertError::Invalid(message.into()))
}

/// A finite number, else an error naming it.
pub(crate) fn finite(name: &str, v: f64) -> Result<f64, CulvertError> {
    if v.is_finite() {
        Ok(v)
    } else {
        invalid(format!("{name} must be a number (got {v})"))
    }
}

/// A finite number greater than zero, else an error naming it.
pub(crate) fn positive(name: &str, v: f64) -> Result<f64, CulvertError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        invalid(format!("{name} must be greater than zero (got {v})"))
    }
}

/// A finite number of zero or more, else an error naming it.
pub(crate) fn not_negative(name: &str, v: f64) -> Result<f64, CulvertError> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        invalid(format!("{name} must not be negative (got {v})"))
    }
}
