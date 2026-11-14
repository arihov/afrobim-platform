//! Road pavement as AfroBIM model elements: a layer stack along an alignment, its typical
//! cross-section, 3D slabs, layer quantities, and a pavement structure design check.
//!
//! The crate has two parts, kept apart on purpose.
//!
//! **Modelling** ([`PavementDefinition`], [`cross_section`], [`solids`], [`PavementQuantities`],
//! [`PavementModule`]). Every dimension is entered by the engineer or written from a design
//! result. The crate draws those dimensions and measures them by plain geometry. It holds no
//! rates, densities or costs. The only values from a standard on this side are the reminders in
//! [`PavementDefinition::notes`], each quoted with its clause.
//!
//! **Design** ([`design`]). One sub-module per published method; which governs is the
//! engineer's choice. Each states in its own documentation what it covers, what stays an input
//! and what it does not do. The result is a calculation for an engineer to check, not a
//! certified design.
//!
//! # Conventions
//!
//! Stations, widths and offsets are in metres, thicknesses in millimetres, cross-fall in per
//! cent. Offsets are positive to the right looking up-station, as in `afrobim-alignment`; only
//! that crate's horizontal geometry is used. Drawings and solids are in millimetres. The frames
//! of the cross-section and of the solids are described on [`CrossSection`] and
//! [`PavementSolids`]; the quantity formulae in the `geometry` module source and the README.

pub mod design;
mod geometry;
mod model;
mod module;
mod quantities;
mod text;

pub use geometry::{cross_section, heading_change_rad, solids, strip_area_m2, CrossSection, PavementSolids, SectionLabel, SectionOptions};
pub use model::{CrossfallShape, Layer, Material, PavementDefinition, Shoulder, ShoulderLayers, Side, Strip, StripKind};
pub use module::{add_pavement, pavements, sync_alignments, update_pavement, PavementModule, PAVEMENT_CATEGORY, PAVEMENT_LAYER};
pub use quantities::{
    layers_to_csv, measure_definition, summary_to_csv, LayerQuantity, LayerTotal, PavementQuantities, SkippedElement, LAYERS_CSV_HEADER,
    SUMMARY_CSV_HEADER,
};

/// Why a pavement could not be created or measured.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PavementError {
    #[error("element {0} is not a pavement in the document")]
    ElementNotFound(u64),
    #[error("the pavement definition is not valid: {0}")]
    InvalidDefinition(String),
}
