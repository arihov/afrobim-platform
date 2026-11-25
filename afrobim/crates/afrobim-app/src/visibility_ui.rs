// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Sign visibility in the app (`VISIBILITY`): the inputs dialog, the run, the results with the
//! check lines and their citations, the plan envelope and sight-line profile drawings, a 3D overlay
//! of the limiting sight line and its obstruction, and the road-wide CSV export.
//!
//! The inputs of a sign are stored on it as `check.visibility` (the engine's `VisibilityInputs`),
//! so `CHECKALL` and the sheets read the same record. Every field of the dialog starts empty except
//! what the engine marks as a default to confirm: the driver's eye height (1.05 m, which two held
//! sources state for other criteria). It is shown as "to confirm" and the dialog will not store or
//! run anything until the engineer ticks to accept it; the acceptance is stored beside the inputs
//! (`check.visibility.accepted`) and CHECKALL reports a record without it as input needed.
//!
//! The UK speed band of a speed is the engineer's decision, kept as the project's speed band rule
//! (PROJECTSETTINGS). The approach speed is entered with its unit as in the road restraint check
//! and the band is resolved from it by `projset::resolve` against the visibility tables' bands; the
//! form shows the band and the rule's sentence and lets the engineer override the band for the
//! check. The override and the sentence are recorded on the sign ([`KEY_BAND`]) and the sentence
//! heads the calculation lines. With no approach speed entered the band is picked by hand, as before.

use std::collections::HashMap;

use afrobim_alignment::TrafficSide;
use afrobim_draw::Primitive;
use afrobim_kernel::{Document, Element, ElementId};
use afrobim_visibility::sources::{SpeedBandMph, UkClearanceCase, KPH_PER_MPH, UG_EYE_HEIGHT, UK_EYE_HEIGHT};
use afrobim_visibility::sweep::{LateralObstruction, ObstructionSide, TravelDirection};
use afrobim_visibility::{
    analyse_sign, derived_x_height, plan_envelope, run_all, scene_from_document, sight_line_profile, EdgeOffsets, EyeOffset, ObstacleScene, RoadEdges, RoadRun, RunOptions, SceneOptions, SignAnalysis, SignContext, Standards,
    Status, UkSignRule, VisibilityDrawing, VisibilityInputs, KEY,
};
use egui::{pos2, Align2, Color32, FontId, Pos2, RichText, Shape, Stroke, Ui};

use crate::app::AfroBimApp;
use crate::check_inputs;

/// The property that records the driver's eye height was confirmed by the engineer.
pub const KEY_ACCEPTED: &str = "check.visibility.accepted";
/// The property that records how the speed band was settled: the rule's sentence, an override, an answer.
pub const KEY_BAND: &str = "check.visibility.band";

/// How a stored speed band was settled.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BandRecord {
    pub overridden: bool,
    pub answer: Option<bool>,
    /// The sentence for the calculation lines.
    pub note: String,
}

/// The line that heads the calculation of a sign whose band was settled by the project's rule or
/// by the engineer, when one is recorded.
pub fn band_line(doc: &Document, id: ElementId) -> Option<String> {
    let r: BandRecord = check_inputs::load(doc, id, KEY_BAND)?;
    (!r.note.is_empty()).then(|| format!("Speed band: {}", r.note))
}
#[allow(non_snake_case)]
fn GOOD() -> egui::Color32 {
    crate::theme::success()
}
#[allow(non_snake_case)]
fn BAD() -> egui::Color32 {
    crate::theme::danger()
}
#[allow(non_snake_case)]
fn WARN() -> egui::Color32 {
    crate::theme::warn()
}
#[allow(non_snake_case)]
fn NEED() -> egui::Color32 {
    crate::theme::accent_text()
}

pub fn status_colour(s: Status) -> Color32 {
    match s {
        Status::Ok => GOOD(),
        Status::Fails => BAD(),
        Status::Differs => WARN(),
        Status::InputNeeded => NEED(),
        Status::Information => crate::theme::muted(),
    }
}

/// True when the engineer confirmed the engine's default eye height for this sign.
pub fn eye_height_accepted(doc: &Document, id: ElementId) -> bool {
    doc.find(id).and_then(|e| e.properties.get(KEY_ACCEPTED)).is_some_and(|v| v.contains("eye_height_m"))
}

fn set_accepted(doc: &mut Document, id: ElementId, accepted: bool) {
    if let Some(e) = doc.find_mut(id) {
        if accepted {
            e.properties.insert(KEY_ACCEPTED.to_string(), "[\"eye_height_m\"]".to_string());
        } else {
            e.properties.remove(KEY_ACCEPTED);
        }
    }
}

/// What a run needs besides the document: the obstruction scene and the legends' x-heights.
pub struct VisSource {
    pub scene: ObstacleScene,
    pub x_heights: HashMap<ElementId, f64>,
}

/// Builds the obstruction scene of the document from the solids the modules generated.
pub fn source_for(doc: &Document, solids_of: &dyn Fn(&Element) -> Vec<afrobim_draw::Mesh3>, x_heights: HashMap<ElementId, f64>) -> VisSource {
    VisSource { scene: scene_from_document(doc, solids_of, &SceneOptions::default()), x_heights }
}

/// What a run on a worker thread needs, taken on the UI thread: a copy of the document, the
/// solids the modules generated for each element and the legends' x-heights. The obstruction scene
/// is built from them inside the worker, because that is the slow part, and the engine's own
/// types are built there too.
pub struct VisInput {
    pub doc: Document,
    pub solids: HashMap<ElementId, Vec<afrobim_draw::Mesh3>>,
    pub x_heights: HashMap<ElementId, f64>,
    pub only: Option<Vec<ElementId>>,
}

/// What a run on a worker thread hands back.
pub struct VisOut {
    pub run: RoadRun,
    pub scene: ObstacleScene,
}

impl VisInput {
    /// Builds the scene and runs the analysis: the same two steps as [`source_for`] and [`run`].
    pub fn compute(self) -> VisOut {
        let solids = self.solids;
        let src = source_for(&self.doc, &|e: &Element| solids.get(&e.id).cloned().unwrap_or_default(), self.x_heights);
        let run = run(&self.doc, &src, self.only);
        VisOut { run, scene: src.scene }
    }
}

/// Starts [`VisInput::compute`] on a worker thread, to be applied only while `doc` is unchanged.
pub fn spawn_run(doc: &Document, input: VisInput) -> crate::background::Job<VisOut> {
    crate::background::Job::spawn("Sign visibility", crate::background::fingerprint(doc), move || input.compute())
}

/// The analysis of one sign with the inputs stored on it.
pub fn analyse(doc: &Document, src: &VisSource, id: ElementId) -> SignAnalysis {
    let x_height_of = |e: &Element| src.x_heights.get(&e.id).copied();
    let ctx = SignContext { doc, scene: Some(&src.scene), x_height_of: &x_height_of };
    analyse_sign(&ctx, id, None)
}

/// The analysis of the placed signs (all, or those named) with the inputs stored on each.
pub fn run(doc: &Document, src: &VisSource, only: Option<Vec<ElementId>>) -> RoadRun {
    let x_height_of = |e: &Element| src.x_heights.get(&e.id).copied();
    let ctx = SignContext { doc, scene: Some(&src.scene), x_height_of: &x_height_of };
    run_all(&ctx, &RunOptions { default_inputs: None, only })
}

