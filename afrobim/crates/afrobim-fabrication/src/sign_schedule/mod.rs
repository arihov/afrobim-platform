//! The AfroBIM sign schedule: a CSV of the signs of a road, with the station and offset each one
//! stands at.
//!
//! - [`export::template_csv`] writes a blank schedule, [`export::export_csv`] writes the
//!   document's signs in the same format, so export then import round-trips.
//! - [`csv::parse`] reads any CSV (comma, semicolon or tab; quoted fields; UTF-8 with or
//!   without a BOM). [`columns::propose_mapping`] maps its headers to the schedule's fields by
//!   the synonyms in [`columns::SYNONYMS`]; the caller can override the mapping and save it as
//!   JSON ([`columns::ColumnMapping`]).
//! - [`plan::plan_import`] makes a plan, not a change: for each row the sign that would be
//!   created or the reasons it cannot be. It needs no sign engine.
//! - [`apply::apply_plan`] applies the valid rows, through a [`apply::SignBuilder`] the app
//!   implements (it alone has the engine that lays a sign out).
//!
//! The README of this crate has the format, the synonyms and the limits.

pub mod apply;
pub mod columns;
pub mod csv;
pub mod export;
pub mod params;
pub mod plan;

pub use apply::{apply_plan, ApplyReport, DuplicatePolicy, SignBuilder};
pub use columns::{propose_mapping, ColumnEntry, ColumnMapping, Field, LengthUnit, MappingProposal, SYNONYMS};
pub use csv::{parse as parse_csv, CsvError, CsvTable};
pub use export::{export_csv, template_csv, CsvStyle, Export};
pub use params::{parse_legend, takes_legend, write_legend, PARAMS};
pub use plan::{
    alignments_in, existing_signs, parse_station, plan_import, ExistingSign, ImportPlan, IssueKind, PlacementRequest, PlanContext, PlanError, PlanRow,
    PlannedSign, RowIssue, RowOutcome,
};

/// The element category of a sign in the document (the app's traffic-signs module).
pub const SIGN_CATEGORY: &str = "sign";
/// The layer a created sign is put on.
pub const SIGN_LAYER: &str = "Signs";
/// Element property: the sign's description.
pub const PROP_DESCRIPTION: &str = "Description";
/// Element property: ground to the lower edge of the face, mm. The same key the supports crate
/// reads and writes.
pub const PROP_MOUNTING_HEIGHT_MM: &str = "Mounting height (mm)";
/// Element property: the nominal size the schedule gives (x-height, or the size the rule pack
/// tables for the sign), mm. It is recorded; the layout takes its size from the design speed.
pub const PROP_SIZE_MM: &str = "Nominal size (mm)";
/// Element property: free notes.
pub const PROP_NOTES: &str = "Notes";
