//! Concept design of road alignments.
//!
//! - [`HorizontalDesign`]: PIs with a radius and optional spirals; tangent points, spiral and
//!   curve elements, stations, and plain-words reasons when the geometry is impossible.
//! - [`VerticalDesign`]: PVIs with parabolic curves given by length or K; grades, curve ends,
//!   high and low points, levels at any station. [`fit_to_ground`] proposes a starting-point profile.
//! - Editing operations on both (`insert_pi`, `move_pi`, `delete_pi`, `set_radius`, ...): each
//!   returns the recomputed solution or the reason it is impossible, leaving the design unchanged.
//! - [`AlignmentDesign::build`] gives an `afrobim_alignment::Alignment` with its `Profile`, the
//!   same type an imported alignment has; [`write_landxml`] exports it.
//! - [`run_checks`]: design checks against limits the user supplies ([`DesignBasis`], from CSV or JSON), or
//!   against the built-in Uganda MoWT Road Design Manual Volume I (January 2010) basis, [`DesignBasis::mowt_2010`];
//!   the typed lookups of the manual's tables are in [`mowt`]. No other limits are built in.
//! - [`OptionSet`], [`compare_options`], [`score`], [`options_comparison_csv`]: several options compared on
//!   figures computed from the model, with the user's own weights.
//! - [`add_design`], [`store_design`], [`DesignModule`] and friends: the document elements.
//!
//! Metres and radians; plan X east, Y north; stations are internal stations. The README has the
//! formulae, the API examples and the limits.

mod checks;
mod design;
mod error;
mod fit;
mod horizontal;
mod landxml;
pub mod mowt;
mod module;
mod options;
pub mod physics;
mod vertical;

pub use checks::{run_checks, Bound, CheckItem, CheckOptions, CheckReport, CheckStatus, DesignBasis, StandardRow};
pub use design::{AlignmentDesign, DesignedAlignment};
pub use error::{DesignError, Problem};
pub use mowt::{builtin_bases, BuiltinBasisInfo, BuiltinSelection, MowtOptions};
pub use fit::{fit_to_ground, FitInputs, FitResult, FIT_LABEL};
pub use horizontal::{CurveData, HorizontalDesign, HorizontalSolution, TangentData};
pub use landxml::{write_landxml, LandXmlOptions};
pub use module::{
    add_design, add_option_set, compare_in_document, crossing_features_from_alignments, designs, load_design, load_option_set, plan_labels, register,
    store_design, store_option_set, DesignElement, DesignModule, OptionSetModule, PlanLabel, PlanLabelKind, ALIGNMENTS_LAYER, DESIGN_CATEGORY, DESIGN_LAYER,
    OPTIONS_CATEGORY,
};
pub use options::{
    compare_options, options_comparison_csv, crossings_with, figure_keys, score, scoring_csv, AlignmentOption, CompareInputs, OptionsComparison, Criterion, CrossingFeature,
    Direction, Figure, OptionResult, OptionScore, OptionSet, ScoreCell, ScoreTable,
};
pub use vertical::{CurveKind, CurveSpec, GradeData, VCurveData, VPvi, VerticalDesign, VerticalSolution};

pub use afrobim_alignment::Pi;
