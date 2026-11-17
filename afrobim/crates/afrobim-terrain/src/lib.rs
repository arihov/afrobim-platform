//! Terrain surface engine for AfroBIM: an ordered, rebuildable TIN surface definition, contours,
//! analysis and volumes. No user interface. See the README for scope, methods and limits.
#![forbid(unsafe_code)]

mod analysis;
mod build;
mod contour;
mod definition;
pub mod display;
mod error;
mod geom;
mod grid;
mod triangulate;
mod volume;

pub use definition::*;
pub use analysis::{areas, elevation_bands, extremes, flow_path, mean_elevation, regular_breaks, section_along, slope_arrows, slope_bands, spot_elevation, triangle_slopes, Areas, Band, Extremes, FlowPath, FlowStop, SlopeArrow, SlopeUnit, TriangleSlope};
pub use contour::{contours, smooth, Contour, ContourKind, ContourOptions, ContourSet};
pub use error::TerrainError;
pub use grid::{parse_esri_ascii, simplify_points, simplify_tin, tin_from_grid, GridImport, GridSpec, SimplifyReport};
pub use volume::{stage_storage, volume_against_level, volume_between, volume_between_grid, StagePoint, VolumeResult};
pub use triangulate::Crossing;

#[cfg(test)]
mod tests_build;
