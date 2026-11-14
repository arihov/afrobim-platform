//! The geometry of the driver's view: a driver's eye is moved along the approach to a sign, and
//! from each position sight lines are traced to points of the sign face.
//!
//! # Conventions
//!
//! Site metres, X east, Y north, Z up. A sign face is a rectangle: its lower-left corner is the
//! site origin of the element lifted by the mounting height, its width runs along local +X, which
//! for a rotation `r` is the direction `(cos r, sin r)` (across the road to the driver's right),
//! and it looks towards local −Y, the direction `(sin r, −cos r)`, the way `afrobim-app`
//! draws it (`view3d::frame_in`). Traffic served by the sign travels on heading `r + 90`
//! degrees.
//!
//! The eye is on the approach at a signed lateral offset from the alignment centreline (positive
//! to the right looking up-station) and `eye_height` above the road level of the profile at its
//! station. The **distance** of an eye position is the distance along the alignment from the eye
//! to the station of the face centre, not the length of the sight line.
//!
//! # What blocks a sight line
//!
//! 1. Triangles of the [`ObstacleScene`]: ground, corridor surfaces, other signs, posts, gantries,
//!    lighting columns, barriers and any other modelled solid. The sign's own supports are
//!    skipped by listing them in `ignore`.
//! 2. The road's own vertical profile: the line is blocked where it runs below the centreline
//!    level of the profile at the station of the point under it (a crest). Sampled every
//!    `sample_m` along the line. Cross-fall is not applied; this is stated as a limit in the README.
//! 3. User-entered lateral obstructions: a wall or bank beside the road beginning at a given
//!    offset from the centreline (optionally of limited height and station range). A sight line
//!    is blocked where it passes beyond that offset. This is how the inside of a horizontal curve
//!    is handled when no solid is modelled there.
//! 4. The back of the face: an eye that is not in front of the face plane cannot see it.
//!
//! A sight line that only touches an obstruction, or ends on it, is not blocked.

use afrobim_alignment::Alignment;
use afrobim_coordination::geom::V3;
use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::scene::{ObstacleKind, ObstacleScene};

/// A sign face in site metres. See the module documentation for the frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FaceGeometry {
    /// Lower-left corner of the face, m (z is the lower edge, i.e. site level + mounting height).
    pub origin: V3,
    /// Rotation of the element about +Z, degrees counter-clockwise from +X.
    pub rotation_deg: f64,
    pub width_m: f64,
    pub height_m: f64,
}

impl FaceGeometry {
    /// Direction of the face's width, unit vector in plan.
    pub fn u_dir(&self) -> V3 {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        [c, s, 0.0]
    }

    /// The face normal, pointing at the approaching traffic: `(sin r, -cos r, 0)`.
    pub fn normal(&self) -> V3 {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        [s, -c, 0.0]
    }

    /// The point `u` along the width and `v` up the height (metres from the lower-left corner).
    pub fn point(&self, u: f64, v: f64) -> V3 {
        let d = self.u_dir();
        [self.origin[0] + u * d[0], self.origin[1] + u * d[1], self.origin[2] + v]
    }

    pub fn centre(&self) -> V3 {
        self.point(self.width_m / 2.0, self.height_m / 2.0)
    }

    pub fn corners(&self) -> [V3; 4] {
        [self.point(0.0, 0.0), self.point(self.width_m, 0.0), self.point(self.width_m, self.height_m), self.point(0.0, self.height_m)]
    }

    /// Points of the face to trace sight lines to: `grid` = 1 gives the centre; 2 the four
    /// corners and the centre; 3 or more an even `grid` x `grid` lattice (3 holds the corners,
    /// edge midpoints and centre).
    pub fn sample_points(&self, grid: usize) -> Vec<V3> {
        match grid {
            0 | 1 => vec![self.centre()],
            2 => {
                let mut v = self.corners().to_vec();
                v.push(self.centre());
                v
            }
            n => {
                let mut v = Vec::with_capacity(n * n);
                for j in 0..n {
                    for i in 0..n {
                        v.push(self.point(self.width_m * i as f64 / (n - 1) as f64, self.height_m * j as f64 / (n - 1) as f64));
                    }
                }
                v
            }
        }
    }

    /// Heading of the traffic the face serves, degrees counter-clockwise from +X.
    pub fn served_heading_deg(&self) -> f64 {
        self.rotation_deg + 90.0
    }
}

/// Which way the approaching driver travels along the alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TravelDirection {
    /// Towards increasing station: the driver approaches from lower stations.
    UpStation,
    /// Towards decreasing station.
    DownStation,
}

impl TravelDirection {
    /// +1 for up-station travel, −1 for down-station.
    pub fn sign(self) -> f64 {
        match self {
            TravelDirection::UpStation => 1.0,
            TravelDirection::DownStation => -1.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            TravelDirection::UpStation => "up-station",
            TravelDirection::DownStation => "down-station",
        }
    }
}

