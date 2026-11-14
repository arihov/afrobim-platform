//! Road restraint systems: safety barrier runs along an alignment as model elements, with
//! quantities and requirement checks.
//!
//! # Two parts
//!
//! **Modelling.** A [`BarrierRun`] is a length of safety barrier between two stations of an
//! alignment at a signed offset, with its declared performance classes (containment level,
//! working width class, impact severity level), its end treatments and, as entered product
//! data, its post spacing and beam height. [`resolve`] works it out against the alignment
//! ([`ResolvedRun`]): the offset line, its exact length, the post positions, levels from the
//! profile. From that come the plan [`plan_drawing`], the schematic [`solids`] and the take-off
//! ([`RestraintQuantities`]). This side holds no requirement and makes no claim of compliance.
//!
//! **Requirements** ([`design`]). The checks DMRB CD 377 Version 5.0.1 and CD 127 Version 1.0.1
//! state in a form that can be applied exactly, each with its clause, set out line by line. The
//! module's own documentation says what it covers and what it leaves out. It does **not**
//! perform or imitate the Road Restraint Risk Assessment Process (RRRAP) that CD 377 requires,
//! and its checks do not replace it.
//!
//! # Units and conventions
//!
//! Metres, as `afrobim-alignment`: internal stations, offsets positive to the right looking
//! up-station. Drawings and solids handed to the kernel are in millimetres. Speed limits are
//! taken in the unit entered ([`design::SpeedLimit`]) because CD 377 states its thresholds in
//! miles per hour.
//!
//! # Not in this crate
//!
//! Vehicle parapet design (a parapet is only something a run can connect to), crash cushions,
//! pedestrian restraint, motorcyclist protection, temporary barriers, the RRRAP, product
//! catalogues, rates and costs. See the README.
//!
//! The output is for an engineer to check. It is not a certified design.

mod classes;
pub mod design;
mod geometry;
mod module;
mod products;
mod quantities;
mod run;

pub use classes::{
    ContainmentGroup, ContainmentLevel, Coverage, ImpactSeverityLevel, ImpactTest, TerminalClass, TestVehicle, VehicleIntrusionClass, WorkingWidthClass,
    THIV_LIMIT_KMH,
};
pub use geometry::{
    bay_count, plan_drawing, post_count, resolve, solids, solids_origin, ResolvedRun, RestraintPlan, RunPoint, MAX_POSTS, PLAN_POST_TICK_M,
    POST_TOLERANCE_M, SCHEMATIC_BEAM_DEPTH_MM, SCHEMATIC_BEAM_THICKNESS_MM, SCHEMATIC_POST_SIZE_MM,
};
pub use module::{
    add_barrier_run, alignment_named, barrier_run_of, barrier_runs, sync_barrier_runs, update_barrier_run, BarrierElement, RestraintModule,
    CATEGORY, NOTE_LEVELS_ZERO, NOTE_SCHEMATIC, RESTRAINT_LAYER,
};
pub use products::{parse_products_csv, Product, ProductCsvError, PRODUCTS_CSV_HEADER};
pub use quantities::{runs_to_csv, summary_to_csv, ClassTotal, RestraintQuantities, RunQuantities, SkippedRun, RUNS_CSV_HEADER, SUMMARY_CSV_HEADER};
pub use run::{BarrierRun, ConnectedSystem, EndTreatment, TrafficFace, Transition};

/// Why a run could not be created, resolved or measured.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RestraintError {
    #[error("the barrier run is not valid: {0}")]
    InvalidRun(String),
    #[error("stations {start_m} m to {end_m} m are not all on alignment '{alignment}', which runs from {from_m} m to {to_m} m")]
    OffAlignment { start_m: f64, end_m: f64, alignment: String, from_m: f64, to_m: f64 },
    #[error("an offset of {offset_m} m is at or beyond the centre of the curve at station {station_m} m")]
    OffsetBeyondCurveCentre { offset_m: f64, station_m: f64 },
    #[error("a post spacing of {spacing_m} m gives {posts} posts; check the spacing")]
    TooManyPosts { posts: usize, spacing_m: f64 },
    #[error("there is no alignment named '{0}' in the document")]
    AlignmentNotFound(String),
    #[error("element {0} is not in the document")]
    ElementNotFound(u64),
    #[error("'{0}' is not a barrier run or cannot be read: {1}")]
    NotABarrierRun(String, String),
}

/// `v` rounded to `max_decimals`, with trailing zeros removed down to `min_decimals`.
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
