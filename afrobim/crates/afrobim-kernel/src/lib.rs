// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! AfroBIM kernel: the document every module and front end shares.
//!
//! A [`Document`] is a set of [`Element`]s. An element is a typed building-information
//! component: it has an identity, a category that names the module responsible for it, a
//! placement in model space (millimetres, Y up), a module-owned definition (`data`) and a
//! free-form property set. Modules turn an element into geometry through the [`Module`] trait;
//! the kernel never knows what a sign or a road marking is.
//!
//! The kernel also owns everything about editing that does not depend on a GUI toolkit: the view
//! transform, picking, window and crossing selection, drafting aids, the command state machine
//! and undo. Front ends forward input and draw state.

pub mod atomic;
mod controller;
pub mod crs;
pub mod crs_plan;
pub mod crs_transform;
mod document;
mod drafting;
mod geometry;
pub mod layout;
mod module;
mod settings;
pub mod speed_band;
mod undo;
mod view;

pub use afrobim_draw::Pt2 as Pt;
pub use atomic::{recover_path, write_atomic, write_atomic_with};
pub use controller::{CanvasCommand, CanvasController, CanvasEvent, CanvasTool};
pub use document::{DocState, Document, DocumentError, Element, ElementId, Layer, SelectMode, SitePlacement, FILE_EXTENSION};
pub use layout::{Layout, LayoutError, LayoutList, PaperShape, Viewport, VpGrip, MODEL_TAB, STANDARD_SCALES};
pub use drafting::{display_spacing, display_spacing_fine, ortho, parse_point, snap};
pub use geometry::BBox;
pub use module::{Generated, Module, ModuleRegistry, Note, Severity};
pub use crs::{crs_from_model_point, elevation_m_to_mm, elevation_mm_to_m, m_to_mm, mm_to_m, model_point_from_crs, supported_epsg_codes, AreaOfUse, CrsCheck, CrsFinding, CrsKind, CrsSource, FindingKind, IncomingDataset, LinearUnit, PlanExtent, ProjectCrs, TransverseMercator};
pub use crs_plan::{
    combined_scale_factor, elevation_factor, plan_transform, BatchResult, CalibrationError, ControlPair, ControlResidual, CrsRef, DataExtent, HeightAssumption, NoPath, PlanKind, PointEffects, SiteCalibration, StepKind,
    StepRecord, TransformError, TransformLog, TransformPlan, TransformedPoint,
};
pub use crs_transform::{geographic_crs, transformations, transformations_between, transformations_containing, GeographicCrs, Transformation};
pub use settings::{
    clear_project_crs, project_crs, set_project_crs, PROJECT_CRS_KEY, project_setting, project_settings_id, set_project_setting, ProjectSettingsModule, PROJECT_SETTINGS_CATEGORY, PROJECT_SETTINGS_LAYER,
};
pub use speed_band::{set_speed_band_rule, speed_band_rule, SpeedBandRule};
pub use undo::UndoHistory;
pub use view::ViewTransform;