/// Whether any sign has stored visibility inputs (so a model-wide check needs the scene).
pub fn any_stored(doc: &Document) -> bool {
    doc.elements().iter().any(|e| e.properties.contains_key(KEY))
}

/// The status line of a run: the number of signs and how many are in each state.
pub fn summary_line(run: &RoadRun) -> String {
    let (mut ok, mut input, mut fails, mut differs) = (0, 0, 0, 0);
    for a in &run.analyses {
        match a.overall() {
            Status::Ok => ok += 1,
            Status::InputNeeded => input += 1,
            Status::Fails => fails += 1,
            Status::Differs => differs += 1,
            Status::Information => {}
        }
    }
    format!("{} sign(s): {ok} OK, {fails} fail, {differs} differ, {input} need input", run.analyses.len())
}

// ─── the form ───────────────────────────────────────────────────────────────────────────

/// A lateral obstruction as typed.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct LateralRow {
    pub right: bool,
    pub offset: String,
    pub top: String,
    pub label: String,
}

/// The dialog's entries, as typed. Starts empty; the eye height starts at the engine's default,
/// marked "to confirm".
#[derive(Clone, PartialEq, Debug)]
pub struct VisForm {
    /// The sign, or `None` for every placed sign.
    pub element: Option<ElementId>,
    /// Store the entries on every placed sign, not only on `element`.
    pub apply_all: bool,
    pub eye_mode: u8,
    pub eye_text: String,
    pub eye_height: String,
    pub eye_height_accepted: bool,
    pub traffic: u8,
    pub direction: u8,
    pub speed: String,
    pub speed_mph: bool,
    pub band: Option<SpeedBandMph>,
    /// The engineer chose the band by hand instead of taking the one the project's rule gives.
    pub band_override: bool,
    /// The answer to an "ask each time" rule.
    pub band_answer: Option<bool>,
    pub large: bool,
    pub rule: u8,
    pub terminal_approach: String,
    pub terminal_sign: String,
    pub clearance: UkClearanceCase,
    pub mounting: String,
    pub over_footway: bool,
    pub overhead: bool,
    pub x_height: String,
    pub words: String,
    pub hazard_on: bool,
    pub hazard_internal: f64,
    pub lateral: Vec<LateralRow>,
    pub edges: [String; 6],
    pub kerbed: bool,
    pub uk: bool,
    pub uganda: bool,
    pub start: String,
    pub step: String,
    pub sample: String,
    pub tolerance: String,
    pub grid: String,
    pub error: String,
}

fn n(v: f64) -> String {
    afrobim_sheets::trim_number(v, 6)
}

impl VisForm {
    pub fn new(element: Option<ElementId>) -> VisForm {
        let d = afrobim_visibility::sweep::SweepParams::default();
        VisForm {
            element,
            apply_all: false,
            eye_mode: 0,
            eye_text: String::new(),
            // The engine's default, shown as "to confirm": it is not used until it is accepted.
            eye_height: n(d.eye_height_m),
            eye_height_accepted: false,
            traffic: 0,
            direction: 0,
            speed: String::new(),
            speed_mph: false,
            band: None,
            band_override: false,
            band_answer: None,
            large: false,
            rule: 0,
            terminal_approach: String::new(),
            terminal_sign: String::new(),
            clearance: UkClearanceCase::Normal,
            mounting: String::new(),
            over_footway: false,
            overhead: false,
            x_height: String::new(),
            words: String::new(),
            hazard_on: false,
            hazard_internal: 0.0,
            lateral: Vec::new(),
            edges: Default::default(),
            kerbed: false,
            uk: true,
            uganda: true,
            // Analysis settings of the engine: resolution of the sweep, not standard values.
            start: n(d.start_distance_m),
            step: n(d.step_m),
            sample: n(d.sample_m),
            tolerance: n(d.tolerance_m),
            grid: d.grid.to_string(),
            error: String::new(),
        }
    }

    /// The form for a sign: its stored inputs when it has some.
    pub fn for_sign(doc: &Document, id: ElementId) -> VisForm {
        match afrobim_visibility::load(doc, id) {
            Some(i) => {
                let mut f = VisForm::from_inputs(&i, Some(id));
                f.eye_height_accepted = eye_height_accepted(doc, id);
                if let Some(r) = check_inputs::load::<BandRecord>(doc, id, KEY_BAND) {
                    f.band_override = r.overridden;
                    f.band_answer = r.answer;
                }
                f
            }
            None => VisForm::new(Some(id)),
        }
    }

    pub fn from_inputs(i: &VisibilityInputs, element: Option<ElementId>) -> VisForm {
        let mut f = VisForm::new(element);
        (f.eye_mode, f.eye_text) = match i.eye_offset {
            Some(EyeOffset::Signed(v)) => (1, n(v)),
            Some(EyeOffset::LaneCentre(v)) => (2, n(v)),
            None => (0, String::new()),
        };
        f.eye_height = n(i.sweep.eye_height_m);
        f.traffic = if i.traffic_side == TrafficSide::Left { 1 } else { 2 };
        f.direction = match i.direction {
            None => 0,
            Some(TravelDirection::UpStation) => 1,
            Some(TravelDirection::DownStation) => 2,
        };
        f.speed = i.approach_speed_kph.map(n).unwrap_or_default();
        f.speed_mph = false;
        f.band = i.uk_band;
        f.large = i.uk_large_case;
        match i.uk_rule {
            UkSignRule::Auto => f.rule = 0,
            UkSignRule::Regulatory => f.rule = 1,
            UkSignRule::Warning => f.rule = 2,
            UkSignRule::Directional => f.rule = 3,
            UkSignRule::NotApplicable => f.rule = 4,
            UkSignRule::SpeedLimitTerminal { approach_mph, sign_mph } => {
                f.rule = 5;
                f.terminal_approach = approach_mph.to_string();
                f.terminal_sign = sign_mph.to_string();
            }
        }
        f.clearance = i.uk_clearance_case;
        f.mounting = i.mounting_height_m.map(n).unwrap_or_default();
        f.over_footway = i.over_footway;
        f.overhead = i.overhead;
        f.x_height = i.x_height_mm.map(n).unwrap_or_default();
        f.words = i.words.map(|w| w.to_string()).unwrap_or_default();
        if let Some(h) = i.hazard_station_m {
            f.hazard_on = true;
            f.hazard_internal = h;
        }
        f.lateral = i.lateral.iter().map(|l| LateralRow { right: l.side == ObstructionSide::Right, offset: n(l.offset_m), top: l.top_above_road_m.map(n).unwrap_or_default(), label: l.label.clone() }).collect();
        let e = &i.road_edges;
        let o = |v: Option<f64>| v.map(n).unwrap_or_default();
        f.edges = [o(e.carriageway.left_m), o(e.carriageway.right_m), o(e.traffic_lane.left_m), o(e.traffic_lane.right_m), o(e.shoulder.left_m), o(e.shoulder.right_m)];
        f.kerbed = e.kerbed;
        f.uk = i.standards.uk;
        f.uganda = i.standards.uganda;
        f.start = n(i.sweep.start_distance_m);
        f.step = n(i.sweep.step_m);
        f.sample = n(i.sweep.sample_m);
        f.tolerance = n(i.sweep.tolerance_m);
        f.grid = i.sweep.grid.to_string();
        f
    }

