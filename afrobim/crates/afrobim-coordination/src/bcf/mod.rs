//! BCF (BIM Collaboration Format) 2.1 exchange of [`crate::Issue`]s.
//!
//! Format source: the buildingSMART/BCF-XML repository, tag `v2.1` (`Schemas/markup.xsd`,
//! `visinfo.xsd`, `version.xsd`, `project.xsd`, and `Documentation/README.md`). See the crate
//! README for what is written, read and left out.

mod export;
mod import;
pub mod time;
mod xml;
pub mod zip;

pub use export::{export_bcf, BcfError, BcfExport, BcfOptions, ORIGINATING_SYSTEM, SIDECAR_NAME};
pub use import::{import_bcf, BcfImport, ImportOptions, ImportReport};
