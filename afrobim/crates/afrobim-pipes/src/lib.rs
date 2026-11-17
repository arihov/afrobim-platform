//! Gravity pipe networks for road and site drainage: the pipe network object of a design package,
//! as an engine for BIM use.
//!
//! * [`model`]: [`PipeNetwork`], [`Structure`], [`Pipe`], shapes, roughness.
//! * [`parts`]: user parts catalogue (JSON or CSV), with BIM classification, material and cost code.
//! * [`edit`]: layout and editing; [`rules`]: the rule set (every value an input).
//! * [`checks`], [`hydraulics`], [`grade`], [`interference`]: checks that return [`Finding`]s.
//! * [`output`]: plan geometry, profile of a run, section crossings; [`tables`], [`quantities`].
//! * [`landxml`]: LandXML 1.2 `PipeNetworks` read and write.
//!
//! Everything here is a calculation for an engineer to check. It is not a certified design, and
//! it holds no engineering default: no pipe size, roughness, cover, velocity, slope, spacing or
//! drop is built in. See the README for sources and limits.

pub mod checks;
mod csv;
pub mod edit;
pub mod error;
pub mod finding;
pub mod grade;
pub mod ground;
pub mod hydraulics;
pub mod interference;
pub mod landxml;
pub mod model;
pub mod output;
pub mod parts;
pub mod quantities;
pub mod rules;
pub mod tables;
pub mod topology;

pub use checks::{HydraulicReport, PipeHydraulics};
pub use edit::{DeleteOutcome, PipeEnd, PipeSpec, RunPoint, RunSpec};
pub use error::PipesError;
pub use finding::{Finding, Severity};
pub use grade::{GradeReport, PipeGrade, StructureGrade};
pub use ground::{Ground, GroundFn, GroundOptFn};
pub use hydraulics::{full_flow, normal_flow, FullFlow, HydraulicSettings, NormalFlow};
pub use landxml::{parse_landxml_pipes, read_landxml_pipes, write_landxml_pipes, LandXmlPipes, WriteOptions};
pub use model::*;
pub use output::{Footprint, PlanGeometry, ProfileAxis, ProfileView, SectionView};
pub use parts::{Part, PartFamily, PartsCatalogue};
pub use quantities::{TrenchQuantities, TrenchSpec, TrenchWidth};
pub use rules::{MatchMode, RuleSet};
pub use tables::{PipeRow, StructureRow};
pub use topology::{FlowResult, Topology};