/// Which side of the centreline a lateral obstruction is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObstructionSide {
    Left,
    Right,
}

/// A wall or bank beside the road, entered by the user: everything beyond `offset_m` from the
/// centreline on `side` blocks the view (up to `top_above_road_m` when given), for the stations
/// in `station_range` when given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LateralObstruction {
    pub side: ObstructionSide,
    /// Distance from the centreline to the near face of the obstruction, m (positive).
    pub offset_m: f64,
    /// Height of the top above the road level, m; `None` is unlimited.
    #[serde(default)]
    pub top_above_road_m: Option<f64>,
    /// Internal stations between which it stands; `None` is the whole alignment.
    #[serde(default)]
    pub station_range: Option<(f64, f64)>,
    #[serde(default)]
    pub label: String,
}

impl LateralObstruction {
    pub fn new(side: ObstructionSide, offset_m: f64, label: &str) -> Self {
        LateralObstruction { side, offset_m, top_above_road_m: None, station_range: None, label: label.to_string() }
    }
}

/// Settings of a sweep. These are analysis settings, not engineering values: nothing here comes
/// from a standard.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SweepParams {
    /// The farthest eye position, as a distance before the sign, m.
    pub start_distance_m: f64,
    /// Spacing of eye positions, m.
    pub step_m: f64,
    /// Spacing of the samples along a sight line for the road profile and lateral tests, m.
    pub sample_m: f64,
    /// The limit distance is refined by bisection to this tolerance, m.
    pub tolerance_m: f64,
    /// Height of the driver's eye above the road, m.
    pub eye_height_m: f64,
    /// Sight lines go to a `grid` x `grid` lattice of the face (see [`FaceGeometry::sample_points`]).
    pub grid: usize,
}

impl Default for SweepParams {
    fn default() -> Self {
        SweepParams { start_distance_m: 400.0, step_m: 5.0, sample_m: 1.0, tolerance_m: 0.05, eye_height_m: crate::sources::EYE_HEIGHT_M, grid: 3 }
    }
}

/// The approach: which alignment, where the face is along it, and the driver's path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approach {
    /// The alignment (a stretch of it is enough: see [`local_alignment`]).
    pub alignment: Alignment,
    /// Internal station of the face centre.
    pub sign_station_m: f64,
    pub direction: TravelDirection,
    /// Lateral offset of the driver's path from the centreline, m, positive to the right looking
    /// up-station.
    pub eye_offset_m: f64,
    /// Road level used where the alignment has no profile (or beyond it), m.
    pub fallback_road_z_m: f64,
}

impl Approach {
    /// Road (centreline) level at a station, m: the profile's, held at its end levels beyond its
    /// range; the fallback level where there is no profile.
    pub fn road_level(&self, station_m: f64) -> f64 {
        match &self.alignment.profile {
            Some(p) => match p.range() {
                Some((lo, hi)) => p.elevation_at(station_m.clamp(lo, hi)).unwrap_or(self.fallback_road_z_m),
                None => self.fallback_road_z_m,
            },
            None => self.fallback_road_z_m,
        }
    }

    /// Station of the eye at a distance before the sign.
    pub fn eye_station(&self, distance_m: f64) -> f64 {
        self.sign_station_m - self.direction.sign() * distance_m
    }
}

/// A stretch of an alignment as an alignment of its own: the elements that overlap
/// `[from_m, to_m]`, with the profile, so that station and offset queries only search the
/// elements nearby. Elements keep their own coordinates, so stations are unchanged.
pub fn local_alignment(a: &Alignment, from_m: f64, to_m: f64) -> Alignment {
    let starts = a.element_stations();
    let (lo, hi) = (from_m.min(to_m), from_m.max(to_m));
    let keep: Vec<usize> = (0..a.elements.len()).filter(|&i| starts[i] + a.elements[i].length >= lo && starts[i] <= hi).collect();
    let Some(&first) = keep.first() else { return a.clone() };
    let elements = keep.iter().map(|&i| a.elements[i]).collect();
    match Alignment::from_elements(&a.name, starts[first], elements) {
        Ok(mut local) => {
            local.profile = a.profile.clone();
            local
        }
        Err(_) => a.clone(),
    }
}

/// One thing that blocked a sight line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blocker {
    pub kind: ObstacleKind,
    pub label: String,
    pub element: Option<ElementId>,
    /// Where the line was stopped, site metres.
    pub point: V3,
    /// Station of the alignment nearest that point, if it has one.
    pub station_m: Option<f64>,
    /// Distance from the eye to the point along the line, m.
    pub along_line_m: f64,
}

/// What an eye position sees of the face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleState {
    /// Every traced point of the face is visible.
    Full,
    /// Some are.
    Partial,
    Blocked,
}