    /// The approach speed as typed, when it is a number above zero.
    pub fn speed_value(&self) -> Option<f64> {
        self.speed.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0)
    }

    /// What the project's rule gives for the approach speed, for the form and for the store.
    pub fn resolved(&self, doc: &Document) -> Option<afrobim_kernel::speed_band::Resolved> {
        let v = self.speed_value()?;
        Some(crate::projset::resolve_answered(doc, v, self.speed_mph, &crate::projset::visibility_thresholds(), self.band_answer))
    }

    /// Settles the band: the rule's band from the approach speed unless the engineer overrode it,
    /// and the record to keep. Fails when the rule still has a question for the engineer.
    pub fn settle_band(&mut self, doc: &Document) -> Result<BandRecord, String> {
        let Some(r) = self.resolved(doc) else {
            return Ok(BandRecord::default());
        };
        if r.ask.is_some() {
            return Err(format!("The project's speed band rule asks each time: {} Answer in the form.", r.band_note));
        }
        let by_rule = SpeedBandMph::from_mph(r.mph_used);
        if self.band_override {
            let chosen = self.band.ok_or("Choose the band you are overriding with, or untick the override.")?;
            let note = format!("{}; resolved band {}; the user overrode it for this check and used {}", r.band_note, by_rule.label(), chosen.label());
            Ok(BandRecord { overridden: true, answer: self.band_answer, note })
        } else {
            self.band = Some(by_rule);
            Ok(BandRecord { overridden: false, answer: self.band_answer, note: format!("{}; band {}", r.band_note, by_rule.label()) })
        }
    }

    /// The engine's inputs, or what is wrong. The eye height must have been accepted.
    pub fn inputs(&self) -> Result<VisibilityInputs, String> {
        let num = |label: &str, t: &str| check_inputs::number(label, t);
        let pos = |label: &str, t: &str| -> Result<f64, String> {
            let v = check_inputs::required(label, t)?;
            if v > 0.0 {
                Ok(v)
            } else {
                Err(format!("{label} must be above zero"))
            }
        };
        if !self.eye_height_accepted {
            return Err(format!("The driver's eye height {} m is a default of the engine that you have not confirmed. Tick 'accept' beside it, or enter your own value and tick.", self.eye_height.trim()));
        }
        let mut i = VisibilityInputs::default();
        i.sweep.eye_height_m = pos("Eye height", &self.eye_height)?;
        i.sweep.start_distance_m = pos("Sweep start distance", &self.start)?;
        i.sweep.step_m = pos("Sweep step", &self.step)?;
        i.sweep.sample_m = pos("Road sample spacing", &self.sample)?;
        i.sweep.tolerance_m = pos("Tolerance", &self.tolerance)?;
        i.sweep.grid = pos("Grid", &self.grid)? as usize;
        i.eye_offset = match (self.eye_mode, num("Driver's lateral position", &self.eye_text)?) {
            (0, _) | (_, None) => None,
            (1, Some(v)) => Some(EyeOffset::Signed(v)),
            (_, Some(v)) => Some(EyeOffset::LaneCentre(v)),
        };
        i.traffic_side = match self.traffic {
            1 => TrafficSide::Left,
            2 => TrafficSide::Right,
            _ => return Err("Choose the side the traffic keeps to.".into()),
        };
        i.direction = match self.direction {
            1 => Some(TravelDirection::UpStation),
            2 => Some(TravelDirection::DownStation),
            _ => None,
        };
        i.approach_speed_kph = num("Approach speed", &self.speed)?.map(|v| if self.speed_mph { v * KPH_PER_MPH } else { v });
        i.uk_band = self.band;
        i.uk_large_case = self.large;
        i.uk_rule = match self.rule {
            1 => UkSignRule::Regulatory,
            2 => UkSignRule::Warning,
            3 => UkSignRule::Directional,
            4 => UkSignRule::NotApplicable,
            5 => UkSignRule::SpeedLimitTerminal {
                approach_mph: check_inputs::required("Approach speed limit (mph)", &self.terminal_approach)? as u32,
                sign_mph: check_inputs::required("Speed limit on the sign (mph)", &self.terminal_sign)? as u32,
            },
            _ => UkSignRule::Auto,
        };
        i.uk_clearance_case = self.clearance;
        i.mounting_height_m = num("Mounting height", &self.mounting)?;
        i.over_footway = self.over_footway;
        i.overhead = self.overhead;
        i.x_height_mm = num("x-height", &self.x_height)?;
        i.words = num("Words", &self.words)?.map(|w| w as u32);
        i.hazard_station_m = self.hazard_on.then_some(self.hazard_internal);
        for (k, r) in self.lateral.iter().enumerate() {
            if r.offset.trim().is_empty() && r.label.trim().is_empty() {
                continue;
            }
            let offset = check_inputs::required(&format!("Offset of obstruction {}", k + 1), &r.offset)?;
            let mut l = LateralObstruction::new(if r.right { ObstructionSide::Right } else { ObstructionSide::Left }, offset, r.label.trim());
            l.top_above_road_m = num("Height of obstruction", &r.top)?;
            i.lateral.push(l);
        }
        let e = |k: usize, label: &str| num(label, &self.edges[k]);
        i.road_edges = RoadEdges {
            carriageway: EdgeOffsets { left_m: e(0, "Carriageway edge, left")?, right_m: e(1, "Carriageway edge, right")? },
            traffic_lane: EdgeOffsets { left_m: e(2, "Traffic lane edge, left")?, right_m: e(3, "Traffic lane edge, right")? },
            shoulder: EdgeOffsets { left_m: e(4, "Shoulder edge, left")?, right_m: e(5, "Shoulder edge, right")? },
            kerbed: self.kerbed,
        };
        if !self.uk && !self.uganda {
            return Err("Tick at least one manual to check against.".into());
        }
        i.standards = Standards { uk: self.uk, uganda: self.uganda };
        Ok(i)
    }
}

/// Stores the form's inputs on the sign (or on every placed sign when the form is for all).
/// Returns how many signs were written.
pub fn store_form(doc: &mut Document, form: &VisForm) -> Result<usize, String> {
    let mut form = form.clone();
    let record = form.settle_band(doc)?;
    let inputs = form.inputs()?;
    let ids: Vec<ElementId> = match form.element.filter(|_| !form.apply_all) {
        Some(id) => vec![id],
        None => doc.elements().iter().filter(|e| e.category == afrobim_visibility::SIGN_CATEGORY && e.site.is_some()).map(|e| e.id).collect(),
    };
    if ids.is_empty() {
        return Err("No sign is placed on an alignment, so there is nothing to store the inputs on.".into());
    }
    for id in &ids {
        afrobim_visibility::save(doc, *id, &inputs);
        set_accepted(doc, *id, true);
        check_inputs::save(doc, *id, KEY_BAND, &record);
    }
    Ok(ids.len())
}

// ─── drawings ───────────────────────────────────────────────────────────────────────────

