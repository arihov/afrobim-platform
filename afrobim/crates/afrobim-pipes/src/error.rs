use crate::model::{PipeId, StructureId};

/// Why an operation on a pipe network was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PipesError {
    #[error("structure {0} is not in the network")]
    NoStructure(StructureId),
    #[error("pipe {0} is not in the network")]
    NoPipe(PipeId),
    #[error("{0}")]
    Invalid(String),
    #[error("parts list: {0}")]
    Parts(String),
    #[error("LandXML: {0}")]
    Xml(String),
}
