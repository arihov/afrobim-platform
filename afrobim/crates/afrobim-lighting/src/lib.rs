//! Street (road) lighting as model elements: installations along an alignment and single
//! columns, with layout, a schematic plan and solids, a column schedule, quantities and a
//! report of requirement checks.
//!
//! # What governs
//!
//! UK practice is the core: DMRB **TD 501 Road lighting design, Version 0.1.0 (September 2025)**,
//! which applies BS 5489-1 and BS EN 13201. Those two British Standards are not freely readable
//! and TD 501 reproduces no table from them, so this crate holds **no lighting class table, no
//! luminance, illuminance or uniformity value, no maintenance factor and no spacing rule**. The
//! lighting class is a label the engineer enters. The [`design`] module checks only what TD 501
//! and the DMRB documents it names (TA 501, CD 109, CD 377) state in a form that can be applied
//! exactly, and says in each line where the requirement comes from. The MoWT *Road Design Manual
//! Volume 1: Geometric Design* (January 2010), section 12.8, is reported beside it as a local
//! reference; where it differs, the UK documents govern.
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
//! Photometry of any kind (illuminance, luminance, uniformity, glare, energy indicators), column
//! and foundation structural design (BS EN 40, CD 354), electrical design (cables, feeder
//! pillars, voltage drop, protection), energy and carbon assessment, tunnels, high mast
//! lighting, and costs. See the README.

pub mod design;
mod geometry;
mod layout;
mod model;
mod module;
mod quantities;

pub use geometry::{lighting_geometry, LightingGeometry, DISPLAY_COLUMN_DIAMETER_MM, DISPLAY_LUMINAIRE_MM, DISPLAY_PLAN_SYMBOL_RADIUS_MM};
pub use layout::{layout_installation, layout_single_column, whole_circle_bearing_deg, ColumnPosition, Layout, Side};
pub use model::{Arrangement, BracketAim, ColumnSpec, EndTreatment, Installation, LightingData, LightingDefinition, Luminaire, SingleColumn};
pub use module::{add_column, add_installation, lighting_of, refresh_layout, LightingModule, CATEGORY, LAYER};
pub use quantities::{
    schedule_to_csv, summary_to_csv, ColumnGroup, LightingQuantities, LuminaireGroup, ScheduleRow, SkippedElement, SCHEDULE_CSV_HEADER,
    SUMMARY_CSV_HEADER,
};

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
