//! The civil commands: one enum, the table the command list reads, and the icon of each.

use egui_phosphor::regular as ph;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cv {
    Toolspace,
    // Points
    Points,
    PointCreate,
    PointTable,
    PointGroups,
    DescKeys,
    PointExport,
    Cogo,
    Traverse,
    // Surfaces
    SurfaceNew,
    SurfaceDef,
    SurfaceStyle,
    SpotElev,
    FlowPath,
    Volumes,
    // Alignments and profiles
    GroundProfile,
    ProfileView,
    // Assemblies, corridors, sections
    Assembly,
    CorridorModel,
    SampleLines,
    SectionViews,
    // Quantities
    Earthworks,
    // Parcels
    ParcelPick,
    ParcelDeed,
    ParcelProps,
    ParcelTopology,
    ParcelSplit,
    ParcelSchedule,
    // Exchange and demo
    CivilLandXml,
    CivilDemo,
}


/// Every civil command (a test checks each one is in the command table).
pub const ALL: [Cv; 30] = [
    Cv::Toolspace,
    Cv::Points,
    Cv::PointCreate,
    Cv::PointTable,
    Cv::PointGroups,
    Cv::DescKeys,
    Cv::PointExport,
    Cv::Cogo,
    Cv::Traverse,
    Cv::SurfaceNew,
    Cv::SurfaceDef,
    Cv::SurfaceStyle,
    Cv::SpotElev,
    Cv::FlowPath,
    Cv::Volumes,
    Cv::GroundProfile,
    Cv::ProfileView,
    Cv::Assembly,
    Cv::CorridorModel,
    Cv::SampleLines,
    Cv::SectionViews,
    Cv::Earthworks,
    Cv::ParcelPick,
    Cv::ParcelDeed,
    Cv::ParcelProps,
    Cv::ParcelTopology,
    Cv::ParcelSplit,
    Cv::ParcelSchedule,
    Cv::CivilLandXml,
    Cv::CivilDemo,
];

pub fn glyph(c: Cv) -> &'static str {
    match c {
        Cv::Toolspace => ph::TREE_VIEW,
        Cv::Points => ph::DOTS_NINE,
        Cv::PointCreate => ph::MAP_PIN_PLUS,
        Cv::PointTable => ph::TABLE,
        Cv::PointGroups => ph::STACK,
        Cv::DescKeys => ph::KEY,
        Cv::PointExport => ph::EXPORT,
        Cv::Cogo => ph::RULER,
        Cv::Traverse => ph::PATH,
        Cv::SurfaceNew => ph::MOUNTAINS,
        Cv::SurfaceDef => ph::LIST_NUMBERS,
        Cv::SurfaceStyle => ph::CHART_LINE,
        Cv::SpotElev => ph::CROSSHAIR_SIMPLE,
        Cv::FlowPath => ph::DROP,
        Cv::Volumes => ph::CUBE_FOCUS,
        Cv::GroundProfile => ph::CHART_LINE_UP,
        Cv::ProfileView => ph::CHART_LINE,
        Cv::Assembly => ph::STACK_SIMPLE,
        Cv::CorridorModel => ph::ROAD_HORIZON,
        Cv::SampleLines => ph::LINE_SEGMENTS,
        Cv::SectionViews => ph::ROWS,
        Cv::Earthworks => ph::CHART_BAR,
        Cv::ParcelPick => ph::POLYGON,
        Cv::ParcelDeed => ph::COMPASS,
        Cv::ParcelProps => ph::IDENTIFICATION_CARD,
        Cv::ParcelTopology => ph::INTERSECT_SQUARE,
        Cv::ParcelSplit => ph::SCISSORS,
        Cv::ParcelSchedule => ph::TABLE,
        Cv::CivilLandXml => ph::FILE_CODE,
        Cv::CivilDemo => ph::FLASK,
    }
}