fn paint_drawing(ui: &mut Ui, d: &VisibilityDrawing, width: f32) {
    let ext = d.drawing.extents;
    let (ew, eh) = (ext.width().max(1.0) as f32, ext.height().max(1.0) as f32);
    let scale = (width / ew).min(520.0 / eh);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ew * scale + 24.0, eh * scale + 24.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0xF6, 0xF5, 0xF0));
    let map = |x: f64, y: f64| pos2(rect.left() + 12.0 + (x - ext.min.x) as f32 * scale, rect.bottom() - 12.0 - (y - ext.min.y) as f32 * scale);
    let rgb = |c: Option<afrobim_draw::Colour>| {
        let [r, g, b] = c.map(|c| c.display_rgb()).unwrap_or([0x10, 0x10, 0x10]);
        Color32::from_rgb(r, g, b)
    };
    for prim in &d.drawing.primitives {
        match prim {
            Primitive::Fill { outline, colour } => {
                let pts: Vec<Pos2> = outline.iter().map(|p| map(p.x, p.y)).collect();
                if pts.len() >= 3 {
                    painter.add(Shape::convex_polygon(pts, rgb(Some(*colour)), Stroke::NONE));
                }
            }
            Primitive::Line { from, to, width, colour } => {
                painter.line_segment([map(from.x, from.y), map(to.x, to.y)], Stroke::new((*width as f32 * scale).max(1.0), rgb(*colour)));
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                let steps = 24;
                let pts: Vec<Pos2> = (0..=steps)
                    .map(|k| {
                        let a = (start_deg + (end_deg - start_deg) * k as f64 / steps as f64).to_radians();
                        map(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                    })
                    .collect();
                painter.add(Shape::line(pts, Stroke::new(1.0_f32, rgb(*colour))));
            }
            _ => {}
        }
    }
    // Labels are placed where the engine put them; one that would print over an earlier one is left out.
    let mut taken: Vec<egui::Rect> = Vec::new();
    let mut order: Vec<&afrobim_visibility::DrawingLabel> = d.labels.iter().collect();
    order.sort_by_key(|l| match l.kind {
        afrobim_visibility::LabelKind::Title => 0,
        afrobim_visibility::LabelKind::Limit => 1,
        afrobim_visibility::LabelKind::Obstruction => 2,
        afrobim_visibility::LabelKind::Sign => 3,
        _ => 4,
    });
    for l in order {
        let at = map(l.at.x, l.at.y);
        let galley = painter.layout_no_wrap(l.text.clone(), FontId::proportional(10.5), Color32::from_rgb(0x22, 0x22, 0x22));
        let r = egui::Rect::from_min_size(pos2(at.x, at.y - galley.size().y / 2.0), galley.size()).expand(1.0);
        if taken.iter().any(|t| t.intersects(r)) {
            continue;
        }
        taken.push(r);
        painter.galley(r.min + egui::vec2(1.0, 1.0), galley, Color32::from_rgb(0x22, 0x22, 0x22));
    }
}

/// The plan envelope and the sight-line profile of an analysis as drawings, or why not.
pub fn drawings_of(a: &SignAnalysis, scene: Option<&ObstacleScene>, exaggeration: f64) -> (Option<VisibilityDrawing>, Option<VisibilityDrawing>) {
    (plan_envelope(a, scene), sight_line_profile(a, scene, exaggeration))
}

// ─── the app side ───────────────────────────────────────────────────────────────────────

/// What the visibility windows keep.
#[derive(Default)]
pub struct VisUi {
    pub form: Option<VisForm>,
    pub results_open: bool,
    pub drawings_open: bool,
    pub run: Option<RoadRun>,
    pub scene: Option<ObstacleScene>,
    pub selected: Option<ElementId>,
    pub overlay: bool,
    pub exaggeration: String,
}

impl VisUi {
    fn analysis(&self) -> Option<&SignAnalysis> {
        let id = self.selected?;
        self.run.as_ref()?.analyses.iter().find(|a| a.element == id)
    }

    /// The limiting sight line and its obstruction of the selected sign, drawn into the 3D view
    /// (site metres to world millimetres).
    pub fn paint_overlay(&self, painter: &egui::Painter, cam: &crate::view3d::Camera, rect: egui::Rect) {
        if !self.overlay {
            return;
        }
        let Some(a) = self.analysis() else { return };
        let (Some(sweep), Some(face)) = (&a.sweep, a.face) else { return };
        let mm = |p: [f64; 3]| [p[0] * 1000.0, p[1] * 1000.0, p[2] * 1000.0];
        let centre = mm(face.centre());
        let seg = |from: [f64; 3], to: [f64; 3], stroke: Stroke| {
            if let (Some((p, _)), Some((q, _))) = (cam.project(from, rect), cam.project(to, rect)) {
                painter.line_segment([p, q], stroke);
            }
        };
        let green = Stroke::new(2.5_f32, GOOD());
        let red = Stroke::new(2.5_f32, BAD());
        let limit = &sweep.full;
        match (&limit.at_limit, &limit.beyond) {
            (Some(last), beyond) => {
                seg(mm(last.eye), centre, green);
                if let Some((p, _)) = cam.project(mm(last.eye), rect) {
                    painter.text(p + egui::vec2(6.0, -6.0), Align2::LEFT_BOTTOM, format!("limit: {:.0} m before the sign", limit.distance_m), FontId::proportional(12.0), GOOD());
                }
                if let Some(b) = beyond {
                    seg(mm(b.eye), centre, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xEF, 0x6C, 0x63, 120)));
                    if let Some(hit) = b.blockers.first() {
                        seg(mm(b.eye), mm(hit.point), red);
                        if let Some((p, _)) = cam.project(mm(hit.point), rect) {
                            painter.circle_stroke(p, 8.0, red);
                            painter.text(p + egui::vec2(10.0, 4.0), Align2::LEFT_TOP, format!("obstruction: {}", hit.label), FontId::proportional(12.0), BAD());
                        }
                    }
                }
            }
            (None, _) => {}
        }
        if limit.reaches_start {
            if let Some(far) = sweep.samples.last() {
                seg(mm(far.eye), centre, green);
                if let Some((p, _)) = cam.project(mm(far.eye), rect) {
                    painter.text(p, Align2::LEFT_BOTTOM, format!("clear to the start of the sweep ({:.0} m)", far.distance_m), FontId::proportional(12.0), GOOD());
                }
            }
        }
    }
}

impl AfroBimApp {
    /// The visibility source of the current model: the obstruction scene from the generated solids
    /// and the legends' x-heights.
    pub(crate) fn vis_source(&mut self) -> VisSource {
        self.sync_generated();
        let solids = |e: &Element| self.cache.get(&e.id).and_then(|c| c.generated.as_ref().ok()).map(|g| g.solids.clone()).unwrap_or_default();
        let x: HashMap<ElementId, f64> = self.doc.elements().iter().filter_map(|e| self.cache.get(&e.id).and_then(|c| c.generated.as_ref().ok()).and_then(|g| derived_x_height(&g.derived)).map(|v| (e.id, v))).collect();
        source_for(&self.doc, &solids, x)
    }

