//! The one error type of the crate.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SurveyError {
    /// A line of an input file could not be read. `line` counts from 1.
    #[error("line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("{0}")]
    Invalid(String),
    #[error("point '{0}' does not exist")]
    NoPoint(String),
    #[error("network '{0}' does not exist")]
    NoNetwork(String),
    /// The normal equations cannot be solved: datum defect, disconnected part or too little data.
    #[error("normal equations are singular: {0}")]
    Singular(String),
    #[error("database file: {0}")]
    Format(String),
    #[error(transparent)]
    Cogo(#[from] afrobim_cogo::CogoError),
}

pub type Result<T> = std::result::Result<T, SurveyError>;

pub(crate) fn parse_err(line: usize, message: impl Into<String>) -> SurveyError {
    SurveyError::Parse { line, message: message.into() }
}
