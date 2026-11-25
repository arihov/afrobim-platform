// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Construction stage: site progress, earned value and reporting.
//!
//! ```text
//!  site measurements --> ProgressLog (dated records per element, append-only, validated)
//!        |
//!        | Rollup (through the schedule crate's element links; weights: user, priced value or equal)
//!        v
//!  activity percent complete + inferred actual dates --> apply_updates --> Programme --> schedule()
//!        |
//!        | budget (user, or priced elements) + baseline dates --> PV; percent --> EV; entered AC
//!        v
//!  EvmEngine: BAC PV EV AC SV CV SPI CPI EAC ETC VAC TCPI, period table, S-curve, CSV,
//!             look-ahead, delay list;   qty_to_date --> afrobim_cost::certify
//! ```
//!
//! No rate, percentage, threshold or typical value is built in; the crate encodes no contract
//! terms. Actual cost is an input, never derived. See the README for the definitions.

mod error;
mod evm;
mod module;
mod payment;
mod record;
mod report;
mod rollup;

pub use error::{ProgressError, Result};
pub use evm::{
    ActivityEvm, ActualCost, ActualCosts, Budget, EacFormula, EacOutcome, EvmEngine, EvmResult, EvmSetup, Figure,
};
pub use module::{
    add_progress, load_progress, refresh_progress, store_progress, ProgressBook, ProgressModule, RefreshParams, StoredReport, CURVE_HEIGHT_MM,
    CURVE_WIDTH_MM, PROGRESS_CATEGORY, PROGRESS_LAYER,
};
pub use payment::{qty_to_date, PaymentQuantities};
pub use record::{ElementProgress, ElementTarget, Measurement, ProgressLog, Record};
pub use report::{
    delay_csv, delay_list, evm_activity_csv, evm_csv, look_ahead, look_ahead_csv, period_csv, s_curve_csv, s_curve_drawing, CurveLabel, CurvePoint,
    DelayRow, Event, LookAheadRow, PeriodRow, PeriodTable, SCurveData, DELAY_CSV_HEADER, EVM_ACTIVITY_CSV_HEADER, EVM_CSV_HEADER,
    LOOK_AHEAD_CSV_HEADER, PERIOD_CSV_HEADER, S_CURVE_CSV_HEADER,
};
pub use rollup::{
    apply_updates, ActivityProgress, ActivityUpdate, ActivityWeights, ApplyReport, InferredDate, Rollup, RollupResult, WeightBasis, WeightSources,
};

#[cfg(test)]
mod tests;
