// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Visibility analysis of the signs of a document: the facts of a sign gathered from its
//! element, the sweep, the checks, and the road-wide run with its CSV.

use std::collections::BTreeMap;
use std::sync::Arc;

use afrobim_alignment::{Alignment, TrafficSide};
use afrobim_coordination::Placement;
use afrobim_draw::Mesh3;
use afrobim_kernel::{Document, Element, ElementId};
use afrobim_signs::SignDefinition;
use afrobim_supports::{supports_of, FOUNDATION_CATEGORY, POST_CATEGORY, PROP_MOUNTING_HEIGHT_MM};
use serde::{Deserialize, Serialize};

use crate::checks::{classify, run_checks, CheckInput, CheckLine, SignGroup, Status};
use crate::inputs::{self, VisibilityInputs};
use crate::scene::{ObstacleKind, ObstacleScene};
use crate::sources::{ug_min_spacing_m, UG_SPACING};
use crate::sweep::{local_alignment, Approach, FaceGeometry, SweepResult, Sweeper, TravelDirection};

/// The category of sign elements.
pub const SIGN_CATEGORY: &str = "sign";

/// Categories that are records or construction lines, not obstructions.
const NOT_OBSTRUCTIONS: [&str; 10] =
    ["alignment", "alignment_design", "alignment_options", "coordination", "schedule", "estimate", "progress", "project_info", "sheets", "marking"];

/// Thickness of the slab that stands for a sign face as an obstruction to other sign faces, mm.
/// A modelling choice (a sign face is flat in the model); no source states it.
pub const SIGN_SLAB_THICKNESS_MM: f64 = 10.0;

/// The `x-height` value in a module's derived values (`"123.4 mm"`), in mm.
pub fn derived_x_height(derived: &BTreeMap<String, String>) -> Option<f64> {
    let v = derived.get("x-height")?;
    v.trim().trim_end_matches("mm").trim().parse().ok()
}

/// The alignments of a document: element id, name and the alignment.
pub fn alignments_of(doc: &Document) -> Vec<(ElementId, String, Alignment)> {
    doc.elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok().map(|a| (e.id, e.name.clone(), a)))
        .collect()
}

/// The mounting height recorded on the first post of a sign, mm.
fn recorded_mounting_mm(doc: &Document, sign: ElementId) -> Option<f64> {
    doc.elements()
        .iter()
        .find(|e| e.category == POST_CATEGORY && e.host == Some(sign))
        .and_then(|p| p.properties.get(PROP_MOUNTING_HEIGHT_MM))
        .and_then(|v| v.parse().ok())
}

/// What goes into the obstruction scene built from a document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneOptions {
    /// Add every ground surface element as a TIN.
    pub include_ground: bool,
    /// Build every corridor and add its finished surface and side slopes (slow on a large model).
    pub include_corridors: bool,
}

impl Default for SceneOptions {
    fn default() -> Self {
        SceneOptions { include_ground: true, include_corridors: true }
    }
}

/// Builds the obstruction scene of a document: ground surfaces, corridor surfaces, sign faces
/// (as thin slabs) and the solids of every other placed element, in site metres.
///
/// `solids_of` gives an element's solids in its own frame, millimetres (the generated solids of
/// its module); it is only called for elements that have a site placement and are not signs,
/// alignments or records. The frame is the one the 3D view and the clash scene use: the element's
/// site position, lifted by the mounting height for anything but a support, turned by its
/// rotation.
pub fn scene_from_document(doc: &Document, solids_of: &dyn Fn(&Element) -> Vec<Mesh3>, options: &SceneOptions) -> ObstacleScene {
    let mut scene = ObstacleScene::new();
    for e in doc.elements() {
        match e.category.as_str() {
            c if NOT_OBSTRUCTIONS.contains(&c) => {}
            "surface" => {
                if options.include_ground {
                    if let Some(tin) = afrobim_corridor::surface_of(doc, e.id) {
                        scene.add_tin(&e.name, Some(e.id), Arc::new(tin));
                    }
                }
            }
            "corridor" => {
                if options.include_corridors {
                    if let Ok(corridor) = afrobim_corridor::corridor_of(doc, e.id) {
                        let m = afrobim_corridor::corridor_meshes(&corridor);
                        let placement = Placement::new([m.origin_x_m * 1000.0, m.origin_y_m * 1000.0, 0.0], 0.0);
                        for mesh in [&m.top, &m.cut_slopes, &m.fill_slopes] {
                            if !mesh.triangles.is_empty() {
                                scene.add_mesh_mm(ObstacleKind::CorridorSurface, &e.name, Some(e.id), mesh, &placement);
                            }
                        }
                    }
                }
            }
            category => {
                let Some(site) = &e.site else { continue };
                let support = category == POST_CATEGORY || category == FOUNDATION_CATEGORY;
                let lift = if support { 0.0 } else { recorded_mounting_mm(doc, e.id).unwrap_or(0.0) };
                let placement = Placement::new([site.x, site.y, site.z + lift], site.rotation_deg);
                let solids = if category == SIGN_CATEGORY {
                    vec![Mesh3::cuboid("Sign face", [0.0, 0.0, 0.0], [e.width, SIGN_SLAB_THICKNESS_MM, e.height], [255, 255, 255])]
                } else {
                    solids_of(e)
                };
                for mesh in solids.iter().filter(|m| !m.triangles.is_empty()) {
                    scene.add_mesh_mm(ObstacleKind::of_category(category), &e.name, Some(e.id), mesh, &placement);
                }
            }
        }
    }
    scene.finish();
    scene
}

