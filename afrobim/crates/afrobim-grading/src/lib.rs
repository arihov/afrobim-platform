// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Feature lines, sites and grading objects of a civil design package, as an engine with no user
//! interface. See the README for scope, methods and limits.

pub mod criteria;
pub mod error;
pub mod featureline;
pub mod geom;
pub mod grading;
pub mod group;
pub mod helpers;
pub mod output;
pub mod rays;
pub mod site;

#[cfg(test)]
mod tests;

pub use criteria::{Criterion, DaylightHit, Direction, Slope};
pub use error::{GradingError, Result};
pub use featureline::{DrapeReport, ElevationStep, FeatureLine, FilletReport, FlatPoint, GradeRow, Hold, JoinResult, QuickEdit, Vertex, VertexKind, WeedReport, WeedTolerances};
pub use grading::{infill_triangles, DaylightLine, Grading, GradingResult, RayResult, RayStatus, Region, RegionKind};
pub use group::{AdjustedVolume, BalanceEvaluation, BalanceOptions, BalanceResult, GradingGroup, GroupBuild, GroupDefinition, Infill, RebuildData, ShrinkSwell, StaleReason};
pub use helpers::{building_pad, layby_tie_in, pond, rim_region, sweep_template, DesignPackage, PlatformSurface, TemplateStep, TemplateSweep};
pub use output::{balance_csv, crossings_csv, grade_table_csv, rays_csv, slope_ticks, stage_storage_csv, volume_csv, GroupGeometry, SlopeTick};
pub use rays::{ray_geometry, ProjectionParams, RayGeom, RayKind, RaySet, Side};
pub use site::{CrossingReport, PriorityRule, Site};
