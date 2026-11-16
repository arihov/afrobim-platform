//! The one error type of the crate.

use thiserror::Error;

/// Everything that can go wrong in this crate.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CogoError {
    #[error("cannot read angle or bearing '{0}'")]
    BadAngle(String),
    #[error("point number {0} is already in use")]
    DuplicateNumber(u32),
    #[error("point number {0} does not exist")]
    NoSuchPoint(u32),
    #[error("point {0} is locked")]
    Locked(u32),
    #[error("a transform needs a non-empty reason and an explicit list of point numbers")]
    TransformNotNamed,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("units '{0}' are not accepted: the engine works in metres only; give a factor to metres explicitly")]
    UnsupportedUnits(String),
    #[error("point {0} has no elevation but the output order needs one")]
    MissingElevation(u32),
    #[error("the lines are parallel or the circles do not meet")]
    NoIntersection,
    #[error("traverse has no closure (it is open)")]
    OpenTraverse,
    #[error("boundary does not close: misclosure {0} m exceeds the tolerance")]
    NotClosed(f64),
    #[error("offset polyline self-intersects at the given distance")]
    OffsetSelfIntersects,
    #[error("group '{0}' does not exist")]
    NoSuchGroup(String),
}

pub type Result<T> = std::result::Result<T, CogoError>;