/// What the analysis of a sign needs from the document and the caller.
pub struct SignContext<'a> {
    pub doc: &'a Document,
    /// Ground and solids; `None` tests only the road profile and the entered lateral offsets.
    pub scene: Option<&'a ObstacleScene>,
    /// The x-height of a sign's legend, mm, when known (the sign module's derived `x-height`).
    pub x_height_of: &'a dyn Fn(&Element) -> Option<f64>,
}

/// The analysis of one sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignAnalysis {
    pub element: ElementId,
    pub mark: String,
    pub alignment: Option<String>,
    pub station_m: Option<f64>,
    pub station_label: Option<String>,
    /// Signed offset of the face centre from the centreline, m.
    pub offset_m: Option<f64>,
    pub direction: Option<TravelDirection>,
    pub eye_offset_m: Option<f64>,
    pub speed_kph: Option<f64>,
    /// Height of the lower edge above the road level at the sign, m.
    pub lower_edge_above_road_m: Option<f64>,
    pub face: Option<FaceGeometry>,
    /// The approach the sweep ran on (a stretch of the alignment, the driver's path), kept for
    /// the drawings.
    pub approach: Option<Approach>,
    /// Elements that never blocked this sign's view: itself and its supports.
    pub ignore: Vec<ElementId>,
    /// The inputs that were used (stored ones, or the defaults supplied).
    pub inputs: VisibilityInputs,
    pub sweep: Option<SweepResult>,
    pub lines: Vec<CheckLine>,
    /// Why the sign was not analysed at all (no site placement, not a sign).
    pub skipped: Option<String>,
}

impl SignAnalysis {
    fn skipped(e: Option<&Element>, id: ElementId, reason: &str, inputs: VisibilityInputs) -> SignAnalysis {
        SignAnalysis {
            element: id,
            mark: e.map_or(String::new(), |e| e.name.clone()),
            alignment: None,
            station_m: None,
            station_label: None,
            offset_m: None,
            direction: None,
            eye_offset_m: None,
            speed_kph: None,
            lower_edge_above_road_m: None,
            face: None,
            approach: None,
            ignore: Vec::new(),
            inputs,
            sweep: None,
            lines: Vec::new(),
            skipped: Some(reason.to_string()),
        }
    }

    /// The most severe status among the lines.
    pub fn worst(&self) -> Status {
        self.lines.iter().map(|l| l.status).max_by_key(|s| s.severity()).unwrap_or(Status::Information)
    }

    pub fn count(&self, status: Status) -> usize {
        self.lines.iter().filter(|l| l.status == status).count()
    }

    /// The overall result: FAILS, DIFFERS or INPUT NEEDED if any line has it (in that order),
    /// otherwise OK, or INFORMATION when no line made a verdict.
    pub fn overall(&self) -> Status {
        match self.worst() {
            Status::Information if self.lines.iter().any(|l| l.status == Status::Ok) => Status::Ok,
            s => s,
        }
    }

    /// The continuous clear visibility distance of the whole face, m, and whether it is a lower
    /// bound (visible all the way to the start of the sweep).
    pub fn clear_full(&self) -> Option<(f64, bool)> {
        self.sweep.as_ref().map(|s| (s.full.distance_m, s.full.reaches_start))
    }

    /// The report of this sign as text lines: a heading and one row per check line.
    pub fn report(&self) -> Vec<String> {
        let mut out = vec![format!(
            "{} ({})",
            self.mark,
            match (&self.alignment, &self.station_label) {
                (Some(a), Some(s)) => format!("{a} station {s}"),
                _ => "not placed on an alignment".to_string(),
            }
        )];
        if let Some(reason) = &self.skipped {
            out.push(format!("  not analysed: {reason}"));
        }
        out.extend(self.lines.iter().map(|l| format!("  {}", l.text())));
        out
    }
}

fn wrap_deg(a: f64) -> f64 {
    let r = a.rem_euclid(360.0);
    if r > 180.0 {
        r - 360.0
    } else {
        r
    }
}

