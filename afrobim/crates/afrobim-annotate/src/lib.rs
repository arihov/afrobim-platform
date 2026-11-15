//! Annotation engine for AfroBIM: text, dimensions, leaders, measuring, display units, styles
//! and layer helpers. See the crate README for the model, the rules and copy-ready examples.
//!
//! Annotations are ordinary document elements (see [`element`]), so the kernel's selection,
//! move, copy, erase and save work on them unchanged. Everything they draw is first reduced to an
//! [`geometry::AnnGeometry`], which feeds the on-screen drawing, the DXF export and the sheets.
//!
//! Every size is a default the user edits; no drafting standard is claimed anywhere in this crate.

pub mod dim;
pub mod element;
mod font;
pub mod geometry;
pub mod layers;
pub mod leader;
pub mod measure;
pub mod output;
pub mod settings;
pub mod styles;
pub mod text;
pub mod units;

pub use dim::{station_offset_cache, ChainMode, DimCache, DimData, DimKind, DimResult, LevelSource, LinearOrient};
pub use element::{
    add_annotation, add_dimension, add_leader, add_text, geometry_absolute, is_annotation, normalize, read, read_absolute, refresh, register, replace_absolute, write, Ann,
    AnnotationModule, RefreshReport, DEFAULT_LAYER, DIM_CATEGORY, LEADER_CATEGORY, TEXT_CATEGORY,
};
pub use geometry::{AnnGeometry, Fill, FillKind, Label, Pt, Seg};
pub use layers::{LayerError, LayerInfo, OnDelete};
pub use leader::{LeaderData, LeaderText};
pub use measure::{MeasureResult, Measurement, Winding};
pub use output::{annotation_items, export_dxf, sheet_drawing, AnnItem, ExportReport};
pub use settings::{AnnotationSettings, LayerProps, SETTINGS_CATEGORY, SETTINGS_LAYER};
pub use styles::{DimStyle, HeightSpec, TextPlacement, TextStyle, Terminator};
pub use text::{Justify, TextData};
pub use units::{AngleFormat, AreaUnit, DisplayUnits, LengthUnit, ParseError};
