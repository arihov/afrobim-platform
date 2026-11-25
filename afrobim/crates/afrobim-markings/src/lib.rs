// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Road markings as AfroBIM model elements, to the UK Traffic Signs Manual Chapter 5.
//!
//! This first cut holds the **longitudinal lines**: centre lines, lane lines, warning lines,
//! edge of carriageway lines and double white lines. The second set of markings (transverse
//! lines, arrows, hatching and chevrons, yellow lines, road studs) and the Uganda Traffic Signs
//! Manual's markings are extension elements (category [`CATEGORY_EXT`], [`ExtMarkingModule`],
//! [`ExtDefinition`]), chosen by [`Basis`]. What is left out is listed in the crate README.
//!
//! - [`catalogue`]: the line types with their mark, gap and widths ([`LineSpec`]), and the rows
//!   of the manual's tables that choose a diagram and width by speed limit and road layout
//!   ([`Application`]). Every number carries its citation; the governing edition is [`EDITION`].
//! - [`MarkingDefinition`]: the element. A line type, an alignment, two stations, an offset, a
//!   speed limit, an optional width, a colour and free-text material. [`MarkingDefinition::resolve`]
//!   checks it against the manual.
//! - [`layout`]: the marks laid along the alignment at the offset, following curves. States
//!   where the pattern starts and how a part module at the end is treated.
//! - [`MarkingModule`]: the kernel module for category [`CATEGORY`], giving a plan drawing and
//!   thin solids. [`add_marking`], [`set_marking`], [`sync_markings`] work on a document.
//! - [`MarkingTakeoff`]: lengths, marks and painted areas, per marking and by line type and
//!   width, as CSV.
//!
//! # Conventions
//!
//! Those of `afrobim-alignment`: metres; plan X east, Y north; stations increase along the
//! alignment; offsets are positive to the right looking up-station. "Left" and "right" of a
//! double line are looking up-station too. Geometry handed to the kernel is in millimetres.
//! Speed limits are in miles per hour, as the manual states them; km/h appears only as a
//! display conversion ([`speed_label`]).
//!
//! Only the horizontal geometry of the alignment is used. The road surface is taken as level
//! ground at z = 0.

pub mod basis;
pub mod catalogue;
mod definition;
#[cfg(test)]
mod ext_tests;
pub mod ext_catalogue;
mod ext_def;
pub mod ext_lookup;
mod ext_module;
pub mod layout;
mod module;
mod quantities;

pub use basis::{Basis, Lookup, NotDefined, UGANDA_EDITION, UK_EDITION};
pub use ext_catalogue::{ext_catalogue, ext_catalogue_for, ext_spec, ArrowDef, ArrowSize, CrossingDef, ExtSpec, Family, GeometryStatus, HatchDef, HatchModule, Kind, LinesDef, Reference, Sel, StrandDef, TaperedArrow, TransverseDef, TriangleDef, WordDef, ZigZagDef};
pub use ext_def::{polygon_area, ExtDefinition, ExtLaid, ExtResolved, Piece, Setting, Shape, CATEGORY_EXT};
pub use ext_lookup::{uganda_lane_arrow_siting, uganda_overtaking_sight_distance_m, deflection_row, lane_arrow_siting, lookup_family, lookup_type, stud_catalogue, stud_rows, uk_stud_spacing_on_bend, DeflectionRow, LaneArrowSiting, StudColour, StudRow, TypeRef, STUD_ID};
pub use ext_module::{add_ext_marking, ext_markings_to_csv, ext_marking_of, ext_plan, set_ext_marking, sync_ext_markings, ExtMarkingModule, ExtPlan, ExtQuantities, ExtTakeoff, SkippedExt, EXT_CSV_HEADER};
pub use catalogue::{
    applications, applications_for, catalogue, kmh, select, spec, speed_label, Application, DoubleLine, LineGroup, LineSpec, LineType, MarkingColour,
    MinMarks, Pattern, RoadContext, SpeedBand, EDITION, EDITION_URL,
};
pub use definition::{MarkingDefinition, Resolved};
pub use layout::{lay_out, Layout, LayoutError, Mark, PatternAnchor, StrandLayout, StrandSpec, CHORD_SAG_M, STATION_TOLERANCE_M};
pub use module::{add_marking, marking_of, plan, plan_at_level, set_marking, sync_markings, MarkingModule, MarkingPlan, CATEGORY, DISPLAY_THICKNESS_MM, LAYER};
pub use quantities::{
    markings_to_csv, summary_to_csv, MarkingGroup, MarkingQuantities, MarkingTakeoff, SkippedMarking, MARKINGS_CSV_HEADER, SUMMARY_CSV_HEADER,
};

/// Why a marking could not be created, changed or measured.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MarkingError {
    #[error("element {0} is not in the document")]
    ElementNotFound(u64),
    #[error("'{0}' is not an alignment that can be read")]
    NotAnAlignment(String),
    #[error("element {0} is not a road marking")]
    NotAMarking(u64),
    #[error("{0}")]
    InvalidDefinition(String),
}