/// Analyses one sign with the inputs stored on it (or `default_inputs`, or the defaults).
pub fn analyse_sign(ctx: &SignContext, id: ElementId, default_inputs: Option<&VisibilityInputs>) -> SignAnalysis {
    analyse_sign_in(ctx, &alignments_of(ctx.doc), id, default_inputs)
}

/// [`analyse_sign`] with the alignments already collected (for a run over many signs).
pub fn analyse_sign_in(ctx: &SignContext, alignments: &[(ElementId, String, Alignment)], id: ElementId, default_inputs: Option<&VisibilityInputs>) -> SignAnalysis {
    let doc = ctx.doc;
    let stored = inputs::load(doc, id);
    let inp = stored.or_else(|| default_inputs.cloned()).unwrap_or_default();
    let Some(e) = doc.find(id) else { return SignAnalysis::skipped(None, id, "the element is not in the document", inp) };
    if e.category != SIGN_CATEGORY {
        return SignAnalysis::skipped(Some(e), id, "the element is not a sign", inp);
    }
    let Some(site) = &e.site else { return SignAnalysis::skipped(Some(e), id, "the sign has no site placement", inp) };
    let def: Option<SignDefinition> = serde_json::from_value(e.data.clone()).ok();

    // Mounting height: entered, or recorded on the sign's post.
    let mounting_m = inp.mounting_height_m.or_else(|| recorded_mounting_mm(doc, id).map(|mm| mm / 1000.0));
    let face = FaceGeometry {
        origin: [site.x / 1000.0, site.y / 1000.0, site.z / 1000.0 + mounting_m.unwrap_or(0.0)],
        rotation_deg: site.rotation_deg,
        width_m: e.width / 1000.0,
        height_m: e.height / 1000.0,
    };
    let centre = face.centre();

    // The alignment: the one named on the placement, else the nearest.
    let named = site.alignment.as_ref().and_then(|n| alignments.iter().find(|(_, name, _)| name == n));
    let found = named.or_else(|| {
        alignments
            .iter()
            .filter_map(|a| a.2.station_offset(centre[0], centre[1]).map(|(s, o)| (a, s, o)))
            .min_by(|x, y| x.2.abs().partial_cmp(&y.2.abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(a, _, _)| a)
    });
    let mut base = SignAnalysis::skipped(Some(e), id, "", inp.clone());
    base.skipped = None;
    base.face = Some(face);
    let (group, advance) = def.as_ref().map_or((SignGroup::Lettered, false), classify);
    let speed_kph = inp.approach_speed_kph.or_else(|| def.as_ref().map(|d| d.speed_kph as f64).filter(|v| *v > 0.0));
    base.speed_kph = speed_kph;

    let Some((_, aname, alignment)) = found else {
        let reason = "the sign is not on an alignment of the document (no alignment named on its placement and none within reach)".to_string();
        let ci = CheckInput {
            inputs: &inp,
            group,
            advance_direction: advance,
            speed_kph,
            sweep: None,
            no_sweep_reason: Some(reason),
            x_height_mm: inp.x_height_mm.or_else(|| (ctx.x_height_of)(e)),
            words: inp.words.or_else(|| def.as_ref().map(|d| d.destinations.len() as u32).filter(|n| *n > 0)),
            lateral_s_m: None,
            direction: TravelDirection::UpStation,
            sign_station_m: None,
            lower_edge_above_road_m: mounting_m,
            lower_edge_above_eye_m: mounting_m.map(|m| m - inp.sweep.eye_height_m),
            near_edge_offset_m: None,
            sign_on_left: None,
            grade_in_travel_direction: None,
            face_turn_from_square_deg: None,
            has_scene: ctx.scene.is_some(),
        };
        base.lines = run_checks(&ci);
        return base;
    };
    base.alignment = Some(aname.clone());

    let (station, offset) = alignment.station_offset(centre[0], centre[1]).or_else(|| site.station_m.map(|s| (s, site.offset_m.unwrap_or(0.0)))).unwrap_or((0.0, 0.0));
    base.station_m = Some(station);
    base.station_label = Some(alignment.station_label(station, 3));
    base.offset_m = Some(offset);

    // Direction of travel: entered, else the traffic the face serves.
    let heading = alignment.point_at(station).map_or(0.0, |p| p.heading_rad);
    let served = face.served_heading_deg().to_radians();
    let direction = inp.direction.unwrap_or(if (served - heading).cos() >= 0.0 { TravelDirection::UpStation } else { TravelDirection::DownStation });
    base.direction = Some(direction);

    let traffic: TrafficSide = inp.traffic_side;
    let eye_offset = inp.eye_offset.map(|o| o.resolve(direction, traffic));
    base.eye_offset_m = eye_offset;

    let fallback_z = site.z / 1000.0;
    let travel_heading = heading + if direction == TravelDirection::DownStation { std::f64::consts::PI } else { 0.0 };
    let n = face.normal();
    let face_turn = wrap_deg(n[1].atan2(n[0]).to_degrees() - (travel_heading + std::f64::consts::PI).to_degrees());

    // Corners nearest the road, for the clearance check.
    let corner_offsets: Vec<f64> = face.corners().iter().filter_map(|c| alignment.station_offset(c[0], c[1]).map(|so| so.1)).collect();
    let near_edge = corner_offsets.iter().cloned().min_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap_or(std::cmp::Ordering::Equal));

    let grade = alignment.profile.as_ref().and_then(|p| p.grade_at(station).ok()).map(|g| g * direction.sign());
    let words = inp.words.or_else(|| def.as_ref().map(|d| d.destinations.len() as u32).filter(|n| *n > 0));
    let x_height = inp.x_height_mm.or_else(|| (ctx.x_height_of)(e));

    let mut no_sweep_reason = None;
    let mut sweep = None;
    let mut s_lat = None;
    let mut road_here = fallback_z;
    if let Some(eo) = eye_offset {
        let reach = inp.sweep.start_distance_m + 50.0;
        let (a, b) = (station, station - direction.sign() * reach);
        let local = local_alignment(alignment, a.min(b) - 50.0, a.max(b) + 50.0);
        let approach = Approach { alignment: local, sign_station_m: station, direction, eye_offset_m: eo, fallback_road_z_m: fallback_z };
        road_here = approach.road_level(station);
        s_lat = Some((offset - eo).abs());
        let mut ignore = vec![id];
        ignore.extend(supports_of(doc, id));
        let mut sweeper = Sweeper::new(face, &approach, inp.sweep).with_lateral(&inp.lateral);
        if let Some(scene) = ctx.scene {
            sweeper = sweeper.with_scene(scene, &ignore);
        }
        sweep = Some(sweeper.run());
        base.ignore = ignore;
        base.approach = Some(approach);
    } else {
        no_sweep_reason = Some("enter the driver's lateral position (the lane the sign is read from) to run the sweep".to_string());
    }
    let lower_edge_above_road = mounting_m.map(|_| face.origin[2] - road_here);
    let ci = CheckInput {
        inputs: &inp,
        group,
        advance_direction: advance,
        speed_kph,
        sweep: sweep.as_ref(),
        no_sweep_reason,
        x_height_mm: x_height,
        words,
        lateral_s_m: s_lat,
        direction,
        sign_station_m: Some(station),
        lower_edge_above_road_m: lower_edge_above_road,
        lower_edge_above_eye_m: mounting_m.map(|_| face.origin[2] - road_here - inp.sweep.eye_height_m),
        near_edge_offset_m: near_edge,
        sign_on_left: Some(offset < 0.0),
        grade_in_travel_direction: grade,
        face_turn_from_square_deg: Some(face_turn),
        has_scene: ctx.scene.is_some(),
    };
    base.lines = run_checks(&ci);
    base.lower_edge_above_road_m = lower_edge_above_road;
    base.sweep = sweep;
    base
}