/// One eye position of the sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EyeSample {
    /// Distance along the alignment before the sign, m.
    pub distance_m: f64,
    pub station_m: f64,
    pub eye: V3,
    pub visible_points: usize,
    pub total_points: usize,
    pub state: SampleState,
    /// Distinct blockers of the traced lines, most frequent first.
    pub blockers: Vec<Blocker>,
    /// Angle between the line of sight to the face centre and the face normal, degrees (3D).
    pub obliquity_deg: f64,
    /// The same angle in plan (the horizontal part of the line of sight and of the normal).
    pub plan_obliquity_deg: f64,
    /// Angle in plan between the direction of travel and the line to the face centre, degrees:
    /// the "observation angle" of the 10 degree reading rule.
    pub observation_angle_deg: f64,
}

/// One traced line, for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct LineTrace {
    pub target: V3,
    /// Where the line ends: the blocker's point, or the target when clear.
    pub end: V3,
    pub blocker: Option<Blocker>,
}

/// Where continuous visibility ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitInfo {
    /// The continuous clear visibility distance, m: the farthest eye distance from which the sign
    /// stays visible (fully, or partly, as the limit was asked) at every closer position.
    pub distance_m: f64,
    /// True when the sign is visible all the way to the start of the sweep: the distance is then
    /// a lower bound.
    pub reaches_start: bool,
    /// The eye position at the limit (the last position that sees it).
    pub at_limit: Option<EyeSample>,
    /// The first position beyond the limit, with what blocks it.
    pub beyond: Option<EyeSample>,
    /// Intervals beyond the limit, as (from, to) eye distances in m between sweep positions, from
    /// which the sign is visible again.
    pub visible_again: Vec<(f64, f64)>,
}

/// An obstruction summarised over the whole sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockerSummary {
    pub kind: ObstacleKind,
    pub label: String,
    pub element: Option<ElementId>,
    /// Eye distances (before the sign) between which this blocks a line, m.
    pub from_distance_m: f64,
    pub to_distance_m: f64,
    /// Stations at which it was hit, m (smallest and largest).
    pub station_range_m: Option<(f64, f64)>,
    pub eye_positions: usize,
}

/// The result of sweeping the eye along the approach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepResult {
    pub params: SweepParams,
    pub face: FaceGeometry,
    pub direction: TravelDirection,
    pub eye_offset_m: f64,
    pub sign_station_m: f64,
    pub samples: Vec<EyeSample>,
    /// The farthest distance actually swept (the alignment may end before `start_distance_m`).
    pub swept_to_m: f64,
    pub truncated: Option<String>,
    /// Continuous visibility of the whole face.
    pub full: LimitInfo,
    /// Continuous visibility of at least part of the face.
    pub partial: LimitInfo,
    pub blockers: Vec<BlockerSummary>,
}

impl SweepResult {
    /// Blockers that are other signs (the sign is masked by them).
    pub fn masking_signs(&self) -> Vec<&BlockerSummary> {
        self.blockers.iter().filter(|b| b.kind == ObstacleKind::Sign).collect()
    }
}

/// Distance of the sight line stopped short of the target so a line ending on the face is not
/// blocked by the face's own plane or by a slab standing behind it, m.
const END_MARGIN_M: f64 = 0.002;
/// Tolerance of the road profile test: the line must be below the road by more than this, m.
const ROAD_EPS_M: f64 = 1e-6;

/// The sweep engine. Holds what the tracing needs; build it with [`Sweeper::new`].
pub struct Sweeper<'a> {
    pub face: FaceGeometry,
    pub approach: &'a Approach,
    pub params: SweepParams,
    pub scene: Option<&'a ObstacleScene>,
    /// Elements that never block: the sign's own posts and foundations (and the sign).
    pub ignore: &'a [ElementId],
    pub lateral: &'a [LateralObstruction],
    /// Whether to test the road's vertical profile (a no-op without a profile).
    pub test_profile: bool,
}

