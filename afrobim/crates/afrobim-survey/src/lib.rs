// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Survey database engine: networks, control points, setups, observations, figures and import
//! events; reductions with a stated correction chain; field book import; traverse closure; and least
//! squares network adjustment with quality measures. No user interface. See the README for scope,
//! formats, methods with citations and limits.
//!
//! Units: metres, decimal degrees (zenith angles), f64, coordinates exactly as given (tested near
//! northing 9 960 000 m). Every standard error, tolerance, refraction coefficient and earth radius is an
//! input of the database or of the option structures; none has a built-in value.

pub mod adjust;
pub mod csv;
pub mod error;
pub mod export;
pub mod fbk;
pub mod fieldbook;
pub mod figures;
pub mod gsi;
pub mod hash;
pub mod import;
pub mod level;
pub mod linalg;
pub mod lsq;
pub mod model;
pub mod reduce;
pub mod report;
pub mod stats;
pub mod topcon;
pub mod traverse;
pub mod units;

pub use adjust::{adjust_network_2d, adjust_with_snooping, AdjustOptions, AdjustedPoint, Adjustment, Ellipse, GlobalTest, ObsKind, ObsResult, RelativePrecision, SnoopResult};
pub use csv::{CsvColumn, CsvMapping, CsvRole};
pub use error::{Result, SurveyError};
pub use export::{into_store, to_cogo_points};
pub use fbk::FbkOptions;
pub use fieldbook::{parse_fieldbook, write_fieldbook};
pub use figures::{flatten_figure, process_linework, Breakline, Figure, FigurePrefix, FigureVertex, LineworkCodes, VertexKind};
pub use gsi::{GsiBacksight, GsiOptions};
pub use import::{DuplicatePolicy, ImportFormat, ParsedData};
pub use level::{adjust_levelling, LevelAdjustment};
pub use model::*;
pub use reduce::{compute_points, reduce_setup, ComputeReport, ReducedSet, Spread};
pub use report::{adjustment_report, level_report};
pub use stats::{chi2_cdf, chi2_quantile, normal_two_sided_critical};
pub use topcon::TopconOptions;
pub use traverse::{traverse_from_setups, TraverseClosure, TraverseReport, TraverseSpec};
pub use units::{AngleUnit, HzDirection, VerticalKind};
