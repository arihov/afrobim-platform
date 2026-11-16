//! Road corridor: a typical cross-section carried along an alignment, cut and filled against an
//! existing ground surface, with cross-sections and earthwork volumes.
//!
//! This crate is geometry and measurement. It holds **no design values**: every width, slope,
//! depth, interval and factor is entered by the engineer.
//!
//! - [`Tin`]: the ground as points and triangles, with elevation at a point, a section cut along
//!   a line, and a compact JSON encoding ([`TinEncoded`]).
//! - [`parse_landxml_surfaces`] / [`read_landxml_surfaces`]: LandXML 1.2 TIN surfaces.
//! - [`read_survey_points`] and [`build_survey_surface`]: a surface from a file of survey points
//!   (point, northing, easting, level, description), triangulated by Delaunay's rule.
//! - [`CoordinateSystem`]: the projected system a surface's coordinates are in (WGS 84 / UTM).
//! - [`Template`], [`Link`], [`Slope`], [`Daylight`], [`TemplateRegion`], [`CorridorDefinition`]:
//!   what the engineer enters.
//! - [`Corridor::build`] and [`section_at`]: the [`Section`]s, each with its finished surface,
//!   earthworks line, ground, daylight points and cut and fill areas.
//! - [`Assembly`], [`Subassembly`], [`library`]: named parametric parts with codes and BIM
//!   classification; [`CorridorModel`], [`build_model`]: baselines, regions, frequencies;
//!   [`corridor_surface`], [`feature_lines`]; [`SampleLineGroup`], [`section_views`];
//!   [`earthwork_table`] (average end area or prismoidal), [`material_volumes`], [`mass_haul`].
//! - [`earthworks`] and [`earthworks_csv`]: volumes by average end area and the mass ordinate.
//! - [`section_drawing`], [`corridor_meshes`], [`plan_lines`]: drawings and 3D surfaces.
//! - [`SurfaceModule`], [`CorridorModule`], [`add_surface`], [`add_corridor`] and friends: the
//!   kernel elements.
//!
//! Conventions: metres; plan X east, Y north; stations are internal (continuous) stations unless
//! a label is asked for; **offsets positive to the right looking up-station**; **slopes positive
//! rising away from the centreline**; **cut is ground above the earthworks line, fill is ground
//! below it**, both reported as positive areas and volumes; the mass ordinate is cut minus fill.
//! Drawings and meshes handed to the kernel are in millimetres. The crate's README has the
//! formulae and the limits.

mod assembly;
mod crs;
mod landxml;
mod model;
mod module;
mod output;
mod quantities;
mod samples;
mod section;
mod survey;
mod template;
mod tin;
mod volumes;

pub use assembly::{
    ifc, library, polyline_from_alignment, Assembly, Bench, Block, Classification, DaylightRule, DepressedMedian, DesignGeometry, DesignLink, DesignPoint, DesignShape, Dim, Ditch,
    DitchShape, ElevationTarget, EvalContext, GenericLink, Layer, LinkMode, OffsetTarget, Side, Strip, SubKind, Subassembly, Wall,
};
pub use crs::CoordinateSystem;
<<<<<<< HEAD
pub use landxml::{parse_landxml_surfaces, parse_landxml_surfaces_with, read_landxml_surfaces, ImportedSurface, SurfaceImport, SurfaceReadOptions, SurfaceXmlError};
=======
pub use model::{
    baseline_earthworks, build_baseline, build_model, corridor_surface, daylight_boundary, design_section_at, feature_lines, AssemblyRegion, Baseline, BaselineResult, CorridorModel,
    FeatureLine, Frequency, ModelCorridor, ModelSection, SurfaceBoundary,
};
pub use quantities::{
    earthwork_table, mass_haul, material_volumes, quantity_csv, EndArea, MassHaul, MaterialRow, MaterialStation, MaterialTable, MaterialTotal, QuantityOptions, QuantityRow, QuantityTable,
    VolumeMethod, QUANTITY_CSV_HEADER,
};
pub use samples::{section_views, surface_earthworks, views_material_volumes, CorridorSampling, DesignView, SampleLine, SampleLineGroup, SampleStations, SectionView, SurfaceLine};
pub use landxml::{parse_landxml_surfaces, read_landxml_surfaces, ImportedSurface, SurfaceImport, SurfaceXmlError};
>>>>>>> worktree-agent-acf0dea74d3942e8f
pub use module::{
    add_corridor, add_surface, add_surface_in, build_corridor, corridor_element, corridor_of, import_landxml_surfaces, import_survey_surface,
    refresh_corridor, set_corridor_definition, surface_crs, surface_display_mesh, surface_of, surfaces, CorridorElement, CorridorModule, CorridorSummary,
    SurfaceDefinition, SurfaceModule, SurveyImportRequest, CORRIDORS_LAYER, CORRIDOR_CATEGORY, DISPLAY_TRIANGLE_CAP, SURFACES_LAYER, SURFACE_CATEGORY,
};
pub use output::{corridor_meshes, plan_lines, section_drawing, CorridorMeshes, LabelKind, PlanLines, SectionDrawing, SectionLabel};
pub use section::{cut_fill_areas, polygon_area, section_at, Corridor, CorridorError, CutFill, DaylightPoint, Section, SectionPoint, SlopeKind};
pub use survey::{
    build_survey_surface, parse_survey_points, read_survey_points, ColumnOrder, SurfaceOptions, SurveyError, SurveyFormat, SurveyPoint, SurveyPoints, SurveySurface,
};
pub use template::{CorridorDefinition, Daylight, Link, Slope, Template, TemplateRegion, TemplateSide, VolumeOptions};
pub use tin::{Tin, TinBounds, TinEncoded, TinError, TIN_ENCODING};
pub use volumes::{earthworks, earthworks_csv, Earthworks, VolumeRow, EARTHWORKS_CSV_HEADER};