impl<'a> Sweeper<'a> {
    pub fn new(face: FaceGeometry, approach: &'a Approach, params: SweepParams) -> Sweeper<'a> {
        Sweeper { face, approach, params, scene: None, ignore: &[], lateral: &[], test_profile: true }
    }

    pub fn with_scene(mut self, scene: &'a ObstacleScene, ignore: &'a [ElementId]) -> Self {
        self.scene = Some(scene);
        self.ignore = ignore;
        self
    }

    pub fn with_lateral(mut self, lateral: &'a [LateralObstruction]) -> Self {
        self.lateral = lateral;
        self
    }

    fn eye_at(&self, d: f64) -> Option<(f64, V3)> {
        let a = &self.approach.alignment;
        let s = self.approach.eye_station(d);
        let (x, y) = a.point_at_offset(s, self.approach.eye_offset_m)?;
        Some((s, [x, y, self.approach.road_level(s) + self.params.eye_height_m]))
    }

    fn station_of(&self, p: V3) -> Option<(f64, f64)> {
        self.approach.alignment.station_offset(p[0], p[1])
    }

    /// Traces one line from `eye` to `target`. Returns the nearest blocker.
    pub fn trace(&self, eye: V3, target: V3) -> Option<Blocker> {
        let dir = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len <= END_MARGIN_M * 2.0 {
            return None;
        }
        let k = (len - END_MARGIN_M) / len;
        let end = [eye[0] + dir[0] * k, eye[1] + dir[1] * k, eye[2] + dir[2] * k];
        let seg_len = len * k;

        let mut best: Option<Blocker> = None;
        let mut limit_t = 1.0;
        if let Some(scene) = self.scene {
            if let Some(h) = scene.first_hit(eye, end, self.ignore) {
                let g = scene.group(h.group);
                limit_t = h.t;
                best = Some(Blocker {
                    kind: g.kind,
                    label: g.label.clone(),
                    element: g.element,
                    point: h.point,
                    station_m: self.station_of(h.point).map(|so| so.0),
                    along_line_m: h.t * seg_len,
                });
            }
        }
        let wants_profile = self.test_profile && self.approach.alignment.profile.is_some();
        if wants_profile || !self.lateral.is_empty() {
            let n = ((seg_len / self.params.sample_m.max(0.01)).ceil() as usize).max(2);
            for i in 1..n {
                let t = i as f64 / n as f64;
                if t >= limit_t {
                    break;
                }
                let p = [eye[0] + (end[0] - eye[0]) * t, eye[1] + (end[1] - eye[1]) * t, eye[2] + (end[2] - eye[2]) * t];
                let Some((station, offset)) = self.station_of(p) else { continue };
                let level = self.approach.road_level(station);
                if wants_profile && p[2] < level - ROAD_EPS_M {
                    best = Some(Blocker {
                        kind: ObstacleKind::RoadProfile,
                        label: "road profile".to_string(),
                        element: None,
                        point: p,
                        station_m: Some(station),
                        along_line_m: t * seg_len,
                    });
                    break;
                }
                let hit = self.lateral.iter().find(|o| {
                    let beyond = match o.side {
                        ObstructionSide::Right => offset >= o.offset_m,
                        ObstructionSide::Left => offset <= -o.offset_m,
                    };
                    let in_range = o.station_range.is_none_or(|(a, b)| station >= a.min(b) && station <= a.max(b));
                    let low_enough = o.top_above_road_m.is_none_or(|top| p[2] < level + top);
                    beyond && in_range && low_enough
                });
                if let Some(o) = hit {
                    best = Some(Blocker {
                        kind: ObstacleKind::LateralOffset,
                        label: if o.label.is_empty() { "lateral obstruction".to_string() } else { o.label.clone() },
                        element: None,
                        point: p,
                        station_m: Some(station),
                        along_line_m: t * seg_len,
                    });
                    break;
                }
            }
        }
        best
    }

    /// The traced lines from the eye at distance `d`, one per sample point of the face, for
    /// drawing. `None` when the eye falls off the alignment.
    pub fn lines_at(&self, d: f64) -> Option<Vec<LineTrace>> {
        let (_, eye) = self.eye_at(d)?;
        Some(
            self.face
                .sample_points(self.params.grid)
                .into_iter()
                .map(|target| {
                    let blocker = self.trace(eye, target);
                    LineTrace { target, end: blocker.as_ref().map_or(target, |b| b.point), blocker }
                })
                .collect(),
        )
    }

    /// What the eye at distance `d` before the sign sees. `None` when the eye falls off the
    /// alignment.
    pub fn sample_at(&self, d: f64) -> Option<EyeSample> {
        let (station, eye) = self.eye_at(d)?;
        let centre = self.face.centre();
        let n = self.face.normal();
        let to_eye = [eye[0] - centre[0], eye[1] - centre[1], eye[2] - centre[2]];
        let to_eye_len = (to_eye[0] * to_eye[0] + to_eye[1] * to_eye[1] + to_eye[2] * to_eye[2]).sqrt();
        let front = to_eye[0] * n[0] + to_eye[1] * n[1] + to_eye[2] * n[2];
        let obliquity_deg = if to_eye_len > 0.0 { (front / to_eye_len).clamp(-1.0, 1.0).acos().to_degrees() } else { 0.0 };
        let plan_len = (to_eye[0] * to_eye[0] + to_eye[1] * to_eye[1]).sqrt();
        let plan_obliquity_deg = if plan_len > 0.0 {
            ((to_eye[0] * n[0] + to_eye[1] * n[1]) / plan_len).clamp(-1.0, 1.0).acos().to_degrees()
        } else {
            0.0
        };
        // Observation angle: between the travel direction at the eye and the plan line to the face.
        let heading = self.approach.alignment.point_at(station).map_or(0.0, |p| p.heading_rad)
            + if self.approach.direction == TravelDirection::DownStation { std::f64::consts::PI } else { 0.0 };
        let (hx, hy) = (heading.cos(), heading.sin());
        let (vx, vy) = (centre[0] - eye[0], centre[1] - eye[1]);
        let observation_angle_deg = (hx * vy - hy * vx).atan2(hx * vx + hy * vy).abs().to_degrees();

        let points = self.face.sample_points(self.params.grid);
        let total = points.len();
        let (visible, blockers) = if front <= 1e-9 {
            let b = Blocker {
                kind: ObstacleKind::BackOfSign,
                label: "back of the sign face".to_string(),
                element: None,
                point: centre,
                station_m: self.station_of(centre).map(|so| so.0),
                along_line_m: to_eye_len,
            };
            (0, vec![b])
        } else {
            let mut visible = 0;
            let mut found: Vec<(Blocker, usize)> = Vec::new();
            for target in points {
                match self.trace(eye, target) {
                    None => visible += 1,
                    Some(b) => match found.iter_mut().find(|(f, _)| f.kind == b.kind && f.label == b.label && f.element == b.element) {
                        Some((_, count)) => *count += 1,
                        None => found.push((b, 1)),
                    },
                }
            }
            found.sort_by_key(|f| std::cmp::Reverse(f.1));
            (visible, found.into_iter().map(|(b, _)| b).collect())
        };
        let state = if visible == total {
            SampleState::Full
        } else if visible == 0 {
            SampleState::Blocked
        } else {
            SampleState::Partial
        };
        Some(EyeSample {
            distance_m: d,
            station_m: station,
            eye,
            visible_points: visible,
            total_points: total,
            state,
            blockers,
            obliquity_deg,
            plan_obliquity_deg,
            observation_angle_deg,
        })
    }

    fn limit(&self, samples: &[EyeSample], ok: fn(&EyeSample) -> bool) -> LimitInfo {
        let Some(first_bad) = samples.iter().position(|s| !ok(s)) else {
            let last = samples.last();
            return LimitInfo {
                distance_m: last.map_or(0.0, |s| s.distance_m),
                reaches_start: !samples.is_empty(),
                at_limit: last.cloned(),
                beyond: None,
                visible_again: Vec::new(),
            };
        };
        let mut lo = if first_bad == 0 { 0.0 } else { samples[first_bad - 1].distance_m };
        let mut hi = samples[first_bad].distance_m;
        let mut beyond = samples[first_bad].clone();
        for _ in 0..60 {
            if hi - lo <= self.params.tolerance_m {
                break;
            }
            let mid = (lo + hi) / 2.0;
            match self.sample_at(mid) {
                Some(s) if ok(&s) => lo = mid,
                Some(s) => {
                    hi = mid;
                    beyond = s;
                }
                None => break,
            }
        }
        let mut visible_again = Vec::new();
        let mut run: Option<(f64, f64)> = None;
        for s in &samples[first_bad..] {
            if ok(s) {
                run = Some(match run {
                    Some((a, _)) => (a, s.distance_m),
                    None => (s.distance_m, s.distance_m),
                });
            } else if let Some(r) = run.take() {
                visible_again.push(r);
            }
        }
        if let Some(r) = run {
            visible_again.push(r);
        }
        LimitInfo { distance_m: lo, reaches_start: false, at_limit: self.sample_at(lo), beyond: Some(beyond), visible_again }
    }

    /// Sweeps the eye from one step before the sign out to `start_distance_m`.
    pub fn run(&self) -> SweepResult {
        let p = self.params;
        let step = p.step_m.max(1e-3);
        let n = (p.start_distance_m / step).floor() as usize;
        let mut distances: Vec<f64> = (1..=n).map(|i| i as f64 * step).collect();
        if distances.last().is_none_or(|&l| p.start_distance_m - l > 1e-9) && p.start_distance_m > 0.0 {
            distances.push(p.start_distance_m);
        }
        let mut samples = Vec::with_capacity(distances.len());
        let mut truncated = None;
        for d in distances {
            match self.sample_at(d) {
                Some(s) => samples.push(s),
                None => {
                    truncated = Some(format!("the approach runs off the alignment {d:.1} m before the sign; the sweep stops at {:.1} m", samples.last().map_or(0.0, |s| s.distance_m)));
                    break;
                }
            }
        }
        let swept_to_m = samples.last().map_or(0.0, |s| s.distance_m);
        let full = self.limit(&samples, |s| s.state == SampleState::Full);
        let partial = self.limit(&samples, |s| s.state != SampleState::Blocked);
        let blockers = summarise_blockers(&samples);
        SweepResult {
            params: p,
            face: self.face,
            direction: self.approach.direction,
            eye_offset_m: self.approach.eye_offset_m,
            sign_station_m: self.approach.sign_station_m,
            samples,
            swept_to_m,
            truncated,
            full,
            partial,
            blockers,
        }
    }
}

fn summarise_blockers(samples: &[EyeSample]) -> Vec<BlockerSummary> {
    let mut out: Vec<BlockerSummary> = Vec::new();
    for s in samples {
        for b in &s.blockers {
            match out.iter_mut().find(|o| o.kind == b.kind && o.label == b.label && o.element == b.element) {
                Some(o) => {
                    o.from_distance_m = o.from_distance_m.min(s.distance_m);
                    o.to_distance_m = o.to_distance_m.max(s.distance_m);
                    o.eye_positions += 1;
                    if let Some(st) = b.station_m {
                        o.station_range_m = Some(match o.station_range_m {
                            Some((a, c)) => (a.min(st), c.max(st)),
                            None => (st, st),
                        });
                    }
                }
                None => out.push(BlockerSummary {
                    kind: b.kind,
                    label: b.label.clone(),
                    element: b.element,
                    from_distance_m: s.distance_m,
                    to_distance_m: s.distance_m,
                    station_range_m: b.station_m.map(|st| (st, st)),
                    eye_positions: 1,
                }),
            }
        }
    }
    out.sort_by(|a, b| a.from_distance_m.partial_cmp(&b.from_distance_m).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::cuboid_m;
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi, Turn};

    /// A straight alignment along +X from station 0, `len` m long.
    fn straight(len: f64) -> Alignment {
        AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(len).build().unwrap()
    }

    /// A face of (almost) a point at (x, y, z), serving traffic that travels +X.
    fn point_face(x: f64, y: f64, z: f64) -> FaceGeometry {
        FaceGeometry { origin: [x, y, z], rotation_deg: -90.0, width_m: 0.0, height_m: 0.0 }
    }

    fn params() -> SweepParams {
        SweepParams { start_distance_m: 300.0, step_m: 2.0, sample_m: 1.0, tolerance_m: 0.005, eye_height_m: 1.05, grid: 1 }
    }

    /// Straight level road, a wall between the driver and the sign: similar triangles.
    ///
    /// Alignment along +X; traffic keeps left, so the driver (travelling +X) is at offset -1.75,
    /// i.e. y = +1.75 (offset is positive to the right, right is -Y when heading +X). The sign
    /// point is at y = 5.0 (offset -5.0), x = 500, z = 2.0. The eye is 1.05 m above the road
    /// (road level 0, no profile).
    ///
    /// A wall (a thin slab, 1 mm) stands at x = 440, spanning y = 2.5 to 3.5 and tall enough not to be
    /// stepped over. In plan the sight line from the eye at distance D (x = 500 - D) to the sign
    /// leaves the lane (y = 1.75) and rises 3.25 m in y over D metres. At the wall (60 m before the
    /// sign) it has covered (D - 60) / D of that, so its y there is
    ///     y(440) = 1.75 + 3.25 (D - 60) / D.
    /// It clears the wall's near edge (y = 2.5) when 3.25 (D - 60) / D = 0.75, i.e.
    ///     D = 60 x 3.25 / (3.25 - 0.75) = 78.0 m,
    /// and passes behind its far edge (y = 3.5) when 3.25 (D - 60) / D = 1.75, i.e.
    ///     D = 60 x 3.25 / (3.25 - 1.75) = 130.0 m.
    /// Eyes closer than 78 m see the sign (the line goes in front of the wall), eyes from 78 m to
    /// 130 m do not, and eyes beyond 130 m see it again. The continuous clear visibility distance
    /// is 78.0 m and it is visible again from 130 m.
    #[test]
    fn straight_level_road_with_a_wall_by_similar_triangles() {
        let approach = Approach { alignment: straight(1000.0), sign_station_m: 500.0, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        let mut scene = ObstacleScene::new();
        let (v, t) = cuboid_m([440.0, 2.5, 0.0], [440.001, 3.5, 6.0]);
        scene.add_mesh_m(ObstacleKind::Other, "wall", Some(ElementId(9)), &v, &t);
        scene.finish();
        let sweep = Sweeper::new(point_face(500.0, 5.0, 2.0), &approach, params()).with_scene(&scene, &[]).run();
        assert!((sweep.full.distance_m - 78.0).abs() < 0.02, "clear distance {}", sweep.full.distance_m);
        assert!(!sweep.full.reaches_start);
        let b = &sweep.full.beyond.as_ref().unwrap().blockers[0];
        assert_eq!(b.kind, ObstacleKind::Other);
        assert_eq!(b.element, Some(ElementId(9)));
        // Hit at the wall's near face, x = 440: station 440.
        assert!((b.station_m.unwrap() - 440.0).abs() < 0.3);
        // Visible again from about 130 m (the first sweep position past it is 130 or 132).
        let again = &sweep.full.visible_again;
        assert_eq!(again.len(), 1);
        assert!(again[0].0 >= 130.0 && again[0].0 <= 132.0 && (again[0].1 - 300.0).abs() < 1e-9, "{again:?}");
        // The summary names the wall over the positions 80 to 130.
        let s = &sweep.blockers[0];
        assert!(s.from_distance_m >= 78.0 && s.to_distance_m <= 130.0);
    }

    /// A crest curve hides the sign, compared with the parabola worked independently.
    ///
    /// Grades +2 % then -2 %, PVI at station 500 (elevation 100), symmetric parabola of length
    /// L = 400 m. Relative to the horizontal tangent at the apex (station 500) the road is
    /// z = z_apex - k x^2 with k = A / (2 L) = 0.04 / 800 = 5e-5 (x measured from the apex).
    /// A line that just grazes the parabola at x0 leaves an eye h1 above the road at x1 = x0 - d1
    /// and meets a point h2 above the road at x2 = x0 + d2. The vertical gap between a tangent
    /// and the parabola at horizontal distance d from the point of tangency is k d^2, so
    ///     h1 = k d1^2  =>  d1 = sqrt(h1 / k),      h2 = k d2^2  =>  d2 = sqrt(h2 / k)
    /// and the sight distance is S = d1 + d2 = (sqrt(h1) + sqrt(h2)) / sqrt(k).
    /// With h1 = 1.05 m and h2 = 2.0 m: S = (1.024695 + 1.414214) / 0.0070711 = 344.92 m.
    /// (Tangency at x0 = x2 - d2 = 180 - 200 = -20 is inside the curve, and so is the eye at
    /// x1 = -164.9, so the closed form holds.)
    #[test]
    fn a_crest_curve_hides_the_sign_as_the_parabola_says() {
        let mut a = straight(1000.0);
        a.profile = Some(
            Profile::new("P", vec![Pvi::point(0.0, 90.0), Pvi::parabola(500.0, 100.0, 400.0), Pvi::point(1000.0, 90.0)]).unwrap(),
        );
        let sign_station = 680.0;
        let road = a.profile.as_ref().unwrap().elevation_at(sign_station).unwrap();
        let approach = Approach { alignment: a, sign_station_m: sign_station, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        // The target is 2.0 m above the road at the sign's station, 5 m off the centreline.
        let face = point_face(sign_station, 5.0, road + 2.0);
        let sweep = Sweeper::new(face, &approach, SweepParams { start_distance_m: 500.0, ..params() }).run();
        let expected = ((1.05f64).sqrt() + (2.0f64).sqrt()) / (5e-5f64).sqrt();
        assert!((expected - 344.92).abs() < 0.01);
        assert!((sweep.full.distance_m - expected).abs() < 0.1, "got {} expected {expected}", sweep.full.distance_m);
        assert_eq!(sweep.full.beyond.as_ref().unwrap().blockers[0].kind, ObstacleKind::RoadProfile);
        // The grazing point is x0 = -20 from the apex: station 480.
        let st = sweep.full.beyond.as_ref().unwrap().blockers[0].station_m.unwrap();
        assert!((st - 480.0).abs() < 2.0, "blocked at station {st}");
    }

    /// The inside of a horizontal curve with a lateral obstruction offset: mid-ordinate relation.
    ///
    /// Centreline radius R = 300 turning right (the centre of curvature is on the right). The
    /// driver keeps left at offset -1.75 and so runs on radius r = R + 1.75 = 301.75. The sign
    /// point is on the same path. An obstruction begins at offset +3.0 (on the inside, right),
    /// that is at radius r0 = R - 3.0 = 297.0, a distance m = r - r0 = 4.75 m inside the path.
    /// The chord between two points of the path whose radii are a central angle phi apart has its
    /// mid-ordinate (sagitta) r (1 - cos(phi/2)) towards the centre. It reaches the obstruction
    /// when that equals m, i.e. when its chord length is
    ///     c = 2 sqrt(r^2 - r0^2) = 2 sqrt(4.75 x 598.75) = 106.66 m
    /// (the small-angle form c = sqrt(8 r m) = 107.1 m is the usual shortcut). The central angle is
    ///     phi = 2 asin(c / 2r)
    /// and the distance along the alignment is R phi, since stations run on the centreline.
    #[test]
    fn a_horizontal_curve_with_a_lateral_obstruction_by_the_mid_ordinate() {
        let a = AlignmentBuilder::new("C", 0.0, 0.0, 0.0, 0.0).line(100.0).arc(300.0, 300.0, Turn::Right).line(100.0).build().unwrap();
        let sign_station = 350.0; // 250 m into the arc
        let (sx, sy) = a.point_at_offset(sign_station, -1.75).unwrap();
        let heading = a.point_at(sign_station).unwrap().heading_rad.to_degrees();
        let face = FaceGeometry { origin: [sx, sy, 2.0], rotation_deg: heading - 90.0, width_m: 0.0, height_m: 0.0 };
        let approach = Approach { alignment: a, sign_station_m: sign_station, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        let lateral = [LateralObstruction::new(ObstructionSide::Right, 3.0, "inside bank")];
        let sweep = Sweeper::new(face, &approach, SweepParams { start_distance_m: 240.0, ..params() }).with_lateral(&lateral).run();
        let (r, r0, big_r) = (301.75f64, 297.0f64, 300.0f64);
        let chord = 2.0 * (r * r - r0 * r0).sqrt();
        let phi = 2.0 * (chord / (2.0 * r)).asin();
        let expected = big_r * phi;
        assert!((chord - 106.66).abs() < 0.01 && ((8.0f64 * r * 4.75).sqrt() - 107.1).abs() < 0.05);
        assert!((sweep.full.distance_m - expected).abs() < 0.05, "got {} expected {expected}", sweep.full.distance_m);
        assert_eq!(sweep.full.beyond.as_ref().unwrap().blockers[0].kind, ObstacleKind::LateralOffset);
    }

    /// One sign hides another. Sign A (1 m wide, 1 m high, lower edge 2 m) stands at x = 460,
    /// y = 4 to 5 (left verge); sign B the same size at x = 500. The eye is at y = 1.75, z = 1.05.
    /// The first point of B to be hidden as the eye moves back is its far top corner
    /// (y = 5.0, z = 3.0): the line passes A's plane (x = 460) at
    ///     y = 1.75 + 3.25 r,  z = 1.05 + 1.95 r,   r = (D - 40) / D
    /// and is inside A (y >= 4) when r >= 2.25 / 3.25 = 0.6923, i.e. D >= 40 / (1 - 0.6923) = 130 m.
    /// The near lower corner (y = 4, z = 2) is never hidden (it is level with A's edge), so B is
    /// partly visible at every distance.
    #[test]
    fn one_sign_masks_another() {
        let approach = Approach { alignment: straight(1000.0), sign_station_m: 500.0, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        // Sign B, lower-left at (500, 5): its width runs towards -Y, to y = 4.
        let b = FaceGeometry { origin: [500.0, 5.0, 2.0], rotation_deg: -90.0, width_m: 1.0, height_m: 1.0 };
        let mut scene = ObstacleScene::new();
        let (v, t) = cuboid_m([460.0, 4.0, 2.0], [460.001, 5.0, 3.0]);
        scene.add_mesh_m(ObstacleKind::Sign, "A", Some(ElementId(2)), &v, &t);
        scene.finish();
        let sweep = Sweeper::new(b, &approach, SweepParams { grid: 3, ..params() }).with_scene(&scene, &[]).run();
        assert!((sweep.full.distance_m - 130.0).abs() < 0.02, "full {}", sweep.full.distance_m);
        assert!(sweep.partial.reaches_start, "partial {:?}", sweep.partial.distance_m);
        let m = sweep.masking_signs();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].element, Some(ElementId(2)));
        assert!(m[0].from_distance_m >= 130.0);
    }

    /// Obliquity: the angle between the line of sight and the face normal. Eye 100 m back and
    /// 3.25 m across (y = 1.75 against the face at y = 5), both 1.05 m high: the line of sight
    /// from the face makes atan(3.25 / 100) = 1.861 degrees with the road direction, which is the
    /// normal of a face square to the road. Turned 5 degrees towards the road (the 95 degrees of
    /// Chapter 1 para 1.51, in either sense) the angle changes by 5 degrees to 3.139 degrees.
    #[test]
    fn obliquity_angle() {
        let approach = Approach { alignment: straight(1000.0), sign_station_m: 500.0, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        let p = SweepParams { start_distance_m: 100.0, step_m: 100.0, ..params() };
        let square = Sweeper::new(point_face(500.0, 5.0, 1.05), &approach, p).sample_at(100.0).unwrap();
        let expected = (3.25f64 / 100.0).atan().to_degrees();
        assert!((square.obliquity_deg - expected).abs() < 1e-9, "{}", square.obliquity_deg);
        assert!((square.plan_obliquity_deg - expected).abs() < 1e-9);
        let turned = FaceGeometry { rotation_deg: -85.0, ..point_face(500.0, 5.0, 1.05) };
        let s = Sweeper::new(turned, &approach, p).sample_at(100.0).unwrap();
        assert!((s.obliquity_deg - (5.0 - expected)).abs() < 1e-9, "{}", s.obliquity_deg);
        // Observation angle (travel direction to the line to the face centre) is the same angle here.
        assert!((square.observation_angle_deg - expected).abs() < 1e-9);
    }

    #[test]
    fn an_eye_behind_the_face_does_not_see_it() {
        let approach = Approach { alignment: straight(1000.0), sign_station_m: 500.0, direction: TravelDirection::DownStation, eye_offset_m: 1.75, fallback_road_z_m: 0.0 };
        // The face serves up-station traffic, the driver comes the other way.
        let s = Sweeper::new(point_face(500.0, 5.0, 1.05), &approach, params()).sample_at(50.0).unwrap();
        assert_eq!(s.state, SampleState::Blocked);
        assert_eq!(s.blockers[0].kind, ObstacleKind::BackOfSign);
    }

    #[test]
    fn the_sweep_stops_where_the_alignment_starts() {
        let approach = Approach { alignment: straight(1000.0), sign_station_m: 120.0, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        let sweep = Sweeper::new(point_face(120.0, 5.0, 1.05), &approach, params()).run();
        assert!((sweep.swept_to_m - 120.0).abs() < 1e-9);
        assert!(sweep.truncated.is_some());
        assert!(sweep.full.reaches_start);
    }
}
