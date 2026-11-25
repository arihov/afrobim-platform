// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! At-grade intersections and roundabouts for AfroBIM: engine only. See the README for scope,
//! constructions and limits.
//!
//! * [`find_crossings`]: where two alignments cross or meet.
//! * [`build_intersection`]: from [`IntersectionInputs`] and the alignments, the edge alignments,
//!   kerb returns (simple, three-centred, chamfer), kerb lines, the secondary profile locked to the
//!   primary, return and edge profiles with low points, a `CorridorModel` description, findings and a
//!   [`DependencyRecord`].
//! * [`build_roundabout`]: circles, entry and exit kerbs, splitters and the reported figures.
//!
//! No engineering value is built in: every dimension is an input. Metres, radians, plan X east and
//! Y north; stations are internal stations; offsets positive to the right looking up-station;
//! cross-fall in percent positive rising away from the centreline.

mod corridor;
mod crossing;
mod edges;
mod error;
mod geom;
mod intersection;
mod kerb;
mod report;
mod roundabout;
mod vertical;

pub use corridor::{CorridorInputs, JunctionCorridor, TargetBinding};
pub use crossing::{find_crossings, Arms, Crossing, CrossingKind};
pub use edges::{build_edge, EdgeBuild, RoadEdges, Widening};
pub use error::{Finding, JunctionError, Severity};
pub use geom::{max_joint_gap, offset_alignment, polyline_between, slice_alignment};
pub use intersection::*;
pub use kerb::{build_return, Arm, KerbReturn, Quadrant, QuadrantFrame, ReturnKind, ReturnSpec};
pub use report::{Drawing, DrawingLayer, Table};
pub use roundabout::*;
pub use vertical::*;
