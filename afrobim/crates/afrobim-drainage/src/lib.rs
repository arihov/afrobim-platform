// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Road drainage: side drains and pipe culverts as model elements.
//!
//! The crate has two parts, as `afrobim-supports` does.
//!
//! **Modelling** ([`SideDrainDefinition`], [`PipeCulvertDefinition`], their kernel modules and
//! the take-off in [`DrainQuantities`] and [`CulvertQuantities`]). Every dimension, level and
//! gradient is entered by the engineer. The crate draws them and measures them by plain
//! geometry. The culvert module raises notes against the few layout limits the chosen design
//! basis states in words (minimum diameter, skew, gradient); each note cites its clause.
//!
//! **Design** ([`design`]). Open-channel flow by Manning's equation under any of three bases
//! the engineer chooses between ([`design::DesignBasis`]): the Uganda Ministry of Works and
//! Transport *Road Design Manual Volume 2: Drainage Design* (January 2010), which also gives
//! peak flow by the Rational method and culvert outlet-control headwater ([`design::mowt`]),
//! the UK *Design Manual for Roads and Bridges* ([`design::dmrb`]) and the South African
//! SANRAL *Drainage Manual*, sixth edition ([`design::sanral`]). Culvert inlet control and the
//! complete culvert check under the MoWT basis are [`design::inlet`] and
//! [`design::culvert_check`]. Each module's documentation states what it covers, what stays an
//! input and what is not done.
//!
//! # Frames and conventions
//!
//! * Alignment conventions are those of `afrobim-alignment`: stations and offsets in metres,
//!   offsets positive to the right looking up-station, plan X east and Y north.
//! * "Left" and "right" of a channel section are seen looking up-station.
//! * Side slopes are horizontal to one vertical: a slope of 1.5 is 1.5 H : 1 V.
//! * Element dimensions are in millimetres, as everywhere in the kernel; levels are in metres
//!   above the project datum; the hydraulics work in metres and m³/s.
//! * A side drain's solids are in a frame whose origin is the drain's centreline at its start
//!   station, **not rotated** (local +X east, +Y north), with z = 0 at the top edges of the
//!   drain when the alignment has no vertical profile. Its site placement is that point with
//!   rotation 0 and z = 0. Where the alignment has a profile, the top edges of the solid lie at
//!   the profile level at each station (the level at the alignment, held at its end levels beyond
//!   its ends; the cross-fall of the road and the ground are not followed), so the solid rises
//!   and falls with the road. The entered gradient is used for hydraulics only.
//! * Where the alignment has a profile a culvert's site placement is at the datum level of its
//!   inlet invert (the levels are entered in metres above datum), so it agrees with a road at
//!   profile level; with no profile the road is taken at level zero, as before.
//! * A culvert's solids are in a frame whose origin is the inlet end of the barrel group's
//!   centre line at inlet invert level, +X along the plan direction of flow, +Y to the left of
//!   the flow, +Z up. Its site placement carries that point and direction.
//!
//! The only built-in dimensions are the `PLACEHOLDER_*` constants: round starting values so a
//! new element is drawable. Every element made from them carries an Info note saying so.

mod culvert;
pub mod design;
mod document;
mod drain;
mod quantities;
mod section;

pub use culvert::{PipeCulvertDefinition, PipeCulvertModule, Side, CULVERT_CATEGORY};
pub use document::{add_pipe_culvert, add_side_drain, alignment_named, measure_document, sync_drainage, CULVERTS_LAYER, DRAINS_LAYER};
pub use drain::{DrainRoute, RoutePoint, SideDrainDefinition, SideDrainModule, DRAIN_CATEGORY};
pub use quantities::{
    culverts_to_csv, drains_to_csv, CulvertQuantities, DrainQuantities, DrainageQuantities, SkippedElement, CULVERTS_CSV_HEADER,
    DRAINS_CSV_HEADER,
};
pub use section::{ChannelSection, Lining};

use afrobim_kernel::{Note, Severity};

/// PLACEHOLDER starting dimensions, used only so that a new element can be drawn before the
/// engineer has entered real values. They are round numbers, come from no standard and are not
/// adequate, typical or compliant values. Definitions built from them have `placeholder` set,
/// which makes the module raise the [`NOTE_PLACEHOLDER`] Info note.
pub const PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM: f64 = 500.0;
/// PLACEHOLDER, see [`PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM`].
pub const PLACEHOLDER_DRAIN_DEPTH_MM: f64 = 500.0;
/// PLACEHOLDER side slope (horizontal to one vertical), see [`PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM`].
pub const PLACEHOLDER_DRAIN_SIDE_SLOPE: f64 = 1.0;
/// PLACEHOLDER, see [`PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM`].
pub const PLACEHOLDER_CULVERT_DIAMETER_MM: f64 = 1000.0;
/// PLACEHOLDER, see [`PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM`].
pub const PLACEHOLDER_CULVERT_WALL_MM: f64 = 100.0;
/// PLACEHOLDER, see [`PLACEHOLDER_DRAIN_BOTTOM_WIDTH_MM`].
pub const PLACEHOLDER_CULVERT_LENGTH_MM: f64 = 10_000.0;

/// Number of sides of the prism that stands for a pipe in the 3D view. Quantities never come
/// from the mesh.
pub const ROUND_SEGMENTS: u32 = 32;

/// Thickness given to an unlined drain's trough in the 3D view, mm, so that a cut in the
/// ground can be seen as a solid. A screen convention only: it is in no quantity.
pub const UNLINED_DISPLAY_SKIN_MM: f64 = 50.0;

/// Note code raised (Info) on an element whose dimensions are still the placeholders.
pub const NOTE_PLACEHOLDER: &str = "PLACEHOLDER_DIMENSIONS";

/// Why a drainage element could not be created, routed, measured or checked.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DrainageError {
    #[error("element {0} is not in the document")]
    ElementNotFound(u64),
    #[error("'{0}' is not an alignment that can be read")]
    NotAnAlignment(String),
    #[error("the definition is not valid: {0}")]
    InvalidDefinition(String),
    #[error("the element cannot be placed on the alignment: {0}")]
    Route(String),
    #[error("{0}")]
    InvalidInput(String),
}

/// The level of the alignment's vertical profile at a station, m above datum. Beyond the ends of
/// the profile the end levels are held. `None` when the alignment has no profile.
pub(crate) fn profile_level_m(alignment: &afrobim_alignment::Alignment, station_m: f64) -> Option<f64> {
    let profile = alignment.profile.as_ref()?;
    let (lo, hi) = profile.range()?;
    profile.elevation_at(station_m.clamp(lo, hi)).ok()
}

pub(crate) fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

pub(crate) fn not_negative(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

pub(crate) fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

pub(crate) fn error_note(code: &str, message: String) -> Note {
    note(Severity::Error, code, message)
}

pub(crate) fn warning_note(code: &str, message: String) -> Note {
    note(Severity::Warning, code, message)
}

pub(crate) fn info_note(code: &str, message: &str) -> Note {
    note(Severity::Info, code, message.to_string())
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

/// A number with at least `min_decimals` and at most `max_decimals` decimals, trailing zeros
/// dropped in between, and no negative zero.
pub(crate) fn fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    let mut s = format!("{v:.max_decimals$}");
    if let Some(dot) = s.find('.') {
        while s.len() > dot + 1 + min_decimals && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.starts_with('-') && s.chars().all(|c| matches!(c, '-' | '0' | '.')) {
        s.remove(0);
    }
    s
}
