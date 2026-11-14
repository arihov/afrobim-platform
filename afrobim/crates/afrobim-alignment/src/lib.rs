//! Road alignments: horizontal geometry, vertical profile, stored superelevation, station
//! equations, station/offset placement, LandXML import.
//!
//! - [`Alignment`]: tangents, circular arcs and clothoid spirals, with point-at-station,
//!   station-and-offset of a point, a polyline for drawing, construction from PIs or element by
//!   element. See the `geometry` module source for the formulae and how they are evaluated.
//! - [`Profile`]: the vertical profile (PVIs, grades, parabolic and circular vertical curves).
//! - [`Superelevation`]: stored cross-fall at stated stations. Data only; no rate is ever computed.
//! - [`StationEquations`]: internal (continuous) stations to displayed stations and back.
//! - [`parse_landxml`] / [`read_landxml`]: LandXML 1.2 alignments with their profile, station
//!   equations and superelevation.
//! - [`resolve`], [`location_report`], [`place`]: where a sign stands beside an alignment (the
//!   side, offset, facing and rotation rules of the C# reference, and their combination with the
//!   geometry). [`place_with`] takes displayed stations and the cross-fall option.
//! - [`AlignmentModule`]: the alignment as a kernel element, drawn in plan.
//!
//! Conventions: metres and radians; plan X east, Y north; headings counter-clockwise from +X in
//! the direction of increasing station; offsets positive to the right looking up-station;
//! curvature positive turning left. Stations are internal (continuous) stations unless a function
//! says displayed; cross-fall is in percent, positive rising away from the centreline. Positions handed to the kernel are in millimetres and
//! rotations in degrees. The crate's README has the full statement.

mod geometry;
mod landxml;
mod module;
mod placement;
mod profile;
mod stationing;
mod superelevation;

pub use geometry::{
    clothoid_xy, format_station, wrap_pi, Alignment, AlignmentBuilder, AlignmentElement, AlignmentError, ElementKind, Joint, Pi, PiCurve,
    StationPoint, Turn,
};
pub use landxml::{parse_landxml, read_landxml, LandXmlError, LandXmlImport};
pub use module::{plan_drawing, AlignmentModule, PlanDrawing, CATEGORY};
pub use placement::{
    location_report, normalize_rad, place, place_with, resolve, rotation, CrossFallAdjust, PlaceOptions, PlacedPosition, PlacedSign, Placement,
    PlacementError, PlacementPlan, PlacementPoint, SignLocation, SignSide, StationReference, TrafficSide,
};
pub use profile::{Profile, ProfileError, ProfilePoint, Pvi, VerticalCurve, VerticalCurveExtent};
pub use stationing::{DisplayedStation, StationCandidate, StationDirection, StationEquation, StationEquationError, StationEquations};
pub use superelevation::{CriticalStation, CrossFall, CrossFallPoint, Superelevation, SuperelevationError, SuperelevationSection};
