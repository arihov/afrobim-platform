//! COGO engine: points, point files, description keys, point groups, coordinate geometry,
//! traverses and parcels. No user interface and no rendering: display helpers return plain geometry
//! and label text. See the README for scope, methods and limits.
//!
//! Coordinates are metres, f64, stored exactly as given (easting, northing, elevation); bearings
//! are whole circle degrees clockwise from grid north.

pub mod angle;
pub mod calc;
pub mod deskey;
pub mod display;
pub mod error;
pub mod geom;
pub mod group;
pub mod parcel;
pub mod point;
pub mod pointfile;
pub mod split;
pub mod topology;
pub mod traverse;

mod arrangement;

pub use angle::{format_bearing, format_dms, format_quadrant, normalize_deg, parse_bearing, parse_dms};
pub use calc::{
    intersect_bearing_bearing, intersect_bearing_distance, intersect_distance_distance, inverse, offset_to_line, point_at_station, point_by_bearing_distance,
    point_by_turned_angle, point_on_line, polyline_length, station_offset, Inverse, LineHit, LineOffset, StationOffset, Turn,
};
pub use deskey::{Classification, DescriptionKey, DescriptionKeySet, KeyMatch};
pub use display::{point_display, PointDisplay};
pub use error::{CogoError, Result};
pub use geom::Xy;
pub use group::{PointGroup, PointGroupSet, Rule, ALL_POINTS};
pub use parcel::{ArcGeom, Boundary, DeedCall, DeedClosure, Location, Parcel, ParcelLabel, ParcelStatus, Segment, ACRE_M2, HECTARE_M2};
pub use point::{CogoPoint, DuplicatePair, LogEntry, PointStore, SourceRecord, TransformOp};
pub use pointfile::{detect_format, export_points, import_into, parse_points, unit_factor, ColumnOrder, Delimiter, DetectionReport, DuplicatePolicy, ExportOptions, PointFileFormat};
pub use split::{acquisition_schedule, area_table_csv, offset_polyline, schedule_csv, split_parcel, AcquisitionRow, Side, SplitOptions, SplitResult};
pub use topology::{analyse_site, TopologyOptions, TopologyReport};
pub use traverse::{AdjustMethod, AngleMode, Closure, Course, Traverse};