/// Options of a road-wide run.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Inputs for signs that have none stored (for example one entered set for the whole road).
    pub default_inputs: Option<VisibilityInputs>,
    /// Restrict the run to these signs; `None` runs every placed sign.
    pub only: Option<Vec<ElementId>>,
}

/// The result of running every placed sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadRun {
    pub analyses: Vec<SignAnalysis>,
}

/// Runs every placed sign of every alignment, then adds the spacing lines between signs.
pub fn run_all(ctx: &SignContext, options: &RunOptions) -> RoadRun {
    let alignments = alignments_of(ctx.doc);
    let ids: Vec<ElementId> = ctx
        .doc
        .elements()
        .iter()
        .filter(|e| e.category == SIGN_CATEGORY && e.site.is_some())
        .map(|e| e.id)
        .filter(|id| options.only.as_ref().is_none_or(|o| o.contains(id)))
        .collect();
    let mut analyses: Vec<SignAnalysis> = ids.iter().map(|&id| analyse_sign_in(ctx, &alignments, id, options.default_inputs.as_ref())).collect();
    apply_spacing(&mut analyses);
    RoadRun { analyses }
}

/// Adds the spacing line of the Uganda manual (D.4.2) to the second sign of each consecutive
/// pair on the same alignment, side and direction of travel: closer than 0.6 v (v in km/h) or 30 m.
/// The provision is for the two signs of one hazard sequence; the line says so, and the engineer
/// decides whether the pair belongs together.
pub fn apply_spacing(analyses: &mut [SignAnalysis]) {
    let mut order: Vec<usize> = (0..analyses.len()).filter(|&i| analyses[i].skipped.is_none() && analyses[i].station_m.is_some()).collect();
    order.sort_by(|&a, &b| {
        let key = |i: usize| (analyses[i].alignment.clone(), analyses[i].offset_m.map(|o| o < 0.0), analyses[i].direction.map(|d| d == TravelDirection::UpStation));
        key(a).cmp(&key(b)).then_with(|| {
            let dir = analyses[a].direction.map_or(1.0, |d| d.sign());
            (analyses[a].station_m.unwrap_or(0.0) * dir).partial_cmp(&(analyses[b].station_m.unwrap_or(0.0) * dir)).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let same = analyses[a].alignment == analyses[b].alignment
            && analyses[a].offset_m.map(|o| o < 0.0) == analyses[b].offset_m.map(|o| o < 0.0)
            && analyses[a].direction == analyses[b].direction;
        if !same || !analyses[b].inputs.standards.uganda {
            continue;
        }
        let gap = (analyses[b].station_m.unwrap_or(0.0) - analyses[a].station_m.unwrap_or(0.0)).abs();
        let Some(v) = analyses[b].speed_kph.or(analyses[a].speed_kph) else {
            continue;
        };
        let need = ug_min_spacing_m(v);
        if gap + 1e-9 < need {
            let text = format!(
                "'{}' is {gap:.1} m beyond '{}' in the direction of travel; the manual asks for at least 0.6 v = {:.1} m ({v:.0} km/h) and never less than 30 m between the two signs of a hazard sequence, so that the first does not obscure the second (applies where the two signs belong together)",
                analyses[b].mark,
                analyses[a].mark,
                need
            );
            analyses[b].lines.push(CheckLine {
                id: "spacing.ug".to_string(),
                status: Status::Differs,
                title: "Spacing between signs, Uganda".to_string(),
                detail: text,
                citation: UG_SPACING.to_string(),
            });
        }
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The header of [`RoadRun::summary_csv`].
pub const SUMMARY_CSV_HEADER: &str = "mark,alignment,station,station_m,offset_m,side,direction,eye_offset_m,speed_kph,lower_edge_above_road_m,clear_visibility_full_m,clear_visibility_full_is_lower_bound,clear_visibility_partial_m,limiting_obstruction,limiting_station_m,obliquity_at_limit_deg,masked_by,ok,fails,differs,input_needed,overall,skipped";

/// The header of [`RoadRun::detail_csv`].
pub const DETAIL_CSV_HEADER: &str = "mark,alignment,station,line,status,title,detail,citation";

impl RoadRun {
    pub fn analysis(&self, id: ElementId) -> Option<&SignAnalysis> {
        self.analyses.iter().find(|a| a.element == id)
    }

    /// One row per sign.
    pub fn summary_csv(&self) -> String {
        let mut out = String::from(SUMMARY_CSV_HEADER);
        out.push('\n');
        for a in &self.analyses {
            let full = a.sweep.as_ref().map(|s| (s.full.distance_m, s.full.reaches_start));
            let partial = a.sweep.as_ref().map(|s| s.partial.distance_m);
            let beyond = a.sweep.as_ref().and_then(|s| s.full.beyond.as_ref()).and_then(|b| b.blockers.first());
            let masked: Vec<String> = a.sweep.as_ref().map(|s| s.masking_signs().iter().map(|b| b.label.clone()).collect()).unwrap_or_default();
            let f = |v: Option<f64>, d: usize| v.map_or(String::new(), |v| format!("{v:.d$}"));
            let row = [
                csv_field(&a.mark),
                csv_field(a.alignment.as_deref().unwrap_or("")),
                csv_field(a.station_label.as_deref().unwrap_or("")),
                f(a.station_m, 3),
                f(a.offset_m, 3),
                a.offset_m.map_or(String::new(), |o| if o < 0.0 { "left" } else { "right" }.to_string()),
                a.direction.map_or(String::new(), |d| d.label().to_string()),
                f(a.eye_offset_m, 2),
                f(a.speed_kph, 1),
                f(a.lower_edge_above_road_m, 3),
                f(full.map(|x| x.0), 1),
                full.map_or(String::new(), |x| x.1.to_string()),
                f(partial, 1),
                csv_field(&beyond.map_or(String::new(), |b| format!("{} {}", b.kind.label(), b.label))),
                f(beyond.and_then(|b| b.station_m), 1),
                f(a.sweep.as_ref().and_then(|s| s.full.at_limit.as_ref()).map(|e| e.obliquity_deg), 1),
                csv_field(&masked.join("; ")),
                a.count(Status::Ok).to_string(),
                a.count(Status::Fails).to_string(),
                a.count(Status::Differs).to_string(),
                a.count(Status::InputNeeded).to_string(),
                a.overall().label().to_string(),
                csv_field(a.skipped.as_deref().unwrap_or("")),
            ];
            out.push_str(&row.join(","));
            out.push('\n');
        }
        out
    }

    /// One row per check line of every sign.
    pub fn detail_csv(&self) -> String {
        let mut out = String::from(DETAIL_CSV_HEADER);
        out.push('\n');
        for a in &self.analyses {
            for l in &a.lines {
                let row = [
                    csv_field(&a.mark),
                    csv_field(a.alignment.as_deref().unwrap_or("")),
                    csv_field(a.station_label.as_deref().unwrap_or("")),
                    csv_field(&l.id),
                    l.status.label().to_string(),
                    csv_field(&l.title),
                    csv_field(&l.detail),
                    csv_field(&l.citation),
                ];
                out.push_str(&row.join(","));
                out.push('\n');
            }
        }
        out
    }

    /// The whole report as text lines: a table of signs, then the detail of each.
    pub fn report(&self) -> Vec<String> {
        let mut out = vec![format!("{:<14} {:<14} {:>11} {:>12} {:<13} {:>3} {:>5} {:>7}", "Sign", "Alignment", "Station", "Clear (m)", "Overall", "OK", "FAILS", "DIFFERS")];
        for a in &self.analyses {
            let clear = a.clear_full().map_or("-".to_string(), |(d, lb)| format!("{}{d:.1}", if lb { ">=" } else { "" }));
            out.push(format!(
                "{:<14} {:<14} {:>11} {:>12} {:<13} {:>3} {:>5} {:>7}",
                a.mark,
                a.alignment.as_deref().unwrap_or("-"),
                a.station_label.as_deref().unwrap_or("-"),
                clear,
                a.overall().label(),
                a.count(Status::Ok),
                a.count(Status::Fails),
                a.count(Status::Differs)
            ));
        }
        out.push(String::new());
        for a in &self.analyses {
            out.extend(a.report());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::{EyeOffset, Standards};
    use crate::sources::SpeedBandMph;
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi};
    use afrobim_corridor::Tin;
    use afrobim_kernel::{Pt, SitePlacement};
    use afrobim_signs::{Destination, Direction};

    /// A document with a level straight road at 100 m along +X, stations 0 to 1000, and a ground
    /// surface given by `ground(x, y)`.
    fn road_doc(ground: impl Fn(f64, f64) -> f64) -> Document {
        let mut doc = Document::default();
        let mut a = AlignmentBuilder::new("Main", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 100.0), Pvi::point(1000.0, 100.0)]).unwrap());
        doc.add("alignment", "Main", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());
        let tin = Tin::from_grid(0.0, -10.0, 1.0, 10.0, 1001, 3, ground).unwrap();
        afrobim_corridor::add_surface(&mut doc, "Ground", &tin, None).unwrap();
        doc
    }

    /// A sign on the left of the road (offset -5 m, so y = +5 for the +X alignment), serving
    /// up-station traffic, with a post recording the mounting height.
    fn add_sign(doc: &mut Document, mark: &str, station_m: f64, w_mm: f64, h_mm: f64, mounting_mm: f64) -> ElementId {
        let def = SignDefinition::stack(mark, 80, vec![Destination::new("Kasese", Direction::Ahead), Destination::new("Masheruka", Direction::Right)]);
        let id = doc.add("sign", mark, Pt::new(0.0, 0.0), w_mm, h_mm, "Signs", serde_json::to_value(&def).unwrap());
        doc.find_mut(id).unwrap().site = Some(SitePlacement {
            x: station_m * 1000.0,
            y: 5000.0,
            z: 100_000.0,
            rotation_deg: -90.0,
            alignment: Some("Main".to_string()),
            station_m: Some(station_m),
            offset_m: Some(-5.0),
        });
        let post = doc.add("post", &format!("{mark}-P"), Pt::new(0.0, 0.0), 100.0, 100.0, "Supports", serde_json::Value::Null);
        let p = doc.find_mut(post).unwrap();
        p.host = Some(id);
        p.properties.insert(PROP_MOUNTING_HEIGHT_MM.to_string(), mounting_mm.to_string());
        id
    }

    fn inputs() -> VisibilityInputs {
        VisibilityInputs {
            eye_offset: Some(EyeOffset::LaneCentre(1.75)),
            approach_speed_kph: Some(80.0),
            uk_band: Some(SpeedBandMph::From41To50),
            hazard_station_m: None,
            sweep: crate::sweep::SweepParams { start_distance_m: 300.0, step_m: 10.0, ..Default::default() },
            standards: Standards { uk: true, uganda: true },
            ..VisibilityInputs::default()
        }
    }

    fn no_solids(_: &Element) -> Vec<Mesh3> {
        Vec::new()
    }

    /// Ground that stands at 110 m from x = 430 to x = 450 and falls to 99 m at x = 451 (a ridge
    /// across the road at road level 100 m). The sight line to the sign at 500 is blocked by the
    /// ground until the eye is past the ridge's far ramp. The ramp is z = 110 - 11 (x - 450); an
    /// eye 1.05 m above the 100 m road is above ground from x = 450 + (110 - 101.05) / 11 =
    /// 450.8136, so the clear distance is 500 - 450.8136 = 49.19 m.
    #[test]
    fn a_ground_ridge_blocks_the_view_and_the_post_of_the_sign_itself_does_not() {
        let ridge = |x: f64, _y: f64| if (430.0..=450.0).contains(&x) { 110.0 } else { 99.0 };
        let mut doc = road_doc(ridge);
        let s1 = add_sign(&mut doc, "S1", 500.0, 2000.0, 1500.0, 1800.0);
        // The sign's own post is a solid in the scene but never blocks its own sign.
        let solids = |e: &Element| if e.category == "post" { vec![Mesh3::cylinder("Post", 0.0, 0.0, 0.0, 1800.0, 50.0, 12, [0, 0, 0])] } else { Vec::new() };
        let scene = scene_from_document(&doc, &solids, &SceneOptions::default());
        assert!(scene.has_ground());
        let xh = |_: &Element| Some(200.0);
        let ctx = SignContext { doc: &doc, scene: Some(&scene), x_height_of: &xh };
        let a = analyse_sign(&ctx, s1, Some(&inputs()));
        let sweep = a.sweep.as_ref().unwrap();
        // The lowest line (to the lower corner, 101.8 m) is clear of the ramp once the eye is
        // above ground, so the eye condition above governs the whole-face limit.
        assert!((sweep.full.distance_m - 49.19).abs() < 0.1, "got {}", sweep.full.distance_m);
        let b = &sweep.full.beyond.as_ref().unwrap().blockers[0];
        assert_eq!(b.kind, ObstacleKind::Ground);
        assert_eq!(a.direction, Some(TravelDirection::UpStation));
        assert_eq!(a.eye_offset_m, Some(-1.75));
        // The mounting height came from the post: 1.8 m above the 100 m road.
        assert!((a.lower_edge_above_road_m.unwrap() - 1.8).abs() < 1e-9);
        let line = |id: &str| a.lines.iter().find(|l| l.id == id).unwrap_or_else(|| panic!("no {id}: {:?}", a.lines.iter().map(|l| &l.id).collect::<Vec<_>>()));
        assert_eq!(line("visibility.ug").status, Status::Fails); // 49 m against 150 m for 80 km/h
        assert!(line("visibility.ug").citation.contains("Table D.2") && line("visibility.ug").citation.contains("July 2004"));
        assert_eq!(line("mounting.ug").status, Status::Ok);
        assert_eq!(line("mounting.uk").status, Status::Differs); // 1800 mm is above 900 to 1500
        assert_eq!(a.overall(), Status::Fails);
    }

    /// Two signs, one in front of the other, and the road-wide run: summary and detail CSV, the
    /// masking line and the Uganda spacing line.
    #[test]
    fn a_road_run_reports_masking_spacing_and_csv() {
        let mut doc = road_doc(|_, _| 99.0);
        let s1 = add_sign(&mut doc, "S1", 500.0, 2000.0, 1500.0, 1800.0);
        // The sign in front: 1 m wide at 460, lower edge 1.8 m, 1 m high (y = 5 to 4).
        let s2 = add_sign(&mut doc, "S2", 460.0, 1000.0, 1000.0, 1800.0);
        let scene = scene_from_document(&doc, &no_solids, &SceneOptions::default());
        let xh = |_: &Element| Some(200.0);
        let ctx = SignContext { doc: &doc, scene: Some(&scene), x_height_of: &xh };
        let run = run_all(&ctx, &RunOptions { default_inputs: Some(inputs()), only: None });
        assert_eq!(run.analyses.len(), 2);
        let a1 = run.analysis(s1).unwrap();
        // S2's slab (y = 5 to 4, z = 101.8 to 102.8, x = 460 to 460.01) hides part of S1 (y = 5 to 3,
        // z = 101.8 to 103.3) from the eye at y = 1.75, z = 101.05: the far top corner (y = 5,
        // z = 103.3) goes behind it at r = (D - 40) / D = 2.25 / 3.25, D = 130 m (as in the sweep
        // module's masking test).
        let full = a1.sweep.as_ref().unwrap().full.distance_m;
        assert!((full - 130.0).abs() < 0.1, "{full}");
        let mask = a1.lines.iter().find(|l| l.id == "mask.sign").expect("masking line");
        assert!(mask.detail.contains("'S2'"), "{}", mask.detail);
        assert_eq!(mask.status, Status::Differs);
        // Spacing: S1 is 40 m beyond S2; at 80 km/h the Uganda minimum is 48 m.
        let sp = a1.lines.iter().find(|l| l.id == "spacing.ug").expect("spacing line");
        assert!(sp.detail.contains("48.0 m") && sp.citation.contains("0.6v"), "{}", sp.detail);
        assert!(run.analysis(s2).unwrap().lines.iter().all(|l| l.id != "spacing.ug"));
        // Legibility: N = 2, 80 km/h, S = 2.25 m: R = 59.26 m, C = 12.76 m, 72.0 m needs 120.0 mm.
        let need = a1.lines.iter().find(|l| l.id == "legibility.x_required").unwrap();
        assert_eq!(need.status, Status::Ok);
        assert!(need.detail.contains("120.0 mm"), "{}", need.detail);
        // CSV: header + one row per sign, same number of fields in every row.
        let csv = run.summary_csv();
        let rows: Vec<&str> = csv.lines().collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], SUMMARY_CSV_HEADER);
        assert!(rows[1].starts_with("S1,Main,"));
        let fields = |r: &str| r.split(',').count();
        assert_eq!(fields(rows[1]), fields(rows[0]), "{}", rows[1]);
        let detail = run.detail_csv();
        assert!(detail.lines().count() > 10 && detail.starts_with(DETAIL_CSV_HEADER));
        assert!(run.report().iter().any(|l| l.contains("S1")));
        if std::env::var("VIS_PRINT").is_ok() {
            println!("{}", run.report().join("\n"));
            println!("{csv}");
        }
    }

    #[test]
    fn inputs_persist_on_the_sign_under_a_hidden_key() {
        let mut doc = road_doc(|_, _| 99.0);
        let s1 = add_sign(&mut doc, "S1", 500.0, 2000.0, 1500.0, 1800.0);
        assert!(inputs::load(&doc, s1).is_none());
        let i = inputs();
        assert!(inputs::save(&mut doc, s1, &i));
        let stored = doc.find(s1).unwrap().properties.get("check.visibility").unwrap().clone();
        assert!(stored.contains("LaneCentre"));
        assert_eq!(inputs::load(&doc, s1), Some(i));
        assert!(inputs::is_hidden_key("check.visibility"));
        // The stored inputs win over the defaults of a run.
        let scene = scene_from_document(&doc, &no_solids, &SceneOptions::default());
        let xh = |_: &Element| None;
        let ctx = SignContext { doc: &doc, scene: Some(&scene), x_height_of: &xh };
        let a = analyse_sign(&ctx, s1, Some(&VisibilityInputs::default()));
        assert!(a.sweep.is_some(), "stored inputs (with the lane) were not used");
    }

    #[test]
    fn missing_inputs_are_asked_for_not_invented() {
        let mut doc = road_doc(|_, _| 99.0);
        let s1 = add_sign(&mut doc, "S1", 500.0, 2000.0, 1500.0, 1800.0);
        let bare = doc.add("sign", "B", Pt::new(0.0, 0.0), 1000.0, 1000.0, "Signs", serde_json::Value::Null);
        let xh = |_: &Element| None;
        let ctx = SignContext { doc: &doc, scene: None, x_height_of: &xh };
        let a = analyse_sign(&ctx, s1, None);
        assert!(a.sweep.is_none());
        assert_eq!(a.lines.iter().find(|l| l.id == "sweep").unwrap().status, Status::InputNeeded);
        assert_eq!(a.lines.iter().find(|l| l.id == "legibility.x").unwrap().status, Status::InputNeeded);
        assert!(a.count(Status::InputNeeded) >= 3, "{:?}", a.lines);
        // A sign with no placement is skipped, not guessed.
        assert!(analyse_sign(&ctx, bare, None).skipped.is_some());
    }

    #[test]
    fn drawings_have_labels_and_the_right_extents() {
        let ridge = |x: f64, _y: f64| if (430.0..=450.0).contains(&x) { 110.0 } else { 99.0 };
        let mut doc = road_doc(ridge);
        let s1 = add_sign(&mut doc, "S1", 500.0, 2000.0, 1500.0, 1800.0);
        let scene = scene_from_document(&doc, &no_solids, &SceneOptions::default());
        let xh = |_: &Element| None;
        let ctx = SignContext { doc: &doc, scene: Some(&scene), x_height_of: &xh };
        let a = analyse_sign(&ctx, s1, Some(&inputs()));
        let plan = crate::drawings::plan_envelope(&a, Some(&scene)).unwrap();
        assert!(plan.drawing.primitives.len() > 50);
        assert!(plan.labels.iter().any(|l| l.kind == crate::drawings::LabelKind::Title && l.text.contains("S1")));
        assert!(plan.labels.iter().any(|l| l.kind == crate::drawings::LabelKind::Obstruction && l.text.contains("ground")));
        assert!(plan.drawing.extents.width() > 250_000.0); // 300 m of approach in millimetres
        let profile = crate::drawings::sight_line_profile(&a, Some(&scene), 5.0).unwrap();
        assert!(profile.labels.iter().any(|l| l.text.contains("blocks the sight line")));
        assert!(profile.vertical_exaggeration == 5.0 && profile.drawing.extents.height() > 0.0);
    }
}
