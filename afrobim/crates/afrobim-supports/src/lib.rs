// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Sign supports: posts and foundations as model elements.
//!
//! The modelling side of this crate takes dimensions and turns them into solids, a sheet
//! elevation and quantities by plain geometry. It holds no table of steel grades, concrete
//! classes, embedment rules or clearances and makes no claim of compliance.
//!
//! The [`design`] module is separate: it sizes a steel CHS post and a planted foundation for
//! wind to the UK method (IHE Sign Structures Guide 2021, BS EN 12899-1), and states in its own
//! documentation what it covers, what it leaves as inputs and what it does not do.
//!
//! The only built-in numbers are the `PLACEHOLDER_*` constants (round starting values so that a
//! new element is drawable; every element created from them carries an Info note saying so) and
//! [`STEEL_DENSITY_KG_M3_NOMINAL`], which callers can override.
//!
//! # Frames
//!
//! Solids follow [`afrobim_draw::Mesh3`]: millimetres, the element's own frame, +X along the
//! element's width from its lower-left corner, +Y into its depth, +Z up. For supports **z = 0 is
//! ground level**: the buried part of a post and the whole of a buried foundation have negative
//! z. The plan bounding box of the element sits with its corner on the origin, so a post of
//! section width `w` and depth `d` occupies x ∈ [0, w], y ∈ [0, d] and its axis is the vertical
//! line through (w/2, d/2).
//!
//! The 2D drawing is an elevation seen looking along +Y (the same side a sign face is read
//! from): drawing X is the frame's X, drawing Y is height, and the lowest point of the element
//! is on drawing y = 0.
//!
//! See the crate README for how each quantity is computed.

mod arrangement;
pub mod design;
mod foundation;
mod hosting;
mod post;
mod quantities;
mod section;

pub use arrangement::SupportArrangement;
pub use foundation::{FoundationDefinition, FoundationModule, FoundationShape, Socket, FOUNDATION_CATEGORY};
pub use hosting::{
    add_supports, remove_supports, replace_supports, supports_of, sync_support_sites, PROP_INDEX, PROP_MOUNTING_HEIGHT_MM,
    PROP_POSITION_ALONG_SIGN_MM, PROP_SETBACK_MM, PROP_SIGN, SUPPORTS_LAYER,
};
pub use post::{PostDefinition, PostModule, POST_CATEGORY};
pub use quantities::{
    foundations_to_csv, posts_to_csv, summary_to_csv, FoundationQuantities, PostQuantities, QuantityOptions, SkippedElement,
    SupportQuantities, FOUNDATIONS_CSV_HEADER, POSTS_CSV_HEADER, SUMMARY_CSV_HEADER,
};
pub use section::Section;

use afrobim_kernel::{Note, Severity};

/// The conventional nominal density of structural steel used for mass estimates, kg/m³.
///
/// This is a convenience for estimating mass from volume. It is not a specification value and
/// is not taken from any standard held in this repository; pass the density your specification
/// gives to [`QuantityOptions`] or [`PostModule::with_density`] to override it.
pub const STEEL_DENSITY_KG_M3_NOMINAL: f64 = 7850.0;

/// PLACEHOLDER starting dimensions, used only so that a new element can be drawn before the
/// engineer has entered real values. They are deliberately round, come from no standard and are
/// not adequate, typical or compliant values. Definitions built from them have `placeholder`
/// set, which makes the module raise the `PLACEHOLDER_DIMENSIONS` Info note.
pub const PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM: f64 = 100.0;
/// PLACEHOLDER, see [`PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM`].
pub const PLACEHOLDER_POST_WALL_MM: f64 = 10.0;
/// PLACEHOLDER, see [`PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM`].
pub const PLACEHOLDER_POST_LENGTH_MM: f64 = 3000.0;
/// PLACEHOLDER, see [`PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM`].
pub const PLACEHOLDER_POST_EMBEDMENT_MM: f64 = 1000.0;
/// PLACEHOLDER side of the cubic foundation block, see [`PLACEHOLDER_POST_OUTSIDE_DIAMETER_MM`].
pub const PLACEHOLDER_FOUNDATION_SIDE_MM: f64 = 1000.0;

/// Number of sides of the prism that stands for a round solid in the 3D view. Quantities never
/// come from the mesh; they use the exact circle formulae.
pub const ROUND_SEGMENTS: u32 = 48;

/// Note code raised (Info) on an element whose dimensions are still the placeholders.
pub const NOTE_PLACEHOLDER: &str = "PLACEHOLDER_DIMENSIONS";

/// Why a support could not be created or measured.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SupportError {
    #[error("element {0} is not in the document")]
    ElementNotFound(u64),
    #[error("the support arrangement is not valid: {0}")]
    InvalidArrangement(String),
    #[error("the definition is not valid: {0}")]
    InvalidDefinition(String),
    #[error("'{0}' already has supports; remove them first or use replace_supports")]
    AlreadySupported(String),
    #[error("{0}")]
    InvalidOption(String),
}

pub(crate) fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

pub(crate) fn error_note(code: &str, message: String) -> Note {
    Note { severity: Severity::Error, code: code.to_string(), message }
}

pub(crate) fn info_note(code: &str, message: &str) -> Note {
    Note { severity: Severity::Info, code: code.to_string(), message: message.to_string() }
}

/// The Error notes of a list joined into one message, or None when there are none.
pub(crate) fn error_text(notes: &[Note]) -> Option<String> {
    let errors: Vec<&str> = notes.iter().filter(|n| n.severity == Severity::Error).map(|n| n.message.as_str()).collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.join(" "))
    }
}
