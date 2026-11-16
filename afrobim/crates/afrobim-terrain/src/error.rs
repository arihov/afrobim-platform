//! Errors of the terrain engine.

use afrobim_corridor::TinError;

/// Why a surface could not be built, analysed or stored.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TerrainError {
    /// A coordinate or a parameter is NaN or infinite.
    #[error("{0} is not a finite number")]
    NonFinite(String),
    /// Fewer than three distinct points are left after merging duplicates and exclusions.
    #[error("a surface needs at least three distinct points; {0} remain")]
    TooFewPoints(usize),
    /// The points do not span an area (for example they are all on one line).
    #[error("no triangle could be formed: {0}")]
    NoSurface(String),
    /// An operation in the definition is not valid.
    #[error("operation {index} is not valid: {message}")]
    InvalidOperation { index: usize, message: String },
    /// An argument of an analysis or volume function is not valid.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The triangulation library refused an input.
    #[error("triangulation failed: {0}")]
    Triangulation(String),
    /// The TIN type of the corridor crate refused the result.
    #[error("the resulting surface is not a valid TIN: {0}")]
    Tin(#[from] TinError),
    /// The stored definition could not be read or written.
    #[error("the stored definition cannot be decoded: {0}")]
    Encoding(String),
}
