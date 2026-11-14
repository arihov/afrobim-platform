//! Why a design edit or build is refused. Messages are written for the engineer, in plain words.

use serde::{Deserialize, Serialize};

/// One reason a design is impossible, tied to the PI or PVI it concerns (index from 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    /// Index of the PI (horizontal) or PVI (vertical) the problem concerns; `None` for the design as a whole.
    pub index: Option<usize>,
    pub message: String,
}

impl Problem {
    pub fn at(index: usize, message: impl Into<String>) -> Problem {
        Problem { index: Some(index), message: message.into() }
    }
    pub fn whole(message: impl Into<String>) -> Problem {
        Problem { index: None, message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DesignError {
    /// The geometry cannot exist as entered. Every problem found is listed.
    #[error("{}", join(.0))]
    Impossible(Vec<Problem>),
    /// An edit named a PI, PVI or station that does not exist.
    #[error("{0}")]
    BadEdit(String),
    /// Something about the document, the surface or the stored data.
    #[error("{0}")]
    Document(String),
    /// Input data (a limits table, a file) that cannot be read.
    #[error("{0}")]
    Input(String),
}

fn join(problems: &[Problem]) -> String {
    problems.iter().map(|p| p.message.clone()).collect::<Vec<_>>().join(" ")
}

impl DesignError {
    /// The problems listed by a [`DesignError::Impossible`], empty for the other kinds.
    pub fn problems(&self) -> &[Problem] {
        match self {
            DesignError::Impossible(p) => p,
            _ => &[],
        }
    }
}
