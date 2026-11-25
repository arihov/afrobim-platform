// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Sign visibility along an alignment (work package A2, analysis stage).
//!
//! For a sign with a site placement, a driver's eye is swept along the approach and sight lines
//! are traced to the sign face. The result is the continuous clear visibility distance, what
//! obstructs the view and at which stations, the obliquity of the line of sight, and a list of
//! check lines against the provisions of the UK Traffic Signs Manual chapters and the Uganda
//! Traffic Signs Manual held in the repository, each with its citation.
//!
//! - [`sweep`]: the geometry (faces, approach, sight lines, obstruction tests, the sweep).
//! - [`scene`]: the triangles a sight line can hit (ground TIN, corridor surfaces, other solids).
//! - [`sources`]: every value taken from a held document, with its citation.
//! - [`checks`]: the check lines (`OK`, `FAILS`, `DIFFERS`, `INPUT NEEDED`, `INFORMATION`).
//! - [`inputs`]: the entries kept on a sign under the property key `check.visibility`.
//! - [`analysis`]: one sign or a whole document: facts from elements, the sweep, the checks, the
//!   road-wide run and its CSV.
//! - [`drawings`]: a plan of the visibility envelope and a profile of a sight line, with labels.
//!
//! The README states the method, what was found in the sources and what was not, and the limits.
//!
//! ```
//! use afrobim_alignment::AlignmentBuilder;
//! use afrobim_visibility::sweep::{Approach, FaceGeometry, SweepParams, Sweeper, TravelDirection};
//!
//! // A straight road, a sign 5 m left of the centreline at station 500, a driver in the left lane.
//! let alignment = AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap();
//! let approach = Approach { alignment, sign_station_m: 500.0, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
//! let face = FaceGeometry { origin: [500.0, 5.0, 1.8], rotation_deg: -90.0, width_m: 1.0, height_m: 1.0 };
//! let result = Sweeper::new(face, &approach, SweepParams::default()).run();
//! assert!(result.full.reaches_start); // nothing in the way: visible from at least 400 m
//! ```

pub mod analysis;
pub mod checks;
pub mod drawings;
pub mod inputs;
pub mod scene;
pub mod sources;
pub mod sweep;

pub use analysis::{
    alignments_of, analyse_sign, analyse_sign_in, apply_spacing, derived_x_height, run_all, scene_from_document, RoadRun, RunOptions, SceneOptions,
    SignAnalysis, SignContext, DETAIL_CSV_HEADER, SIGN_CATEGORY, SUMMARY_CSV_HEADER,
};
pub use checks::{classify, run_checks, CheckInput, CheckLine, SignGroup, Status};
pub use drawings::{plan_envelope, sight_line_profile, DrawingKind, DrawingLabel, LabelKind, VisibilityDrawing};
pub use inputs::{is_hidden_key, load, save, EdgeOffsets, EyeOffset, RoadEdges, Standards, UkSignRule, VisibilityInputs, KEY};
pub use scene::{ObstacleKind, ObstacleScene};
pub use sources::{Citation, SpeedBandMph, UgSpeedClass};
pub use sweep::{
    local_alignment, Approach, Blocker, EyeSample, FaceGeometry, LateralObstruction, ObstructionSide, SweepParams, SweepResult, Sweeper, TravelDirection,
};
