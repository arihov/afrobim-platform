//! Point cloud engine for a road BIM application: survey scans, drone photogrammetry and lidar as
//! reference data and as a source for ground surfaces. No user interface.
//!
//! See the README for the formats, the methods with their citations, the measured performance and
//! the limits. Coordinates are `f64`, exactly as the file gives them after its scale and offset
//! (true UTM magnitudes); stored data is never recentred. A local origin is used only in
//! [`DisplayBuffer`]s, which return it.

pub mod buffer;
pub mod cache;
pub mod classes;
pub mod color;
pub mod compare;
pub mod error;
pub mod export;
pub mod filter;
pub mod ground;
pub mod header;
pub mod index;
pub mod reference;
pub mod section;
pub mod source;
pub mod stats;
pub mod surface;
pub mod text;

pub use buffer::{Bounds3, PointBuffer, PointRecord};
pub use cache::{cache_path_for, open_indexed, sha256_file, CacheStatus, OpenedIndex};
pub use classes::{class_label, class_name, ClassTable};
pub use color::{colorize, ClassPalette, ColorMode, ColorRamp, RgbScale};
pub use compare::{compare_to_tin, BandColors, Comparison, DifferenceBands};
pub use error::{PointCloudError, Result};
pub use export::{export_las, export_stream, export_text, ClipRegion, ExportOptions, ExportReport, LasVersion, TextExportOptions};
pub use filter::{CompiledFilter, PointFilter, ReturnFilter};
pub use ground::{exponential_windows, ground_from_classes, grid_minimum_slope_filter, linear_windows, progressive_morphological_filter, GridMinSlopeParams, GroundResult, PmfParams};
pub use header::{epsg_from_wkt, CrsInfo, GeoKeyEntry, GeoKeyValue, HeaderInfo, SourceKind};
pub use index::{DisplayBuffer, IndexOptions, LodResult, Neighbor, Overflow, PointIndex};
pub use reference::{DisplaySettings, PointCloudReference, SourceCheck};
pub use section::{section_slice, SectionPoint, SectionSlice};
pub use source::{detect_kind, open_source, read_all, read_header, PointSource, SourceOptions};
pub use stats::{ClassHistogram, DensityGrid, DensitySummary, Histogram};
pub use surface::{decimate_to_tolerance, ground_surface, surface_operation, surface_points, thin_to_grid, tin_from_points, CellStat, DecimationResult, FitReport, GroundSurface, SurfaceSpec, ThinResult};
pub use text::{detect_text_format, Delimiter, TextDetection, TextFormat, TextLayout, TextRole};