    /// Opens the inputs dialog for the selected sign, or for all signs when none is selected.
    pub(crate) fn vis_open_inputs(&mut self, all: bool) {
        let sign = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == afrobim_visibility::SIGN_CATEGORY));
        self.vis.form = Some(match (all, sign) {
            (false, Some(id)) => VisForm::for_sign(&self.doc, id),
            _ => VisForm::new(None),
        });
    }

    /// What a worker needs for a run (see [`VisInput`]): the generated solids and x-heights are
    /// read from the cache here, the scene is built later in the worker.
    pub(crate) fn vis_input(&mut self, only: Option<Vec<ElementId>>) -> VisInput {
        self.sync_generated();
        let generated = |id: &ElementId| self.cache.get(id).and_then(|c| c.generated.as_ref().ok());
        let solids = self.doc.elements().iter().filter_map(|e| generated(&e.id).filter(|g| !g.solids.is_empty()).map(|g| (e.id, g.solids.clone()))).collect();
        let x_heights = self.doc.elements().iter().filter_map(|e| generated(&e.id).and_then(|g| derived_x_height(&g.derived)).map(|v| (e.id, v))).collect();
        VisInput { doc: self.doc.clone(), solids, x_heights, only }
    }

    /// Runs the analysis for the signs named (all placed signs for `None`) on the UI thread and
    /// opens the results: for headless runs and tests.
    pub(crate) fn vis_run(&mut self, only: Option<Vec<ElementId>>) {
        let out = self.vis_input(only).compute();
        self.apply_vis_out(out);
    }

    /// As [`AfroBimApp::vis_run`] on a worker thread, with a progress window and Cancel. The result
    /// is applied only if the document has not changed since the run started.
    pub(crate) fn vis_run_bg(&mut self, only: Option<Vec<ElementId>>) {
        if self.bg.vis.is_some() {
            self.status = "A sign visibility run is already in progress.".into();
            return;
        }
        let input = self.vis_input(only);
        self.bg.vis = Some(spawn_run(&self.doc, input));
        self.status = "Sign visibility started on a worker thread; the model stays editable, but an edit discards the result.".into();
    }

    /// Takes a finished run in: the summary, the results window and the 3D overlay.
    pub(crate) fn apply_vis_out(&mut self, out: VisOut) {
        let VisOut { run, scene } = out;
        self.status = format!("Sign visibility: {}. Analysis aid only: the engineer decides.", summary_line(&run));
        self.vis.selected = run.analyses.iter().find(|a| Some(a.element) == self.vis.selected).map(|a| a.element).or_else(|| run.analyses.first().map(|a| a.element));
        self.vis.run = Some(run);
        self.vis.scene = Some(scene);
        self.vis.results_open = true;
        self.vis.overlay = true;
        match self.options.vis_view.clone().as_deref() {
            Some("drawings") => {
                self.vis.results_open = false;
                self.vis.drawings_open = true;
            }
            Some("overlay") => {
                self.vis.results_open = false;
                if let Some(id) = self.vis.selected {
                    self.zoom_to_element(id);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn vis_export_csv(&mut self) {
        let Some(run) = &self.vis.run else {
            self.status = "Run the visibility check first.".into();
            return;
        };
        let (summary, lines) = (run.summary_csv(), run.detail_csv());
        let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("sign-visibility.csv").save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("csv");
        }
        let lines_path = path.with_file_name(format!("{}-lines.csv", path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "sign-visibility".into())));
        self.status = match std::fs::write(&path, summary).and_then(|()| std::fs::write(&lines_path, lines)) {
            Ok(()) => format!("Wrote {} (one row per sign) and {} (one row per check line).", path.display(), lines_path.display()),
            Err(e) => format!("Could not write the visibility CSV: {e}"),
        };
    }

    pub(crate) fn visibility_windows(&mut self, ctx: &egui::Context) {
        self.vis_form_window(ctx);
        if self.vis.results_open {
            self.vis_results_window(ctx);
        }
        if self.vis.drawings_open {
            self.vis_drawings_window(ctx);
        }
    }

    fn vis_form_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.vis.form.take() else { return };
        let title = match f.element.and_then(|id| self.doc.find(id)) {
            Some(e) => format!("Visibility inputs for {}", e.name),
            None => "Visibility inputs for all placed signs".to_string(),
        };
        let alignment = f.element.and_then(|id| self.doc.find(id)).and_then(|e| e.site.as_ref().and_then(|s| s.alignment.clone())).and_then(|name| crate::alignment_glue::alignments(&self.doc).into_iter().find(|a| a.1 == name).map(|a| a.0));
        let (mut keep, mut save, mut run) = (true, false, false);
        let dlg = crate::ui_kit::Dialog::titled(title).subtitle("The inputs of the sight-distance analysis").width(640.0).show(ctx, |ui| {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "An analysis aid: the result states no compliance. Every field starts empty except the one marked 'to confirm', which is the engine's own default and is not used until you accept it. The UK band of a speed is your decision.");
            {
                crate::ui_kit::heading(ui, "Driver and traffic");
                if f.element.is_some() {
                    crate::ui_kit::toggle(ui, &mut f.apply_all, "Store these entries on every placed sign, not only this one");
                }
                crate::ui_kit::grid(ui, "vis-form", |ui| {
                    ui.label("Driver's lateral position");
                    ui.vertical(|ui| {
                        crate::ui_kit::segmented_of(ui, &mut f.eye_mode, &[(1, "Signed offset (right positive)"), (2, "Distance to lane centre")]);
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut f.eye_text).desired_width(70.0).horizontal_align(egui::Align::RIGHT).hint_text("m"));
                            ui.label("m");
                        });
                    });
                    ui.end_row();
                    ui.label("Driver's eye height (m)");
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut f.eye_height).desired_width(70.0));
                            crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "to confirm");
                            ui.checkbox(&mut f.eye_height_accepted, "accept this value");
                        });
                        crate::ui_kit::note(ui, format!("Engine default, stated in held sources for other criteria, not for sign visibility: {UK_EYE_HEIGHT}; {UG_EYE_HEIGHT}."));
                    });
                    ui.end_row();
                    ui.label("Traffic keeps to");
                    crate::ui_kit::segmented_of(ui, &mut f.traffic, &[(1, "The left"), (2, "The right")]);
                    ui.end_row();
                    ui.label("Direction of travel");
                    egui::ComboBox::from_id_salt("vis-dir").selected_text(["from the sign's rotation (the traffic it serves)", "up station", "down station"][f.direction as usize % 3]).show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.direction, 0, "from the sign's rotation (the traffic it serves)");
                        ui.selectable_value(&mut f.direction, 1, "up station");
                        ui.selectable_value(&mut f.direction, 2, "down station");
                    });
                    ui.end_row();
                    ui.label("Approach speed (85th percentile)");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut f.speed).desired_width(70.0).hint_text("blank: the sign's speed class"));
                        crate::ui_kit::segmented_of(ui, &mut f.speed_mph, &[(false, "km/h"), (true, "mph")]);
                    });
                    ui.end_row();
                    ui.label("UK speed band");
                    ui.vertical(|ui| {
                        let resolved = f.resolved(&self.doc);
                        match &resolved {
                            Some(r) => {
                                let by_rule = SpeedBandMph::from_mph(r.mph_used);
                                crate::ui_kit::heading(ui, &format!("From the approach speed: {}", by_rule.label()));
                                ui.label(RichText::new(&r.band_note).small().weak());
                                if let Some(ask) = r.ask {
                                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("The project rule asks each time: treat {:.2} mph as {} mph?", ask.exact_mph, ask.band_mph));
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(&mut f.band_answer, Some(true), "Yes, take the band");
                                        ui.selectable_value(&mut f.band_answer, Some(false), "No, keep the exact value");
                                    });
                                }
                                ui.checkbox(&mut f.band_override, "Override the band for this check (recorded with the inputs)");
                                if f.band_override {
                                    egui::ComboBox::from_id_salt("vis-band").selected_text(f.band.map(|b| b.label()).unwrap_or("choose a band")).show_ui(ui, |ui| {
                                        for b in SpeedBandMph::ALL {
                                            ui.selectable_value(&mut f.band, Some(b), b.label());
                                        }
                                    });
                                }
                            }
                            None => {
                                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Enter the approach speed to have the band resolved by the project's rule, or choose one:");
                                egui::ComboBox::from_id_salt("vis-band").selected_text(f.band.map(|b| b.label()).unwrap_or("not chosen")).show_ui(ui, |ui| {
                                    ui.selectable_value(&mut f.band, None, "not chosen");
                                    for b in SpeedBandMph::ALL {
                                        ui.selectable_value(&mut f.band, Some(b), b.label());
                                    }
                                });
                            }
                        }
                        ui.checkbox(&mut f.large, "the larger figures for 70 mph dual carriageways and motorways");
                    });
                    ui.end_row();
                    ui.label("UK provision");
                    egui::ComboBox::from_id_salt("vis-rule").selected_text(["from the sign's class", "regulatory (Chapter 3)", "warning (Chapter 4)", "lettered (x-height method)", "none applies", "terminal speed limit (Table 14-3)"][f.rule as usize % 6]).show_ui(ui, |ui| {
                        for (i, t) in ["from the sign's class", "regulatory (Chapter 3)", "warning (Chapter 4)", "lettered (x-height method)", "none applies", "terminal speed limit (Table 14-3)"].iter().enumerate() {
                            ui.selectable_value(&mut f.rule, i as u8, *t);
                        }
                    });
                    ui.end_row();
                    if f.rule == 5 {
                        ui.label("Terminal limits (mph)");
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut f.terminal_approach).desired_width(60.0).hint_text("approach"));
                            ui.add(egui::TextEdit::singleline(&mut f.terminal_sign).desired_width(60.0).hint_text("on sign"));
                        });
                        ui.end_row();
                    }
                    ui.label("UK set-back case");
                    egui::ComboBox::from_id_salt("vis-clear").selected_text(format!("{:?}", f.clearance)).show_ui(ui, |ui| {
                        for c in [UkClearanceCase::Normal, UkClearanceCase::SevereCamberOrCentralReserve, UkClearanceCase::HighSpeedDual, UkClearanceCase::HardenedVerge] {
                            ui.selectable_value(&mut f.clearance, c, format!("{c:?}"));
                        }
                    });
                    ui.end_row();
                    for (label, field, hint) in [("Mounting height of the lower edge (m)", &mut f.mounting, "blank: the post's"), ("x-height of the legend (mm)", &mut f.x_height, "blank: the module's"), ("Words or destinations", &mut f.words, "blank: counted")] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(field).desired_width(160.0).hint_text(hint));
                        ui.end_row();
                    }
                    ui.label("Over a footway, overhead");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut f.over_footway, "over or alongside a footway");
                        ui.checkbox(&mut f.overhead, "overhead (gantry or mast arm)");
                    });
                    ui.end_row();
                    ui.label("Hazard or junction");
                    ui.vertical(|ui| {
                        ui.checkbox(&mut f.hazard_on, "the sign refers to a hazard or junction at a station");
                        if f.hazard_on {
                            crate::station_input::station_field(ui, &self.doc, alignment, "vis-hazard", &mut f.hazard_internal);
                        }
                    });
                    ui.end_row();
                    ui.label("Manuals");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut f.uk, "UK Traffic Signs Manual");
                        ui.checkbox(&mut f.uganda, "Uganda Traffic Signs Manual");
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                crate::ui_kit::heading(ui, "Road edges at the sign: distances from the centreline (m), for the clearance lines");
                egui::Grid::new("vis-edges").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label("");
                    ui.label("left");
                    ui.label("right");
                    ui.end_row();
                    for (k, name) in ["Carriageway edge or kerb face", "Traffic lane edge", "Shoulder edge"].iter().enumerate() {
                        ui.label(*name);
                        ui.add(egui::TextEdit::singleline(&mut f.edges[2 * k]).desired_width(70.0));
                        ui.add(egui::TextEdit::singleline(&mut f.edges[2 * k + 1]).desired_width(70.0));
                        ui.end_row();
                    }
                });
                crate::ui_kit::toggle(ui, &mut f.kerbed, "The road is kerbed at the sign");
                crate::ui_kit::heading(ui, "Walls or banks beside the road (the inside of a curve): nothing beyond the offset can be seen");
                let mut remove = None;
                for (k, r) in f.lateral.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        crate::ui_kit::segmented_of(ui, &mut r.right, &[(false, "Left"), (true, "Right")]);
                        ui.add(egui::TextEdit::singleline(&mut r.offset).desired_width(70.0).hint_text("offset m"));
                        ui.add(egui::TextEdit::singleline(&mut r.top).desired_width(70.0).hint_text("top m (opt)"));
                        ui.add(egui::TextEdit::singleline(&mut r.label).desired_width(160.0).hint_text("what it is"));
                        if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                            remove = Some(k);
                        }
                    });
                }
                if let Some(k) = remove {
                    f.lateral.remove(k);
                }
                if ui.button("Add an obstruction").clicked() {
                    f.lateral.push(LateralRow::default());
                }
                ui.add_space(4.0);
                crate::ui_kit::heading(ui, "Analysis settings (resolution of the sweep, not standard values; the engine's own starting values)");
                crate::ui_kit::grid(ui, "vis-analysis", |ui| {
                    for (label, field) in [("Sweep length before the sign (m)", &mut f.start), ("Eye step (m)", &mut f.step), ("Road sample spacing (m)", &mut f.sample), ("Bisection tolerance (m)", &mut f.tolerance), ("Sight-line grid (points per side)", &mut f.grid)] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(field).desired_width(70.0));
                        ui.end_row();
                    }
                });
            }
            if !f.error.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &f.error);
            }
        }, |ui| crate::ui_kit::Footer::new("Store and run").secondary(&["Store"]).show(ui));
        if dlg.closed {
            keep = false;
        }
        if let Some((_, out)) = &dlg.inner {
            if out.cancel {
                keep = false;
            }
            if out.primary {
                save = true;
                run = true;
            }
            if out.secondary == Some(0) {
                save = true;
            }
        }
        if save {
            match store_form(&mut self.doc, &f) {
                Ok(count) => {
                    self.status = format!("Visibility inputs stored on {count} sign(s) under check.visibility.");
                    self.schedule_commit_now();
                    if run {
                        let only = f.element.filter(|_| !f.apply_all).map(|id| vec![id]);
                        self.vis_run_bg(only);
                    }
                    return;
                }
                Err(e) => {
                    f.error = e;
                    self.vis.form = Some(f);
                }
            }
        } else if keep {
            self.vis.form = Some(f);
        }
    }

    fn vis_results_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut csv, mut rerun, mut edit, mut draw) = (false, false, false, false);
        let mut copy: Option<String> = None;
        let mut pick: Option<ElementId> = None;
        let dlg = crate::ui_kit::Dialog::titled("Sign visibility results").subtitle("What the sight lines found, with no compliance stated").width(920.0).show(ctx, |ui| {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "An analysis aid. Each line is OK, FAILS, DIFFERS, INPUT NEEDED or INFORMATION with the provision it rests on; it states no compliance and the engineer decides. The road surface is the centreline level of the profile (no cross-fall); obstruction is geometric only.");
            crate::ui_kit::form(ui, "vis-res-opts", |f| {
                f.row("Limiting sight line in 3D", |ui| crate::ui_kit::toggle(ui, &mut self.vis.overlay, "Draw it in the 3D view"));
            });
            let Some(run) = &self.vis.run else {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "No run yet. Press Run all signs.");
                return;
            };
            crate::ui_kit::heading(ui, "Signs");
            egui::ScrollArea::vertical().id_salt("vis-list").max_height(170.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("vis-rows").num_columns(5).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                    for h in ["Sign", "Station", "Overall", "Clear distance (whole face)", "Limiting obstruction"] {
                        crate::ui_kit::th(ui, &h);
                    }
                    ui.end_row();
                    for a in &run.analyses {
                        let on = self.vis.selected == Some(a.element);
                        if ui.selectable_label(on, &a.mark).clicked() {
                            pick = Some(a.element);
                        }
                        ui.label(a.station_label.clone().unwrap_or_else(|| "not placed".into()));
                        if a.skipped.is_some() {
                            ui.label(RichText::new("not analysed").weak());
                        } else {
                            ui.label(RichText::new(a.overall().label()).color(status_colour(a.overall())));
                        }
                        ui.label(a.clear_full().map(|(d, lower)| format!("{}{d:.1} m", if lower { ">= " } else { "" })).unwrap_or_default());
                        ui.label(a.sweep.as_ref().and_then(|s| s.full.beyond.as_ref()).and_then(|b| b.blockers.first()).map(|b| b.label.clone()).unwrap_or_default());
                        ui.end_row();
                    }
                });
            });
            ui.separator();
            let Some(a) = self.vis.run.as_ref().and_then(|r| r.analyses.iter().find(|a| Some(a.element) == self.vis.selected)) else {
                ui.label(RichText::new("Select a sign.").weak());
                return;
            };
            egui::ScrollArea::vertical().id_salt("vis-lines").auto_shrink([false, false]).show(ui, |ui| {
                if let Some(why) = &a.skipped {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("{} was not analysed: {why}", a.mark));
                }
                if let Some(d) = self.doc.find(a.element).and_then(|e| e.properties.get(crate::demo_glue::KEY_DEMO_VISIBILITY)) {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("DEMO inputs: {d}"));
                }
                if let Some(l) = band_line(&self.doc, a.element) {
                    ui.label(RichText::new(l).small().strong());
                }
                if afrobim_visibility::load(&self.doc, a.element).is_some() && !eye_height_accepted(&self.doc, a.element) {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "The stored inputs use the engine's default eye height, which has not been confirmed for this sign. Open the inputs and accept it.");
                }
                for l in &a.lines {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(l.status.label()).strong().color(status_colour(l.status)));
                        ui.label(RichText::new(&l.title).strong());
                    });
                    ui.label(&l.detail);
                    if !l.citation.is_empty() {
                        crate::ui_kit::note(ui, format!("Source: {}", l.citation));
                    }
                    ui.add_space(3.0);
                }
                if ui.button("Copy the lines").clicked() {
                    copy = Some(a.report().join("\n"));
                }
            });
        }, |ui| crate::ui_kit::Footer::new("Run all signs").secondary(&["Edit inputs", "Plan and profile drawings", "Export CSV..."]).cancel(Some("Close")).show(ui));
        if dlg.closed {
            open = false;
        }
        if let Some((_, out)) = &dlg.inner {
            if out.cancel {
                open = false;
            }
            if out.primary {
                rerun = true;
            }
            match out.secondary {
                Some(0) => edit = true,
                Some(1) => draw = true,
                Some(2) => csv = true,
                _ => {}
            }
        }
        self.vis.results_open = open;
        if let Some(id) = pick {
            self.vis.selected = Some(id);
            self.doc.select(&[id], afrobim_kernel::SelectMode::Replace);
        }
        if let Some(t) = copy {
            ctx.copy_text(t);
        }
        if csv {
            self.vis_export_csv();
        }
        if rerun {
            self.vis_run(None);
        }
        if edit {
            if let Some(id) = self.vis.selected {
                self.vis.form = Some(VisForm::for_sign(&self.doc, id));
            }
        }
        if draw {
            self.vis.drawings_open = true;
        }
    }

    fn vis_drawings_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        if self.vis.exaggeration.is_empty() {
            self.vis.exaggeration = "5".into();
        }
        let exaggeration = self.vis.exaggeration.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
        let drawings = self.vis.analysis().map(|a| drawings_of(a, self.vis.scene.as_ref(), exaggeration.unwrap_or(5.0)));
        let mark = self.vis.analysis().map(|a| a.mark.clone()).unwrap_or_default();
        let tab_id = egui::Id::new("vis-drawings-tab");
        let dlg = crate::ui_kit::Dialog::titled(if mark.is_empty() { "Visibility drawings".to_string() } else { format!("Visibility drawings: {mark}") }).id_title(format!("Visibility drawings: {mark}")).width(900.0).help("The plan shows the eye positions along the approach coloured by what they see, the sight lines at the limit and the obstruction. The profile is the limiting sight line over the road and ground. Add either to a drawing sheet as a Detail sheet in SHEETSET (kinds 'Sign visibility, plan envelope' and 'Sign visibility, sight-line profile').").show_body(ctx, |ui| {
            crate::ui_kit::section(ui, "Drawing", |ui| {
                crate::ui_kit::form(ui, "vis-dr", |fm| {
                    fm.row_note("Profile vertical exaggeration", "The plan is drawn to scale; only the profile is exaggerated", |ui| {
                        crate::ui_kit::num_field(ui, &mut self.vis.exaggeration, ": 1", &crate::ui_kit::NumRule::required().above(0.0));
                    });
                });
            });
            let Some((plan, profile)) = &drawings else {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Select a sign in the results first.");
                return;
            };
            let mut tab: usize = ui.data(|d| d.get_temp(tab_id)).unwrap_or(0);
            crate::ui_kit::tabs(ui, &mut tab, &["Plan envelope", "Sight-line profile"]);
            ui.data_mut(|d| d.insert_temp(tab_id, tab));
            let d = if tab == 0 { plan } else { profile };
            crate::ui_kit::scroll_x(ui, "vis-dr-x", |ui| match d {
                Some(d) => paint_drawing(ui, d, 780.0),
                None => {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "This sign has no sweep (inputs missing or not placed), so there is no drawing.");
                }
            });
        });
        if dlg.closed {
            open = false;
        }
        self.vis.drawings_open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets_glue::test_support::demo_doc;
    use afrobim_kernel::{Pt, SitePlacement};

    /// The corridor test alignment with one sign standing beside it at an internal station.
    fn doc_with_sign() -> (Document, ElementId) {
        let mut doc = demo_doc();
        let (aid, _) = crate::alignment_glue::alignments(&doc).remove(0);
        let a = crate::alignment_glue::alignment_of(&doc, aid).unwrap();
        let name = doc.elements().iter().find(|e| e.id == aid).unwrap().name.clone();
        let station = a.start_station_m + 300.0;
        let (x, y) = a.point_at_offset(station, 6.0).unwrap();
        let id = doc.add("sign", "VS-1", Pt::new(0.0, 0.0), 1000.0, 1000.0, "Signs", serde_json::Value::Null);
        doc.find_mut(id).unwrap().site = Some(SitePlacement { x: x * 1000.0, y: y * 1000.0, z: 0.0, rotation_deg: 0.0, alignment: Some(name), station_m: Some(station), offset_m: Some(6.0) });
        (doc, id)
    }

    #[test]
    fn the_eye_height_is_a_default_to_confirm_and_nothing_is_stored_until_it_is_accepted() {
        let (mut doc, id) = doc_with_sign();
        let mut f = VisForm::new(Some(id));
        // Every other field starts empty, and the one default is the engine's own.
        assert!(f.eye_text.is_empty() && f.speed.is_empty() && f.mounting.is_empty() && f.band.is_none() && f.traffic == 0 && f.lateral.is_empty());
        assert_eq!(f.eye_height, n(afrobim_visibility::sweep::SweepParams::default().eye_height_m));
        assert!(!f.eye_height_accepted);
        f.traffic = 1;
        f.eye_mode = 2;
        f.eye_text = "1.75".into();
        let refused = store_form(&mut doc, &f).unwrap_err();
        assert!(refused.contains("not confirmed"), "{refused}");
        assert!(afrobim_visibility::load(&doc, id).is_none() && !eye_height_accepted(&doc, id));
        f.eye_height_accepted = true;
        // The traffic side has no default: it is a required choice.
        f.traffic = 0;
        assert!(store_form(&mut doc, &f).unwrap_err().contains("side"));
        f.traffic = 1;
        f.speed = "50".into();
        f.speed_mph = true;
        f.band = Some(SpeedBandMph::From41To50);
        f.lateral.push(LateralRow { right: true, offset: "9".into(), top: String::new(), label: "bank".into() });
        assert_eq!(store_form(&mut doc, &f), Ok(1));
        // Stored under check.visibility, and the dialog reads it back as it was entered.
        assert!(doc.find(id).unwrap().properties.contains_key("check.visibility"));
        assert!(eye_height_accepted(&doc, id));
        let back = VisForm::for_sign(&doc, id);
        assert_eq!((back.eye_mode, back.eye_text.as_str(), back.traffic, back.band, back.eye_height_accepted), (2, "1.75", 1, Some(SpeedBandMph::From41To50), true));
        assert_eq!(back.lateral, vec![LateralRow { right: true, offset: "9".into(), top: String::new(), label: "bank".into() }]);
        // 50 mph is stored as km/h through the manual's own factor.
        let stored = afrobim_visibility::load(&doc, id).unwrap();
        assert!((stored.approach_speed_kph.unwrap() - 50.0 * KPH_PER_MPH).abs() < 1e-9);
        assert_eq!(stored.uk_band, Some(SpeedBandMph::From41To50));
    }

    #[test]
    fn the_band_comes_from_the_project_rule_and_an_override_is_recorded() {
        let (mut doc, id) = doc_with_sign();
        let mut f = VisForm::new(Some(id));
        f.eye_height_accepted = true;
        f.traffic = 1;
        f.speed = "80".into();
        f.speed_mph = false;
        assert_eq!(store_form(&mut doc, &f), Ok(1));
        // The default rule resolves 80 km/h to 50 mph, the 41 to 50 mph row, with the sentence.
        assert_eq!(afrobim_visibility::load(&doc, id).unwrap().uk_band, Some(SpeedBandMph::From41To50));
        let line = band_line(&doc, id).unwrap();
        assert!(line.contains("treats it as 50 mph") && line.contains("41 to 50 mph"), "{line}");
        // The engineer overrides the band for this check: the override is stored and recorded.
        f.band_override = true;
        f.band = Some(SpeedBandMph::From51To60);
        assert_eq!(store_form(&mut doc, &f), Ok(1));
        assert_eq!(afrobim_visibility::load(&doc, id).unwrap().uk_band, Some(SpeedBandMph::From51To60));
        let line = band_line(&doc, id).unwrap();
        assert!(line.contains("overrode") && line.contains("51 to 60 mph"), "{line}");
        assert!(VisForm::for_sign(&doc, id).band_override);
        // With "ask each time" nothing is stored until it is answered.
        afrobim_kernel::set_speed_band_rule(&mut doc, &afrobim_kernel::SpeedBandRule::AskEachTime);
        f.band_override = false;
        assert!(store_form(&mut doc, &f).unwrap_err().contains("asks each time"));
        f.band_answer = Some(true);
        assert_eq!(store_form(&mut doc, &f), Ok(1));
        assert_eq!(afrobim_visibility::load(&doc, id).unwrap().uk_band, Some(SpeedBandMph::From41To50));
    }

    #[test]
    fn a_stored_record_runs_through_the_engine_and_reaches_a_checkall_row() {
        let (mut doc, id) = doc_with_sign();
        let src = VisSource { scene: scene_from_document(&doc, &|_| Vec::new(), &SceneOptions::default()), x_heights: HashMap::new() };
        // Before any inputs: the engine says what it needs.
        let a = analyse(&doc, &src, id);
        assert!(a.overall() == Status::InputNeeded || a.skipped.is_some(), "{:?}", a.overall());
        let mut f = VisForm::new(Some(id));
        f.eye_height_accepted = true;
        f.traffic = 1;
        f.eye_mode = 2;
        f.eye_text = "1.75".into();
        store_form(&mut doc, &f).unwrap();
        let a = analyse(&doc, &src, id);
        assert!(a.skipped.is_none() && a.sweep.is_some(), "{:?}", a.skipped);
        assert!(!a.lines.is_empty() && a.lines.iter().any(|l| !l.citation.is_empty() || l.id.contains("visib")));
        let r = run(&doc, &src, Some(vec![id]));
        assert_eq!(r.analyses.len(), 1);
        assert!(r.summary_csv().contains("VS-1"));
        // The drawings exist for a swept sign and are offered as sheet feeds by the same data.
        let (plan, profile) = drawings_of(&a, Some(&src.scene), 5.0);
        assert!(plan.is_some() && profile.is_some());
        // The two drawings are offered to the sheets as detail feeds named after the sign.
        let live = crate::sheets_glue::Live::from_doc(&doc, &crate::sheets_glue::test_support::registry(), None);
        assert!(live.by_element.contains_key("vis_plan:VS-1") && live.by_element.contains_key("vis_profile:VS-1"), "{:?}", live.by_element.keys().collect::<Vec<_>>());
        // CHECKALL carries a row for the sign, with the engine's lines.
        let report = crate::check_all::run_all(&doc, &crate::check_all::Sources { notes_of: &|_| None, label_of: &|c| c.to_string(), vis: Some(&src) });
        let row = report.rows.iter().find(|r| r.mark == "VS-1" && r.check.starts_with("Sign visibility")).expect("a CHECKALL row for the visibility");
        assert!(!row.lines.is_empty(), "{row:?}");
        // Without the confirmation recorded, the same inputs are input needed.
        set_accepted(&mut doc, id, false);
        let report = crate::check_all::run_all(&doc, &crate::check_all::Sources { notes_of: &|_| None, label_of: &|c| c.to_string(), vis: Some(&src) });
        let row = report.rows.iter().find(|r| r.mark == "VS-1" && r.check.starts_with("Sign visibility")).unwrap();
        assert_eq!(row.status, crate::check_all::Status::InputNeeded);
        assert!(row.reason.contains("eye height"));
    }
}
