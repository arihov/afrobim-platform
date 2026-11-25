// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Street (road) lighting as model elements: installations along an alignment and single
//! columns, with layout, a schematic plan and solids, a column schedule, quantities and a
//! report of requirement checks.
//!
//! # What governs
//!
//! UK practice is the core: DMRB **TD 501 Road lighting design, Version 0.1.0 (September 2025)**,
//! which applies BS 5489-1 and BS EN 13201 and reproduces no table from them. The current
//! editions of those standards have not been read. The owner's copy of **BS 5489-1:2003, a
//! superseded edition**, is held in [`bs5489_2003`] (column clearances, class selection tables,
//! luminaire maintenance factors) and is labelled as superseded wherever it is used. BS EN
//! 13201-2 is not held; the class values of EN 13201-2:2015 and the class selection of CEN/TR
//! 13201-1:2014 are taken from a **secondary source**, Stockmar's *Handbook about Interpretation
//! of EN 13201* (LiTG, 2017), in [`en13201`], and are to be confirmed against the standard. There
//! is **no spacing rule**. The class check [`design::check_lighting`] compares calculated results
//! with the class values; the engineer enters them, or [`calc`] computes what can be computed
//! correctly from a luminaire file ([`photometry`]: IES LM-63 and EULUMDAT): horizontal, vertical
//! and semi-cylindrical illuminance on user-defined grids, the edge illuminance ratio, and
//! luminance when the user supplies an r-table. The lighting class is a label the engineer
//! enters.
//! The [`design`] module checks only what these documents and the DMRB documents TD 501 names
//! (TA 501, CD 109, CD 377) state in a form that can be applied exactly, and says in each line
//! where the requirement comes from. The MoWT *Road Design Manual Volume 1: Geometric Design*
//! (January 2010), section 12.8, and TII DN-LHT-03038 (Ireland, February 2012) are reported
//! beside it as secondary references; where they differ, TD 501 governs.
//!
//! # What is modelled
//!
//! - [`Installation`]: columns at a spacing between two stations of an alignment, in one of five
//!   [`Arrangement`]s. [`layout_installation`] turns it into [`ColumnPosition`]s.
//! - [`SingleColumn`]: one column at a station and offset (junctions, conflict areas).
//! - [`LightingModule`]: the kernel module (category `lighting`). It draws a schematic plan and
//!   schematic solids from the positions stored in the element.
//! - [`LightingQuantities`], [`schedule_to_csv`], [`summary_to_csv`]: the take-off.
//! - [`design::check_lighting`]: the requirement checks, line by line.
//! - [`en13201`]: the M, C and P class requirements and the CEN/TR 13201-1 weighting selection
//!   (a suggestion), as reproduced in the Stockmar handbook.
//! - [`photometry`]: IES and EULUMDAT readers, flux and peak checks. [`calc`]: illuminance
//!   grids, drawings, CSV, and storage of the run on the element.
//! - [`bs5489_2003`]: the tables of the superseded BS 5489-1:2003, in one module so that a
//!   later edition can replace it.
//!
//! The geometry is a diagram: a column of a nominal display diameter, a straight horizontal
//! bracket and a block for the luminaire, at the entered mounting height and outreach. It is not
//! a product model and no dimension of it other than the mounting height, the outreach, the tilt
//! and the positions is entered data.
//!
//! # Conventions
//!
//! Those of `afrobim-alignment`: metres; plan X east, Y north; stations are internal
//! (continuous) stations, shown to the user through `Alignment::station_label`; offsets are
//! positive to the right looking up-station; directions are radians counter-clockwise from +X.
//! [`whole_circle_bearing_deg`] turns a direction into a bearing clockwise from north. Drawings
//! and solids handed to the kernel are in millimetres.
//!
//! # Not in this crate
//!
//! Threshold increment and other glare, the EN 13201-3 calculation grid and observer positions,
//! r-table values, energy indicators, the HS, SC and EV classes, column
//! and foundation structural design (BS EN 40, CD 354), electrical design (cables, feeder
//! pillars, voltage drop, protection), energy and carbon assessment, tunnels, high mast
//! lighting, and costs. See the README.

pub mod bs5489_2003;
pub mod calc;
pub mod design;
pub mod en13201;
mod geometry;
mod layout;
mod model;
mod module;
pub mod photometry;
mod quantities;

pub use geometry::{lighting_geometry, LightingGeometry, DISPLAY_COLUMN_DIAMETER_MM, DISPLAY_LUMINAIRE_MM, DISPLAY_PLAN_SYMBOL_RADIUS_MM};
pub use layout::{layout_installation, layout_single_column, whole_circle_bearing_deg, ColumnPosition, Layout, Side};
pub use model::{Arrangement, BracketAim, ColumnSpec, EndTreatment, Installation, LightingData, LightingDefinition, Luminaire, SingleColumn};
pub use module::{add_column, add_installation, lighting_of, refresh_layout, LightingModule, CATEGORY, LAYER};
pub use quantities::{
    schedule_to_csv, summary_to_csv, ColumnGroup, LightingQuantities, LuminaireGroup, ScheduleRow, SkippedElement, SCHEDULE_CSV_HEADER,
    SUMMARY_CSV_HEADER,
};

pub use photometry::{load_photometry_file, parse_ies, parse_ldt, parse_photometry_bytes, Photometry};

use afrobim_kernel::{Note, Severity};

/// Why a lighting element could not be created, laid out or measured.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LightingError {
    #[error("element {0} is not in the document")]
    ElementNotFound(u64),
    #[error("element {0} is not a lighting element")]
    NotLighting(u64),
    #[error("the definition is not valid: {0}")]
    InvalidDefinition(String),
    #[error("station {station_m:.3} m is not on alignment '{alignment}' ({start_m:.3} m to {end_m:.3} m)")]
    StationOffAlignment { alignment: String, station_m: f64, start_m: f64, end_m: f64 },
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

/// The Error notes of a list joined into one message, or None when there are none.
pub(crate) fn error_text(notes: &[Note]) -> Option<String> {
    let errors: Vec<&str> = notes.iter().filter(|n| n.severity == Severity::Error).map(|n| n.message.as_str()).collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.join(" "))
    }
}

/// `v` with at most `max_decimals` decimals, trailing zeros trimmed down to `min_decimals`.
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
