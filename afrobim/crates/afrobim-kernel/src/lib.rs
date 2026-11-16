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

mod controller;
pub mod crs;
mod document;
mod drafting;
mod geometry;
mod module;
mod settings;
pub mod speed_band;
mod undo;
mod view;

pub use afrobim_draw::Pt2 as Pt;
pub use controller::{CanvasCommand, CanvasController, CanvasEvent, CanvasTool};
pub use document::{Document, DocumentError, Element, ElementId, Layer, SelectMode, SitePlacement, FILE_EXTENSION};
pub use drafting::{display_spacing, ortho, parse_point, snap};
pub use geometry::BBox;
pub use module::{Generated, Module, ModuleRegistry, Note, Severity};
pub use crs::{AreaOfUse, CrsCheck, CrsFinding, CrsKind, CrsSource, FindingKind, IncomingDataset, LinearUnit, PlanExtent, ProjectCrs, TransverseMercator};
pub use settings::{
    project_crs, set_project_crs, PROJECT_CRS_KEY, project_setting, project_settings_id, set_project_setting, ProjectSettingsModule, PROJECT_SETTINGS_CATEGORY, PROJECT_SETTINGS_LAYER,
};
pub use speed_band::{set_speed_band_rule, speed_band_rule, SpeedBandRule};
pub use undo::UndoHistory;
pub use view::ViewTransform;
