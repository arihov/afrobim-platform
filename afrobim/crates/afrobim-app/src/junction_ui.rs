//! The INTERSECTION and ROUNDABOUT commands: pick the roads, list the crossings found, choose the
//! corridor type, edit the edge offsets, widening tables and the kerb return of each quadrant with a
//! live plan preview, with the defaults basis filling the fields whose condition the dialog knows.
//! The geometry, profiles and corridor description are the `afrobim-junction` engine's; creating them
//! makes real alignment, profile and corridor objects.

use afrobim_alignment::Alignment;
use afrobim_corridor::{Assembly, Frequency};
use afrobim_defaults::Conditions;
use afrobim_junction::*;
use afrobim_kernel::{ElementId, SelectMode};
use egui::{pos2, Color32, RichText, Sense, Stroke};
use std::result::Result;

use crate::app::AfroBimApp;
use crate::civil::{self, INTERSECTION, ROUNDABOUT};
use crate::civil_dlg::{self as d, fmt};
use crate::civil_junction::{self as cj, IntersectionData, RoundaboutData};
use crate::defaults_glue as dg;
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

#[derive(Default)]
pub struct JunctionUi {
    pub int: Option<IntForm>,
    pub round: Option<RoundForm>,
    /// Last time the dependencies were checked against the document.
    pub last_dep: u64,
}

fn num(what: &str, t: &str) -> Result<f64, String> {
    t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{what} is not a number."))
}
fn opt(what: &str, t: &str) -> Result<Option<f64>, String> {
    if t.trim().is_empty() {
        Ok(None)
    } else {
        num(what, t).map(Some)
    }
}

// ─── drawing ────────────────────────────────────────────────────────────────

struct Preview {
    layers: Vec<(String, Vec<Vec<(f64, f64)>>, Color32)>,
}

fn layer_colour(name: &str) -> Color32 {
    match name {
        "roads" => Color32::from_gray(120),
        "edges" => Color32::from_rgb(0x4D, 0xA3, 0xFF),
        "returns" => Color32::from_rgb(0xE0, 0x9A, 0x40),
        "kerb lines" => Color32::from_rgb(0x62, 0xC9, 0xE8),
        "islands" | "splitters" => Color32::from_rgb(0x9C, 0xC9, 0x6B),
        "crossing" => Color32::from_rgb(0xE0, 0x6C, 0x9F),
        "entry kerbs" | "exit kerbs" => Color32::from_rgb(0xE0, 0x9A, 0x40),
        _ => Color32::from_rgb(0x4D, 0xA3, 0xFF),
    }
}

fn draw_preview(ui: &mut egui::Ui, p: &Preview) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(520.0), 330.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0x16, 0x1A, 0x20));
    let pts: Vec<(f64, f64)> = p.layers.iter().flat_map(|l| l.1.iter().flatten().copied()).collect();
    if pts.is_empty() {
        return;
    }
    let (x0, x1) = (pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min), pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max));
    let (y0, y1) = (pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min), pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max));
    let k = ((rect.width() as f64 - 20.0) / (x1 - x0).max(1e-6)).min((rect.height() as f64 - 20.0) / (y1 - y0).max(1e-6));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let map = |q: (f64, f64)| pos2(rect.center().x + ((q.0 - cx) * k) as f32, rect.center().y - ((q.1 - cy) * k) as f32);
    for (name, lines, col) in &p.layers {
        for l in lines {
            if l.len() == 1 {
                painter.circle_stroke(map(l[0]), 5.0, Stroke::new(1.5_f32, *col));
            } else if l.len() >= 2 {
                painter.add(egui::Shape::line(l.iter().map(|q| map(*q)).collect(), Stroke::new(if name == "roads" { 1.0 } else { 1.6 }, *col)));
            }
        }
    }
    painter.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, format!("Plan preview, {:.0} m across", (x1 - x0).max(y1 - y0)), egui::FontId::proportional(10.0), Color32::from_gray(170));
}

fn to_preview(drawing: &Drawing, roads: Vec<Vec<(f64, f64)>>) -> Preview {
    let mut layers = vec![("roads".to_string(), roads, layer_colour("roads"))];
    layers.extend(drawing.layers.iter().map(|l| (l.name.clone(), l.polylines.clone(), layer_colour(&l.name))));
    Preview { layers }
}

fn findings_ui(ui: &mut egui::Ui, id: &str, f: &[(String, String, String)]) {
    if f.is_empty() {
        return;
    }
    ui_kit::heading(ui, &format!("Findings ({})", f.len()));
    egui::ScrollArea::vertical().id_salt(id).max_height(120.0).show(ui, |ui| {
        for (code, sev, msg) in f {
            let col = match sev.as_str() {
                "error" => crate::theme::danger(),
                "warning" => crate::theme::warn(),
                _ => ui.visuals().weak_text_color(),
            };
            ui.add(egui::Label::new(RichText::new(format!("{sev}  {code}  {msg}")).small().color(col)).wrap());
        }
    });
}

// ─── INTERSECTION ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WRow {
    road: usize,
    side: usize,
    start: String,
    taper: String,
    len: String,
    exit: String,
    width: String,
}

#[derive(Clone)]
pub struct RetRow {
    on: bool,
    kind: usize,
    r1: String,
    r2: String,
    r3: String,
    o1: String,
    o3: String,
    leg_p: String,
    leg_s: String,
}

pub struct IntForm {
    edit: Option<ElementId>,
    name: String,
    primary: usize,
    secondary: usize,
    extend: String,
    crossing: usize,
    corridor_type: usize,
    // edges
    p_left: String,
    p_right: String,
    s_left: String,
    s_right: String,
    primary_road: dg::RoadChoice,
    secondary_type: usize,
    widen: Vec<WRow>,
    // widening defaults helper
    taper_basis: usize,
    setting: usize,
    speed_idx: usize,
    grad_case: usize,
    h_taper: String,
    h_len: String,
    h_width: String,
    add_to: usize,
    // extents
    ext: [String; 4],
    // returns
    ret_basis: usize,
    rets: Vec<RetRow>,
    // visibility reference
    vis_type: usize,
    // vertical
    vertical: bool,
    crossfall_from_alignment: bool,
    cf_p: String,
    cf_s: String,
    tie_in: String,
    edge_spacing: String,
    return_spacing: String,
    // corridor
    corridor_on: bool,
    ground: Option<ElementId>,
    asm: [Option<usize>; 5],
    freq: [String; 4],
    target_step: String,
    end_inset: String,
    preview: Option<(String, Result<(Preview, Vec<(String, String, String)>, Vec<Crossing>), String>)>,
    message: String,
}

const SECONDARY_TYPES: [(&str, &str); 3] = [("simple_or_ghost_single_lane", "Simple or ghost island, single lane"), ("sld_or_dual_single_lane", "Single lane dualling, or dual, single lane"), ("two_lanes", "Two lanes")];
const SETTINGS: [(&str, &str); 3] = [("urban", "Urban"), ("rural", "Rural"), ("staggered", "Staggered")];
const GRAD_CASES: [(&str, &str); 5] = [("length_up_0_4", "Upgrade 0 to 4 %"), ("length_up_over_4", "Upgrade over 4 %"), ("length_down_0_4", "Downgrade 0 to 4 %"), ("length_down_over_4_dual", "Downgrade over 4 %, dual"), ("length_down_over_4_single", "Downgrade over 4 %, single")];
const VIS_TYPES: [(&str, &str); 3] = [("direct_access", "Direct access"), ("simple_priority", "Simple priority junction"), ("other_priority", "Other priority junction")];
const ASM_LABELS: [&str; 5] = ["Primary approach", "Primary through", "Secondary approach", "Secondary mouth", "Returns"];

impl IntForm {
    fn new(example: bool, ground: Option<ElementId>) -> IntForm {
        let e = |s: &str| if example { s.to_string() } else { String::new() };
        IntForm {
            edit: None, name: String::new(), primary: 0, secondary: 1, extend: String::new(), crossing: 0, corridor_type: 0,
            p_left: e("3.5"), p_right: e("3.5"), s_left: e("3"), s_right: e("3"), primary_road: Default::default(), secondary_type: 0, widen: vec![],
            taper_basis: 0, setting: 0, speed_idx: 0, grad_case: 0, h_taper: String::new(), h_len: String::new(), h_width: String::new(), add_to: 0,
            ext: [e("60"), e("60"), e("50"), e("50")],
            ret_basis: 0, rets: vec![RetRow { on: true, kind: 0, r1: String::new(), r2: String::new(), r3: String::new(), o1: String::new(), o3: String::new(), leg_p: String::new(), leg_s: String::new() }; 4],
            vis_type: 0, vertical: false, crossfall_from_alignment: false, cf_p: String::new(), cf_s: String::new(), tie_in: e("20"), edge_spacing: e("5"), return_spacing: e("2"),
            corridor_on: false, ground, asm: [None; 5], freq: [e("10"), e("10"), e("10"), e("5")], target_step: e("1"), end_inset: e("0.2"),
            preview: None, message: String::new(),
        }
    }

    fn quadrant_exists(c: &Crossing, q: &Quadrant) -> bool {
        let pa = if q.primary_arm == Arm::Ahead { c.arms_a.ahead } else { c.arms_a.behind };
        let sa = if q.secondary_arm == Arm::Ahead { c.arms_b.ahead } else { c.arms_b.behind };
        pa && sa
    }

    /// The engine inputs the form describes, or the first thing that is wrong.
    fn to_inputs(&self, names: &[String], asms: &[Assembly], crossings: &[Crossing]) -> Result<IntersectionInputs, String> {
        let (p, s) = (names.get(self.primary).ok_or("Choose the primary road.")?, names.get(self.secondary).ok_or("Choose the secondary road.")?);
        if p == s {
            return Err("The two roads must be different alignments.".into());
        }
        let edges = |l: &str, r: &str, road: usize| -> Result<RoadEdges, String> {
            let mut e = RoadEdges { left_offset_m: num("A left edge offset", l)?, right_offset_m: num("A right edge offset", r)?, left_widening: vec![], right_widening: vec![] };
            for w in self.widen.iter().filter(|w| w.road == road) {
                let wd = Widening { start_station_m: num("A widening start", &w.start)?, taper_length_m: num("A widening taper", &w.taper)?, length_m: num("A widening length", &w.len)?, exit_taper_length_m: opt("A widening exit taper", &w.exit)?.unwrap_or(0.0), width_m: num("A widening width", &w.width)? };
                if w.side == 0 { e.left_widening.push(wd) } else { e.right_widening.push(wd) }
            }
            Ok(e)
        };
        let mut returns = Vec::new();
        for (q, r) in Quadrant::ALL.iter().zip(&self.rets) {
            if !r.on || !crossings.get(self.crossing).is_none_or(|c| Self::quadrant_exists(c, q)) {
                continue;
            }
            let kind = match r.kind {
                0 => ReturnKind::Simple { radius_m: num(&format!("The radius of {}", q.label()), &r.r1)? },
                1 => ReturnKind::Compound { radius1_m: num("Radius 1", &r.r1)?, radius2_m: num("Radius 2", &r.r2)?, radius3_m: num("Radius 3", &r.r3)?, offset1_m: num("Offset 1", &r.o1)?, offset3_m: num("Offset 3", &r.o3)? },
                _ => ReturnKind::Chamfer { primary_leg_m: num("The primary leg", &r.leg_p)?, secondary_leg_m: num("The secondary leg", &r.leg_s)? },
            };
            returns.push(ReturnSpec { quadrant: q.clone(), kind });
        }
        let vertical = if self.vertical {
            let cf = |t: &str| -> Result<CrossfallInput, String> {
                if self.crossfall_from_alignment {
                    Ok(CrossfallInput::FromAlignment)
                } else {
                    let v = num("A crossfall", t)?;
                    Ok(CrossfallInput::Fixed { left_pct: v, right_pct: v })
                }
            };
            Some(VerticalInputs { primary_crossfall: cf(&self.cf_p)?, secondary_crossfall: cf(&self.cf_s)?, tie_in_length_m: num("The tie-in length", &self.tie_in)?, lock_sample_spacing_m: None, edge_profile_spacing_m: num("The edge profile spacing", &self.edge_spacing)?, return_profile_spacing_m: num("The return profile spacing", &self.return_spacing)? })
        } else {
            None
        };
        let corridor = if self.corridor_on {
            let pick = |i: usize| -> Result<String, String> { self.asm[i].and_then(|a| asms.get(a)).map(|a| a.name.clone()).ok_or_else(|| format!("Choose the {} assembly.", ASM_LABELS[i].to_lowercase())) };
            let fr = |i: usize| -> Result<Frequency, String> { Ok(Frequency::uniform(num("A section interval", &self.freq[i])?)) };
            Some(CorridorInputs { name: None, library: asms.to_vec(), primary_approach_assembly: pick(0)?, primary_through_assembly: pick(1)?, secondary_approach_assembly: pick(2)?, secondary_mouth_assembly: pick(3)?, return_assembly: pick(4)?, approach_frequency: fr(0)?, through_frequency: fr(1)?, mouth_frequency: fr(2)?, return_frequency: fr(3)?, target_step_m: num("The target step", &self.target_step)?, end_inset_m: num("The end inset", &self.end_inset)? })
        } else {
            None
        };
        Ok(IntersectionInputs {
            name: if self.name.trim().is_empty() { "Intersection".into() } else { self.name.trim().to_string() },
            primary: p.clone(),
            secondary: s.clone(),
            crossing_index: self.crossing,
            extend_m: opt("The extension", &self.extend)?.unwrap_or(0.0),
            corridor_type: if self.corridor_type == 0 { CorridorType::PrimaryCrownMaintained } else { CorridorType::AllCrownsMaintained },
            primary_edges: edges(&self.p_left, &self.p_right, 0)?,
            secondary_edges: edges(&self.s_left, &self.s_right, 1)?,
            extents: JunctionExtents { primary_behind_m: num("The primary extent behind", &self.ext[0])?, primary_ahead_m: num("The primary extent ahead", &self.ext[1])?, secondary_behind_m: num("The secondary extent behind", &self.ext[2])?, secondary_ahead_m: num("The secondary extent ahead", &self.ext[3])? },
            returns,
            islands: vec![],
            vertical,
            corridor,
        })
    }

    fn from_inputs(i: &IntersectionInputs, names: &[String]) -> IntForm {
        let mut f = IntForm::new(false, None);
        f.name = i.name.clone();
        f.primary = names.iter().position(|n| *n == i.primary).unwrap_or(0);
        f.secondary = names.iter().position(|n| *n == i.secondary).unwrap_or(1);
        f.crossing = i.crossing_index;
        f.extend = if i.extend_m > 0.0 { afrobim_defaults::fmt_num(i.extend_m) } else { String::new() };
        f.corridor_type = if i.corridor_type == CorridorType::PrimaryCrownMaintained { 0 } else { 1 };
        let t = |v: f64| afrobim_defaults::fmt_num(v);
        f.p_left = t(i.primary_edges.left_offset_m);
        f.p_right = t(i.primary_edges.right_offset_m);
        f.s_left = t(i.secondary_edges.left_offset_m);
        f.s_right = t(i.secondary_edges.right_offset_m);
        for (road, e) in [(0usize, &i.primary_edges), (1usize, &i.secondary_edges)] {
            for (side, list) in [(0usize, &e.left_widening), (1usize, &e.right_widening)] {
                for w in list {
                    f.widen.push(WRow { road, side, start: t(w.start_station_m), taper: t(w.taper_length_m), len: t(w.length_m), exit: t(w.exit_taper_length_m), width: t(w.width_m) });
                }
            }
        }
        f.ext = [t(i.extents.primary_behind_m), t(i.extents.primary_ahead_m), t(i.extents.secondary_behind_m), t(i.extents.secondary_ahead_m)];
        for r in &mut f.rets {
            r.on = false;
        }
        for spec in &i.returns {
            if let Some(k) = Quadrant::ALL.iter().position(|q| *q == spec.quadrant) {
                f.rets[k].on = true;
                match &spec.kind {
                    ReturnKind::Simple { radius_m } => {
                        f.rets[k].kind = 0;
                        f.rets[k].r1 = t(*radius_m);
                    }
                    ReturnKind::Compound { radius1_m, radius2_m, radius3_m, offset1_m, offset3_m } => {
                        f.rets[k].kind = 1;
                        (f.rets[k].r1, f.rets[k].r2, f.rets[k].r3, f.rets[k].o1, f.rets[k].o3) = (t(*radius1_m), t(*radius2_m), t(*radius3_m), t(*offset1_m), t(*offset3_m));
                    }
                    ReturnKind::Chamfer { primary_leg_m, secondary_leg_m } => {
                        f.rets[k].kind = 2;
                        (f.rets[k].leg_p, f.rets[k].leg_s) = (t(*primary_leg_m), t(*secondary_leg_m));
                    }
                }
            }
        }
        if let Some(v) = &i.vertical {
            f.vertical = true;
            f.tie_in = t(v.tie_in_length_m);
            f.edge_spacing = t(v.edge_profile_spacing_m);
            f.return_spacing = t(v.return_profile_spacing_m);
            match (&v.primary_crossfall, &v.secondary_crossfall) {
                (CrossfallInput::Fixed { left_pct, .. }, CrossfallInput::Fixed { left_pct: s, .. }) => {
                    f.cf_p = t(*left_pct);
                    f.cf_s = t(*s);
                }
                _ => f.crossfall_from_alignment = true,
            }
        }
        f
    }
}

impl AfroBimApp {
    pub(crate) fn junction_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        let names: Vec<String> = cj::all_alignments(&self.doc).into_iter().map(|a| a.name).collect();
        if names.len() < if cv == crate::civil_cmds::Cv::Roundabout { 1 } else { 2 } {
            self.status = if cv == crate::civil_cmds::Cv::Roundabout { "A roundabout needs at least one alignment for its arms: import or draw one first.".into() } else { "An intersection needs two alignments: import or draw them first (ALIGNDESIGN, LANDXML).".into() };
            return;
        }
        let example = self.options.civil_demo;
        let ground = civil::surface_choices(&self.doc).first().map(|s| s.0);
        if cv == crate::civil_cmds::Cv::Intersection {
            let sel = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == INTERSECTION));
            let f = match sel.and_then(|id| civil::get::<IntersectionData>(&self.doc, id, INTERSECTION).map(|d| (id, d))) {
                Some((id, dt)) => {
                    let mut f = IntForm::from_inputs(&dt.inputs, &names);
                    f.edit = Some(id);
                    f.ground = dt.ground.map(ElementId).or(ground);
                    f
                }
                None => IntForm::new(example, ground),
            };
            self.civil_ui.junction.int = Some(f);
        } else {
            self.civil_ui.junction.round = Some(RoundForm::new(example, &names, self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == ROUNDABOUT)).and_then(|id| civil::get::<RoundaboutData>(&self.doc, id, ROUNDABOUT).map(|d| (id, d)))));
        }
    }

    fn intersection_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.junction.int.take() else { return };
        let aligns: Vec<Alignment> = cj::all_alignments(&self.doc);
        let names: Vec<String> = aligns.iter().map(|a| a.name.clone()).collect();
        let asms = cj::assemblies_of(&self.doc);
        let surfaces = civil::surface_choices(&self.doc);
        // The crossings of the two chosen roads.
        let extend = f.extend.trim().parse::<f64>().unwrap_or(0.0).max(0.0);
        let crossings: Vec<Crossing> = match (aligns.get(f.primary), aligns.get(f.secondary)) {
            (Some(a), Some(b)) if f.primary != f.secondary => find_crossings(a, b, extend),
            _ => vec![],
        };
        f.crossing = f.crossing.min(crossings.len().saturating_sub(1));
        // The live preview, recomputed when the inputs change.
        let inputs = f.to_inputs(&names, &asms, &crossings);
        let key = inputs.as_ref().map(|i| serde_json::to_string(i).unwrap_or_default()).unwrap_or_else(|e| e.clone());
        if f.preview.as_ref().is_none_or(|p| p.0 != key) {
            let r = match &inputs {
                Ok(i) => build_intersection(i, &aligns).map_err(|e| e.to_string()).map(|r| {
                    let roads: Vec<Vec<(f64, f64)>> = [&i.primary, &i.secondary].iter().filter_map(|n| aligns.iter().find(|a| &a.name == *n)).map(|a| polyline_between(a, a.start_station_m, a.end_station_m(), 1.0)).collect();
                    (to_preview(&r.drawing(0.25), roads), r.findings.iter().map(|x| (x.code.clone(), format!("{:?}", x.severity).to_lowercase(), format!("{}: {}", x.subject, x.message))).collect(), r.crossings.clone())
                }),
                Err(e) => Err(e.clone()),
            };
            f.preview = Some((key, r));
        }
        let mut create = false;
        let mut add_row = false;
        let ready = matches!(&f.preview, Some((_, Ok(_))));
        let editing = f.edit.is_some();
        let pos = NumRule::default().min(0.0);
        let out = Dialog::new("Intersection", crate::glyphs::by_name("INTERSECTION")).width(960.0).resizable(true).max_body_height(680.0).help("Choose the two alignments and the crossing, then the lane edge offsets, any widening, and the kerb return of each quadrant. The plan preview and the findings update as you type. Creating makes real alignments with their profiles (the four edges, the kerb returns and lines, the secondary profile locked to the primary) and, if you ask for it, a corridor. Fields with a defaults entry for the condition you choose are pre-filled with the citation; the rest are inputs.").show(
            ctx,
            |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(430.0);
                        ui_kit::form(ui, "int-head", |fm| {
                            fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Junction A"));
                            fm.row("Primary road", |ui| {
                                ui_kit::combo_owned(ui, "int-p", &mut f.primary, &names);
                            });
                            fm.row("Secondary road", |ui| {
                                ui_kit::combo_owned(ui, "int-s", &mut f.secondary, &names);
                            });
                            fm.row_note("Extend a short road by", "Finds a crossing a secondary that stops short would make; the build refuses it", |ui| {
                                d::num_opt_rule(ui, &mut f.extend, "m", &pos).ok();
                            });
                            fm.row("Corridor type", |ui| {
                                ui_kit::segmented(ui, &mut f.corridor_type, &["Primary crown maintained", "All crowns maintained"]);
                            });
                        });
                        ui_kit::section(ui, &format!("Crossings found ({})", crossings.len()), |ui| {
                            if crossings.is_empty() {
                                ui.label(RichText::new("The two roads do not cross.").small().weak());
                            }
                            let rows: Vec<Vec<String>> = crossings.iter().enumerate().map(|(i, c)| vec![if i == f.crossing { "(chosen)".into() } else { String::new() }, fmt(c.station_a_m, 3), fmt(c.station_b_m, 3), fmt(c.angle_rad.to_degrees(), 1), fmt(c.skew_rad.to_degrees(), 1), format!("{:?}", c.kind).to_lowercase()]).collect();
                            if !rows.is_empty() {
                                ui_kit::results_table(ui, "int-cross", &["", "Station primary", "Station secondary", "Angle deg", "Skew deg", "Kind"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Text], &rows);
                                if crossings.len() > 1 {
                                    ui.horizontal(|ui| {
                                        ui.label("Use crossing");
                                        let labels: Vec<String> = (1..=crossings.len()).map(|i| i.to_string()).collect();
                                        ui_kit::combo_owned(ui, "int-cx", &mut f.crossing, &labels);
                                    });
                                }
                            }
                        });
                        ui_kit::section(ui, "Lane edges", |ui| {
                            ui_kit::form(ui, "int-edges", |fm| {
                                fm.row_note("Primary road type", "Single carriageway types give the edge offset (half the carriageway width)", |ui| dg::road_selector(ui, "int-road", &mut f.primary_road));
                                let single = f.primary_road.prefix().is_some() && dg::get(&f.primary_road.key("carriageway_width").unwrap_or_default(), &f.primary_road.cond()).is_some() && matches!(f.primary_road.road.as_str(), "S2" | "WS2" | "WS2+1" | "WS2_climbing" | "SU2");
                                for (label, field, salt) in [("Primary, left offset", &mut f.p_left, "pl"), ("Primary, right offset", &mut f.p_right, "pr")] {
                                    fm.row(label, |ui| {
                                        ui.vertical(|ui| match f.primary_road.key("carriageway_width") {
                                            Some(k) if single => {
                                                dg::field(ui, ("int", salt), field, "m", &k, &f.primary_road.cond(), 0.5, &NumRule::required().above(0.0));
                                            }
                                            _ => {
                                                d::num_rule(ui, field, "m", NumRule::required().above(0.0));
                                            }
                                        });
                                    });
                                }
                                fm.row_note("Secondary approach", "The pack's approach lane width gives the offset of a single lane each side", |ui| {
                                    ui_kit::combo(ui, "int-sec-type", &mut f.secondary_type, &SECONDARY_TYPES.map(|t| t.1));
                                });
                                for (label, field, salt) in [("Secondary, left offset", &mut f.s_left, "sl"), ("Secondary, right offset", &mut f.s_right, "sr")] {
                                    fm.row(label, |ui| {
                                        ui.vertical(|ui| {
                                            let cond = Conditions::none().label("junction_type", SECONDARY_TYPES[f.secondary_type.min(2)].0);
                                            if f.secondary_type == 0 {
                                                dg::field(ui, ("int", salt), field, "m", "junction.minor_road.approach_lane_width", &cond, 1.0, &NumRule::required().above(0.0));
                                            } else {
                                                d::num_rule(ui, field, "m", NumRule::required().above(0.0));
                                                ui.label(RichText::new("The pack's approach lane width for this type is for lanes, not an edge offset: enter the offset.").size(crate::theme::FONT_SMALL - 1.0).weak());
                                            }
                                        });
                                    });
                                }
                            });
                        });
                        ui_kit::section(ui, "Extents of the build along each road", |ui| {
                            ui_kit::form(ui, "int-ext", |fm| {
                                for (i, label) in ["Primary, behind the crossing", "Primary, ahead", "Secondary, behind", "Secondary, ahead"].iter().enumerate() {
                                    fm.row(label, |ui| {
                                        d::num_rule(ui, &mut f.ext[i], "m", NumRule::required().above(0.0));
                                    });
                                }
                            });
                        });
                    });
                    ui.vertical(|ui| {
                        match &f.preview {
                            Some((_, Ok((p, _, _)))) => draw_preview(ui, p),
                            Some((_, Err(e))) => {
                                ui_kit::callout(ui, Level::Warning, e.clone());
                            }
                            None => {}
                        }
                        if let Some((_, Ok((_, findings, _)))) = &f.preview {
                            findings_ui(ui, "int-find", findings);
                        }
                    });
                });
                ui_kit::section(ui, "Kerb returns, one per quadrant", |ui| {
                    ui_kit::form(ui, "int-rbasis", |fm| {
                        fm.row_note("Radius basis", "Fills the simple radius of every quadrant from the defaults basis", |ui| {
                            ui_kit::combo(ui, "int-rb", &mut f.ret_basis, &["No basis: typed", "No design vehicle, urban", "No design vehicle, rural", "Design vehicle, urban", "Design vehicle, rural", "Design vehicle, staggered", "Ghost island, no tapers"]);
                        });
                    });
                    let (key, cond): (Option<&str>, Conditions) = match f.ret_basis {
                        1 => (Some("junction.corner_radius.simple_no_design_vehicle_urban"), Conditions::none()),
                        2 => (Some("junction.corner_radius.simple_no_design_vehicle_rural"), Conditions::none()),
                        3..=5 => (Some("junction.corner_radius.simple_design_vehicle"), Conditions::none().label("setting", SETTINGS[f.ret_basis - 3].0)),
                        6 => (Some("junction.corner_radius.ghost_island_no_tapers"), Conditions::none()),
                        _ => (None, Conditions::none()),
                    };
                    for (qi, q) in Quadrant::ALL.iter().enumerate() {
                        let exists = crossings.get(f.crossing).is_none_or(|c| IntForm::quadrant_exists(c, q));
                        if !exists {
                            continue;
                        }
                        let r = &mut f.rets[qi];
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut r.on, q.label());
                            ui_kit::combo(ui, &format!("int-rk{qi}"), &mut r.kind, &["Simple", "Compound", "Chamfer"]);
                        });
                        if r.on {
                            ui.indent(("int-ret", qi), |ui| match r.kind {
                                0 => {
                                    ui.vertical(|ui| match key {
                                        Some(k) => {
                                            dg::field(ui, ("int-ret", qi), &mut r.r1, "m radius", k, &cond, 1.0, &NumRule::required().above(0.0));
                                        }
                                        None => {
                                            d::num_rule(ui, &mut r.r1, "m radius", NumRule::required().above(0.0));
                                        }
                                    });
                                }
                                1 => {
                                    ui.horizontal(|ui| {
                                        for (t, u) in [(&mut r.r1, "R1"), (&mut r.r2, "R2"), (&mut r.r3, "R3"), (&mut r.o1, "o1"), (&mut r.o3, "o3")] {
                                            ui.label(u);
                                            ui.add(egui::TextEdit::singleline(t).desired_width(60.0));
                                        }
                                    });
                                }
                                _ => {
                                    ui.horizontal(|ui| {
                                        ui.label("Primary leg");
                                        ui.add(egui::TextEdit::singleline(&mut r.leg_p).desired_width(70.0));
                                        ui.label("Secondary leg");
                                        ui.add(egui::TextEdit::singleline(&mut r.leg_s).desired_width(70.0));
                                    });
                                }
                            });
                        }
                    }
                });
                ui_kit::section(ui, "Widening tables (turning lanes)", |ui| {
                    ui_kit::form(ui, "int-wbasis", |fm| {
                        fm.row_note("Turning lane basis", "Scratch values from the defaults basis; Add puts them in a row", |ui| {
                            ui_kit::combo(ui, "int-tb", &mut f.taper_basis, &["No basis: typed", "Simple junction taper (design vehicle)", "Ghost island taper", "Diverge, by design speed"]);
                        });
                    });
                    if f.taper_basis > 0 {
                        let speeds = dg::speeds_of("junction.diverge.length_up_0_4");
                        let sc = Conditions::none().label("setting", SETTINGS[f.setting.min(2)].0);
                        ui.horizontal(|ui| {
                            if f.taper_basis == 1 {
                                ui_kit::combo(ui, "int-set", &mut f.setting, &SETTINGS.map(|t| t.1));
                            }
                            if f.taper_basis == 3 {
                                let labels: Vec<String> = speeds.iter().map(|v| format!("{} km/h", afrobim_defaults::fmt_num(*v))).collect();
                                f.speed_idx = f.speed_idx.min(labels.len().saturating_sub(1));
                                ui_kit::combo_owned(ui, "int-spd", &mut f.speed_idx, &labels);
                                ui_kit::combo(ui, "int-gc", &mut f.grad_case, &GRAD_CASES.map(|t| t.1));
                            }
                        });
                        let dsc = speeds.get(f.speed_idx).map(|v| Conditions::none().design_speed(*v));
                        ui_kit::form(ui, "int-wh", |fm| {
                            fm.row("Taper length", |ui| {
                                ui.vertical(|ui| match (f.taper_basis, &dsc) {
                                    (1, _) => {
                                        dg::field(ui, "int-ht", &mut f.h_taper, "m", "junction.corner_taper.simple_design_vehicle_length", &sc, 1.0, &NumRule::default().above(0.0));
                                    }
                                    (2, _) => {
                                        dg::field(ui, "int-ht", &mut f.h_taper, "m", "junction.corner_taper.ghost_island_length", &Conditions::none(), 1.0, &NumRule::default().above(0.0));
                                    }
                                    (_, Some(c)) => {
                                        dg::field(ui, "int-ht", &mut f.h_taper, "m", "junction.diverge.length_direct_taper", c, 1.0, &NumRule::default().above(0.0));
                                    }
                                    _ => {}
                                });
                            });
                            if f.taper_basis == 3 {
                                fm.row("Full-width length", |ui| {
                                    ui.vertical(|ui| {
                                        if let Some(c) = &dsc {
                                            dg::field(ui, "int-hl", &mut f.h_len, "m", &format!("junction.diverge.{}", GRAD_CASES[f.grad_case.min(4)].0), c, 1.0, &NumRule::default().above(0.0));
                                        }
                                    });
                                });
                                fm.row("Width", |ui| {
                                    ui.vertical(|ui| {
                                        dg::field(ui, "int-hw", &mut f.h_width, "m", "junction.taper.diverge_width_at_corner", &Conditions::none(), 1.0, &NumRule::default().above(0.0));
                                    });
                                });
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Add a row for");
                        ui_kit::combo(ui, "int-wto", &mut f.add_to, &["Primary, left", "Primary, right", "Secondary, left", "Secondary, right"]);
                        if ui.button("Add a widening row").clicked() {
                            add_row = true;
                        }
                    });
                    if !f.widen.is_empty() {
                        egui::Grid::new("int-wtab").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                            for h in ["Road", "Side", "Start station", "Taper", "Full length", "Exit taper", "Width", ""] {
                                ui_kit::th(ui, h);
                            }
                            ui.end_row();
                            let mut remove = None;
                            for (i, w) in f.widen.iter_mut().enumerate() {
                                ui.label(if w.road == 0 { "primary" } else { "secondary" });
                                ui.label(if w.side == 0 { "left" } else { "right" });
                                for t in [&mut w.start, &mut w.taper, &mut w.len, &mut w.exit, &mut w.width] {
                                    ui.add(egui::TextEdit::singleline(t).desired_width(70.0));
                                }
                                if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this row").clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                            if let Some(i) = remove {
                                f.widen.remove(i);
                            }
                        });
                    }
                    ui_kit::note(ui, "Stations are along the road the row belongs to. A taper on a curved road is a straight chord (the engine reports the deviation).");
                });
                ui_kit::section(ui, "Visibility distances (reference)", |ui| {
                    ui_kit::combo(ui, "int-vis", &mut f.vis_type, &VIS_TYPES.map(|t| t.1));
                    let c = Conditions::none().label("junction_type", VIS_TYPES[f.vis_type.min(2)].0);
                    let rows = dg::reference_rows(&[("X distance, standard", "junction.visibility.x_distance_standard", c.clone()), ("X distance, minimum", "junction.visibility.x_distance_minimum", c), ("Y distance", "junction.visibility.y_distance", Conditions::none())]);
                    ui_kit::results_table(ui, "int-visrows", &["Value", "Number", "Unit", "Source"], &[Col::Text, Col::Num, Col::Text, Col::Text], &rows);
                });
                ui_kit::section(ui, "Levels (profiles of the edges, the locked secondary and the returns)", |ui| {
                    ui_kit::toggle(ui, &mut f.vertical, "Make the profiles");
                    if f.vertical {
                        ui_kit::toggle(ui, &mut f.crossfall_from_alignment, "Crossfall from the alignments' stored superelevation");
                        ui_kit::form(ui, "int-vert", |fm| {
                            if !f.crossfall_from_alignment {
                                for (label, field, salt) in [("Primary crossfall", &mut f.cf_p, "cfp"), ("Secondary crossfall", &mut f.cf_s, "cfs")] {
                                    fm.row_note(label, "Rising away from the centreline positive: a crown is negative", |ui| {
                                        ui.vertical(|ui| {
                                            dg::field(ui, ("int", salt), field, "%", "link.crossfall.normal", &Conditions::none(), -1.0, &NumRule::required());
                                        });
                                    });
                                }
                            }
                            fm.row("Tie-in length", |ui| {
                                d::num_rule(ui, &mut f.tie_in, "m", NumRule::required().above(0.0));
                            });
                            fm.row("Edge profile spacing", |ui| {
                                d::num_rule(ui, &mut f.edge_spacing, "m", NumRule::required().above(0.0));
                            });
                            fm.row("Return profile spacing", |ui| {
                                d::num_rule(ui, &mut f.return_spacing, "m", NumRule::required().above(0.0));
                            });
                        });
                    }
                });
                ui_kit::section(ui, "Corridor regions (through the corridor model)", |ui| {
                    ui_kit::toggle(ui, &mut f.corridor_on, "Also make the corridor");
                    if f.corridor_on {
                        if !f.vertical {
                            ui_kit::callout(ui, Level::Info, "The corridor needs the profiles: tick Make the profiles.");
                        }
                        let labels: Vec<String> = asms.iter().map(|a| a.name.clone()).collect();
                        ui_kit::form(ui, "int-cor", |fm| {
                            fm.row("Ground surface", |ui| {
                                d::pick_from(ui, "int-ground", &surfaces, &mut f.ground);
                            });
                            for (i, label) in ASM_LABELS.iter().enumerate() {
                                fm.row(&format!("{label} assembly"), |ui| {
                                    let mut sel = f.asm[i].map(|a| a + 1).unwrap_or(0);
                                    let mut items = vec!["(choose)".to_string()];
                                    items.extend(labels.iter().cloned());
                                    if ui_kit::combo_owned(ui, &format!("int-asm{i}"), &mut sel, &items) {
                                        f.asm[i] = sel.checked_sub(1);
                                    }
                                });
                            }
                            for (i, label) in ["Approach", "Through", "Mouth", "Return"].iter().enumerate() {
                                fm.row(&format!("{label} section interval"), |ui| {
                                    d::num_rule(ui, &mut f.freq[i], "m", NumRule::required().above(0.0));
                                });
                            }
                            fm.row("Target step", |ui| {
                                d::num_rule(ui, &mut f.target_step, "m", NumRule::required().above(0.0));
                            });
                            fm.row("End inset", |ui| {
                                d::num_rule(ui, &mut f.end_inset, "m", NumRule::required().min(0.0));
                            });
                        });
                    }
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new(if editing { "Update the intersection" } else { "Create the intersection" }).icon(crate::glyphs::by_name("INTERSECTION")).enabled(ready, "The inputs are not complete or the build is refused: see the preview").show(ui);
                create = o.primary;
                o
            },
        );
        if add_row {
            let road = f.add_to / 2;
            f.widen.push(WRow { road, side: f.add_to % 2, start: String::new(), taper: f.h_taper.clone(), len: f.h_len.clone(), exit: String::new(), width: f.h_width.clone() });
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if create {
            match inputs {
                Ok(i) => match cj::make_intersection(&mut self.doc, f.edit, &i.name.clone(), i, f.ground.map(|g| g.0), false, "Intersection dialog") {
                    Ok((id, notes)) => {
                        self.doc.select(&[id], SelectMode::Replace);
                        self.status = format!("Made the intersection. {}", notes.join(" "));
                        self.schedule_commit_now();
                        return;
                    }
                    Err(e) => f.message = format!("Refused: {e}"),
                },
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.junction.int = Some(f);
    }
}

// ─── ROUNDABOUT ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ArmRow {
    name: String,
    alignment: usize,
    approach_half: String,
    flare: String,
    entry_width: String,
    entry_radius: String,
    exit_width: String,
    exit_radius: String,
    departure_half: String,
    splitter: Vec<(String, String)>,
    nose: String,
    path: [String; 3],
}

pub struct RoundForm {
    edit: Option<ElementId>,
    name: String,
    e: String,
    n: String,
    icd: String,
    circ_width: String,
    apron: String,
    clockwise: bool,
    arms: Vec<ArmRow>,
    sel: usize,
    preview: Option<(String, Result<(Preview, RoundaboutResult), String>)>,
    message: String,
}

impl RoundForm {
    fn new(example: bool, names: &[String], existing: Option<(ElementId, RoundaboutData)>) -> RoundForm {
        let ex = |s: &str| if example { s.to_string() } else { String::new() };
        if let Some((id, d)) = existing {
            let i = &d.inputs;
            let t = |v: f64| afrobim_defaults::fmt_num(v);
            return RoundForm {
                edit: Some(id), name: i.name.clone(), e: format!("{}", i.centre.0), n: format!("{}", i.centre.1), icd: t(i.inscribed_diameter_m), circ_width: t(i.circulatory_width_m), apron: t(i.apron_width_m), clockwise: i.circulation == Circulation::Clockwise,
                arms: i.arms.iter().map(|a| ArmRow { name: a.name.clone(), alignment: names.iter().position(|n| *n == a.alignment).unwrap_or(0), approach_half: t(a.approach_half_width_m), flare: t(a.flare_length_m), entry_width: t(a.entry_width_m), entry_radius: t(a.entry_radius_m), exit_width: t(a.exit_width_m), exit_radius: t(a.exit_radius_m), departure_half: t(a.departure_half_width_m), splitter: a.splitter.points.iter().map(|p| (t(p.distance_m), t(p.half_width_m))).collect(), nose: t(a.splitter.nose_radius_m), path: a.path_offsets.map(|p| [t(p.from_entry_kerb_m), t(p.from_splitter_m), t(p.from_central_island_m)]).unwrap_or_default() }).collect(),
                sel: 0, preview: None, message: String::new(),
            };
        }
        let arm = |name: &str, al: usize| ArmRow { name: name.into(), alignment: al, approach_half: ex("3.5"), flare: ex("25"), entry_width: String::new(), entry_radius: String::new(), exit_width: ex("4"), exit_radius: String::new(), departure_half: ex("3.5"), splitter: vec![(ex("0"), ex("0.5")), (ex("15"), ex("2"))], nose: ex("0.3"), path: [ex("2"), ex("2"), ex("2")] };
        RoundForm { edit: None, name: String::new(), e: String::new(), n: String::new(), icd: String::new(), circ_width: String::new(), apron: ex("1"), clockwise: false, arms: vec![arm("Arm 1", 0)], sel: 0, preview: None, message: String::new() }
    }

    fn to_inputs(&self, aligns: &[Alignment]) -> Result<RoundaboutInputs, String> {
        let mut arms = Vec::new();
        for a in &self.arms {
            let al = aligns.get(a.alignment).ok_or("Choose an alignment for every arm.")?;
            let splitter = SplitterInput { points: a.splitter.iter().filter(|p| !p.0.trim().is_empty() || !p.1.trim().is_empty()).map(|p| Ok(SplitterPoint { distance_m: num("A splitter distance", &p.0)?, half_width_m: num("A splitter half width", &p.1)? })).collect::<Result<_, String>>()?, nose_radius_m: num("The nose radius", &a.nose)? };
            let path = if a.path.iter().all(|p| p.trim().is_empty()) { None } else { Some(PathOffsets { from_entry_kerb_m: num("A path offset", &a.path[0])?, from_splitter_m: num("A path offset", &a.path[1])?, from_central_island_m: num("A path offset", &a.path[2])? }) };
            arms.push(ArmInput { name: a.name.clone(), alignment: al.name.clone(), approach_half_width_m: num("The approach half width", &a.approach_half)?, flare_length_m: num("The flare length", &a.flare)?, entry_width_m: num("The entry width", &a.entry_width)?, entry_radius_m: num("The entry radius", &a.entry_radius)?, exit_width_m: num("The exit width", &a.exit_width)?, exit_radius_m: num("The exit radius", &a.exit_radius)?, departure_half_width_m: num("The departure half width", &a.departure_half)?, splitter, path_offsets: path });
        }
        Ok(RoundaboutInputs { name: if self.name.trim().is_empty() { "Roundabout".into() } else { self.name.trim().to_string() }, centre: (num("The centre easting", &self.e)?, num("The centre northing", &self.n)?), inscribed_diameter_m: num("The inscribed circle diameter", &self.icd)?, circulatory_width_m: num("The circulatory width", &self.circ_width)?, apron_width_m: num("The apron width", &self.apron)?, circulation: if self.clockwise { Circulation::Clockwise } else { Circulation::Anticlockwise }, arms })
    }
}

impl AfroBimApp {
    fn roundabout_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.junction.round.take() else { return };
        for (e, n) in std::mem::take(&mut self.civil_ui.queue) {
            f.e = format!("{e:.3}");
            f.n = format!("{n:.3}");
        }
        let aligns = cj::all_alignments(&self.doc);
        let names: Vec<String> = aligns.iter().map(|a| a.name.clone()).collect();
        let inputs = f.to_inputs(&aligns);
        let key = inputs.as_ref().map(|i| serde_json::to_string(i).unwrap_or_default()).unwrap_or_else(|e| e.clone());
        if f.preview.as_ref().is_none_or(|p| p.0 != key) {
            let r = match &inputs {
                Ok(i) => build_roundabout(i, &aligns).map_err(|e| e.to_string()).map(|r| {
                    let roads: Vec<Vec<(f64, f64)>> = i.arms.iter().filter_map(|a| aligns.iter().find(|x| x.name == a.alignment)).map(|a| polyline_between(a, a.start_station_m, a.end_station_m(), 1.0)).collect();
                    (to_preview(&r.drawing(0.25), roads), r)
                }),
                Err(e) => Err(e.clone()),
            };
            f.preview = Some((key, r));
        }
        let ready = matches!(&f.preview, Some((_, Ok(_))));
        let editing = f.edit.is_some();
        let mut create = false;
        let mut pick = false;
        let pos = NumRule::required().above(0.0);
        let cond0 = Conditions::none();
        let out = Dialog::new("Roundabout", crate::glyphs::by_name("ROUNDABOUTDESIGN")).width(960.0).resizable(true).max_body_height(680.0).help("A roundabout is an inscribed circle, a circulatory carriageway, an apron and a central island, with each arm's entry and exit kerbs and splitter island. The entry angle and the entry path radius are measured by AfroBIM's own construction (not the construction CD 116 defines) and are shown beside the limits of the defaults basis as a guide. The fields the basis has a value for are pre-filled with their citation.").show(
            ctx,
            |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(440.0);
                        ui_kit::form(ui, "rb-head", |fm| {
                            fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Junction R1"));
                            fm.row("Centre", |ui| {
                                ui.horizontal(|ui| {
                                    d::num(ui, &mut f.e, "E");
                                    d::num(ui, &mut f.n, "N");
                                    if ui.button("Pick").clicked() {
                                        pick = true;
                                    }
                                });
                            });
                            fm.row("Circulation", |ui| {
                                ui_kit::segmented(ui, &mut (f.clockwise as usize), &["Anticlockwise", "Clockwise"]);
                            });
                            fm.row_note("Inscribed circle diameter", "From the basis: the lower limit", |ui| {
                                ui.vertical(|ui| {
                                    dg::field(ui, "rb-icd", &mut f.icd, "m", "roundabout.icd.min", &cond0, 1.0, &pos);
                                });
                            });
                            fm.row_note("Circulatory width", "From the basis: its lower factor times the entry width of the first arm", |ui| {
                                ui.vertical(|ui| {
                                    let factor = dg::number("roundabout.circulatory_width.min_factor", &cond0);
                                    let entry = f.arms.first().and_then(|a| a.entry_width.trim().parse::<f64>().ok());
                                    if let (Some(k), Some(w)) = (factor, entry) {
                                        let want = afrobim_defaults::fmt_num(k * w);
                                        let _ = want;
                                        dg::field(ui, ("rb-cw", (k * w * 1000.0) as i64), &mut f.circ_width, "m", "roundabout.circulatory_width.min_factor", &cond0, w, &pos);
                                    } else {
                                        d::num_rule(ui, &mut f.circ_width, "m", pos.clone());
                                    }
                                });
                            });
                            fm.row("Apron width", |ui| {
                                d::num_rule(ui, &mut f.apron, "m", NumRule::required().min(0.0));
                            });
                        });
                        ui_kit::section(ui, "Arms", |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (i, a) in f.arms.iter().enumerate() {
                                    if ui.selectable_label(f.sel == i, &a.name).clicked() {
                                        f.sel = i;
                                    }
                                }
                                if ui.button("Add an arm").clicked() {
                                    let mut a = f.arms.last().cloned().unwrap_or_else(|| f.arms[0].clone());
                                    a.name = format!("Arm {}", f.arms.len() + 1);
                                    a.alignment = (a.alignment + 1).min(names.len().saturating_sub(1));
                                    f.arms.push(a);
                                    f.sel = f.arms.len() - 1;
                                }
                                if f.arms.len() > 1 && ui.button("Remove this arm").clicked() {
                                    f.arms.remove(f.sel);
                                    f.sel = f.sel.min(f.arms.len() - 1);
                                }
                            });
                            let k = f.sel.min(f.arms.len().saturating_sub(1));
                            let a = &mut f.arms[k];
                            ui_kit::form(ui, "rb-arm", |fm| {
                                fm.row("Name", |ui| d::text(ui, &mut a.name, ""));
                                fm.row("Alignment", |ui| {
                                    ui_kit::combo_owned(ui, "rb-al", &mut a.alignment, &names);
                                });
                                fm.row("Approach half width", |ui| {
                                    d::num_rule(ui, &mut a.approach_half, "m", pos.clone());
                                });
                                fm.row("Flare length", |ui| {
                                    d::num_rule(ui, &mut a.flare, "m", pos.clone());
                                });
                                fm.row_note("Entry width", "Single lane entry, from the basis", |ui| {
                                    ui.vertical(|ui| {
                                        dg::field(ui, ("rb-ew", k), &mut a.entry_width, "m", "roundabout.entry_lane_width.single_lane_entry", &cond0, 1.0, &pos);
                                    });
                                });
                                fm.row_note("Entry kerb radius", "The basis's lower limit", |ui| {
                                    ui.vertical(|ui| {
                                        dg::field(ui, ("rb-er", k), &mut a.entry_radius, "m", "roundabout.entry_kerb_radius.min", &cond0, 1.0, &pos);
                                    });
                                });
                                fm.row("Exit width", |ui| {
                                    d::num_rule(ui, &mut a.exit_width, "m", pos.clone());
                                });
                                fm.row_note("Exit kerb radius", "Normal, from the basis", |ui| {
                                    ui.vertical(|ui| {
                                        dg::field(ui, ("rb-xr", k), &mut a.exit_radius, "m", "roundabout.exit_kerb_radius.normal", &cond0, 1.0, &pos);
                                    });
                                });
                                fm.row("Departure half width", |ui| {
                                    d::num_rule(ui, &mut a.departure_half, "m", pos.clone());
                                });
                                fm.row("Nose radius", |ui| {
                                    d::num_rule(ui, &mut a.nose, "m", pos.clone());
                                });
                            });
                            ui.label(RichText::new("Splitter island: distance from the give-way line and half width, from the nose outwards").small().weak());
                            for p in &mut a.splitter {
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut p.0).desired_width(70.0).hint_text("distance"));
                                    ui.add(egui::TextEdit::singleline(&mut p.1).desired_width(70.0).hint_text("half width"));
                                });
                            }
                            if ui.small_button("Add a splitter point").clicked() {
                                a.splitter.push(Default::default());
                            }
                            ui.label(RichText::new("Entry path offsets (from the entry kerb, the splitter, the central island): blank for no path radius").small().weak());
                            ui.horizontal(|ui| {
                                for p in &mut a.path {
                                    ui.add(egui::TextEdit::singleline(p).desired_width(70.0));
                                }
                            });
                        });
                    });
                    ui.vertical(|ui| {
                        match &f.preview {
                            Some((_, Ok((p, _)))) => draw_preview(ui, p),
                            Some((_, Err(e))) => {
                                ui_kit::callout(ui, Level::Warning, e.clone());
                            }
                            None => {}
                        }
                        if let Some((_, Ok((_, r)))) = &f.preview {
                            ui_kit::heading(ui, "Measured, beside the limits of the basis");
                            let ang = (dg::number("roundabout.entry_angle.min", &cond0), dg::number("roundabout.entry_angle.max", &cond0));
                            let pr = dg::number("roundabout.entry_path_radius.max_normal", &cond0);
                            let rows: Vec<Vec<String>> = r.arms.iter().map(|a| vec![a.name.clone(), fmt(a.measures.entry_angle_rad.to_degrees(), 1), match ang { (Some(lo), Some(hi)) => format!("{} to {}", afrobim_defaults::fmt_num(lo), afrobim_defaults::fmt_num(hi)), _ => "no limit in the basis".into() }, a.measures.entry_path_radius_m.map(|v| fmt(v, 1)).unwrap_or_else(|| "(no path offsets)".into()), pr.map(|v| format!("at most {}", afrobim_defaults::fmt_num(v))).unwrap_or_else(|| "no limit in the basis".into()), fmt(a.measures.entry_width_m, 2), fmt(a.measures.flare_ratio, 3)]).collect();
                            ui_kit::results_table(ui, "rb-meas", &["Arm", "Entry angle deg", "Limit deg", "Entry path radius m", "Limit m", "Entry width m", "(e-v)/l"], &[Col::Text, Col::Num, Col::Text, Col::Num, Col::Text, Col::Num, Col::Num], &rows);
                            ui_kit::callout(ui, Level::Warning, "The entry angle and the entry path radius are measured by AfroBIM's own construction (afrobim-junction), which is not the one CD 116 defines. The limits are the basis's; compare as a guide only.");
                            let src = dg::get("roundabout.entry_angle.min", &cond0).map(|v| dg::citation_line(&v)).unwrap_or_default();
                            ui.label(RichText::new(src).size(crate::theme::FONT_SMALL - 1.0).weak());
                            let central = f.icd.trim().parse::<f64>().ok().zip(f.circ_width.trim().parse::<f64>().ok()).zip(f.apron.trim().parse::<f64>().ok()).map(|((i, c), a)| i - 2.0 * (c + a));
                            if let Some(c) = central {
                                let lim = dg::number("roundabout.central_island.min_diameter", &cond0);
                                ui.label(format!("Central island diameter {} m{}", fmt(c, 2), lim.map(|l| format!(", the basis's lower limit {} m", afrobim_defaults::fmt_num(l))).unwrap_or_default()));
                            }
                            findings_ui(ui, "rb-find", &r.findings.iter().map(|x| (x.code.clone(), format!("{:?}", x.severity).to_lowercase(), format!("{}: {}", x.subject, x.message))).collect::<Vec<_>>());
                        }
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new(if editing { "Update the roundabout" } else { "Create the roundabout" }).icon(crate::glyphs::by_name("ROUNDABOUTDESIGN")).enabled(ready, "The inputs are not complete or the build is refused: see the preview").show(ui);
                create = o.primary;
                o
            },
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = false;
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if create {
            match inputs {
                Ok(i) => match cj::make_roundabout(&mut self.doc, f.edit, &i.name.clone(), i, false, "Roundabout dialog") {
                    Ok((id, _)) => {
                        self.doc.select(&[id], SelectMode::Replace);
                        self.status = "Made the roundabout and its alignments.".into();
                        self.schedule_commit_now();
                        return;
                    }
                    Err(e) => f.message = format!("Refused: {e}"),
                },
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.junction.round = Some(f);
    }

    /// Rebuilds the intersections and roundabouts whose source alignments or assemblies changed.
    pub(crate) fn junction_refresh_tick(&mut self) {
        let h = crate::civil_cache::dependency_hash(&self.doc);
        if h == self.civil_ui.junction.last_dep {
            return;
        }
        self.civil_ui.junction.last_dep = h;
        let mut done = Vec::new();
        for (id, why) in cj::stale_intersections(&self.doc) {
            let Some(d) = civil::get::<IntersectionData>(&self.doc, id, INTERSECTION) else { continue };
            let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
            match cj::make_intersection(&mut self.doc, Some(id), &name, d.inputs, d.ground, d.example, "Refreshed") {
                Ok(_) => done.push(format!("Refreshed intersection {name}: {} changed.", why.join(", "))),
                Err(e) => done.push(format!("Intersection {name} is out of date ({}) and cannot be rebuilt: {e}", why.join(", "))),
            }
        }
        for (id, why) in cj::stale_roundabouts(&self.doc) {
            let Some(d) = civil::get::<RoundaboutData>(&self.doc, id, ROUNDABOUT) else { continue };
            let name = self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
            match cj::make_roundabout(&mut self.doc, Some(id), &name, d.inputs, d.example, "Refreshed") {
                Ok(_) => done.push(format!("Refreshed roundabout {name}: {} changed.", why.join(", "))),
                Err(e) => done.push(format!("Roundabout {name} is out of date ({}) and cannot be rebuilt: {e}", why.join(", "))),
            }
        }
        if !done.is_empty() {
            self.status = done.join(" ");
            for m in done {
                self.log(m);
            }
            self.civil_ui.junction.last_dep = crate::civil_cache::dependency_hash(&self.doc);
            self.schedule_commit_now();
        }
    }

    pub(crate) fn junction_windows(&mut self, ctx: &egui::Context) {
        self.junction_refresh_tick();
        self.intersection_window(ctx);
        self.roundabout_window(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a1_tests::demo_doc;
    use afrobim_kernel::Document;

    fn app_with(doc: Document) -> AfroBimApp {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = AfroBimApp::new(&cc, crate::Options { screenshot: Some(std::env::temp_dir().join("afrobim-a1-test.png")), ..crate::Options::default() });
        app.doc = doc;
        app
    }

    fn intersection(doc: &Document) -> (ElementId, IntersectionData) {
        let e = doc.elements().iter().find(|e| e.category == INTERSECTION).expect("the demonstration has an intersection");
        (e.id, civil::get(doc, e.id, INTERSECTION).unwrap())
    }

    #[test]
    fn the_demonstration_junctions_make_real_alignments_with_profiles_at_source_coordinates() {
        let doc = demo_doc();
        let back = Document::from_json(&doc.to_json()).unwrap();
        let (id, d) = intersection(&doc);
        assert_eq!(doc.find(id).unwrap().data, back.find(id).unwrap().data);
        // Edges, returns, kerb lines and the locked secondary are alignment elements of the document.
        assert!(d.created.len() >= 4 + 2 + 3, "{}", d.created.len());
        for c in &d.created {
            let e = doc.find(ElementId(*c)).expect("a created alignment is in the document");
            assert_eq!(e.category, afrobim_alignment::CATEGORY);
        }
        let locked = d.created.iter().filter_map(|c| crate::alignment_glue::alignment_of(&doc, ElementId(*c))).find(|a| a.name.contains("locked")).expect("the locked secondary");
        assert!(locked.profile.is_some(), "the locked secondary profile is a profile object on its alignment");
        // The edges carry profiles, and the kerb return low points are marked (stored for the drawing).
        assert!(d.created.iter().filter_map(|c| crate::alignment_glue::alignment_of(&doc, ElementId(*c))).filter(|a| a.profile.is_some()).count() >= 4);
        // The crossing is at the true UTM coordinates and the element sits there.
        let e = doc.find(id).unwrap();
        assert!((e.origin.x - d.crossing[0] * 1000.0).abs() < 1e-3 && (e.origin.y - d.crossing[1] * 1000.0).abs() < 1e-3);
        assert!(d.crossing[0] > 4.0e5 && d.crossing[1] > 9.0e6);
        // The roundabout has its own alignments and the measured figures.
        let rb = doc.elements().iter().find(|e| e.category == ROUNDABOUT).unwrap();
        let rd: RoundaboutData = civil::get(&doc, rb.id, ROUNDABOUT).unwrap();
        assert!(rd.created.len() >= 4 + 3 * 2 && rd.arms.len() == 2 && rd.arms.iter().all(|a| a.entry_path_radius_m.is_some()));
        // Both scenes draw.
        let ctx = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
        let s = cj::build_intersection_scene(e, &ctx).unwrap();
        assert!(!s.scene.lines.is_empty());
        assert!(!cj::build_roundabout_scene(rb).unwrap().scene.derived.is_empty());
    }

    #[test]
    fn a_changed_source_alignment_refreshes_the_intersection_and_replaces_what_it_made() {
        let mut app = app_with(demo_doc());
        let (id, d0) = intersection(&app.doc);
        assert!(cj::stale_intersections(&app.doc).is_empty());
        // Move the primary road's design profile up by a metre.
        let pid = crate::alignment_glue::alignments(&app.doc).into_iter().find(|(_, n)| n == "Demo road (example)").unwrap().0;
        let mut al = crate::alignment_glue::alignment_of(&app.doc, pid).unwrap();
        let prof = al.profile.as_mut().unwrap();
        for p in &mut prof.pvis {
            p.elevation_m += 1.0;
        }
        app.doc.find_mut(pid).unwrap().data = serde_json::to_value(&al).unwrap();
        let stale = cj::stale_intersections(&app.doc);
        assert_eq!(stale.len(), 1);
        assert!(stale[0].1.iter().any(|s| s.contains("Demo road")), "{:?}", stale[0].1);
        app.junction_refresh_tick();
        assert!(cj::stale_intersections(&app.doc).is_empty(), "rebuilt, so no longer stale");
        let (_, d1) = intersection(&app.doc);
        assert_eq!(d0.created.len(), d1.created.len());
        assert!(d0.created.iter().all(|c| app.doc.find(ElementId(*c)).is_none()), "the old alignments were replaced");
        assert!(d1.created.iter().all(|c| app.doc.find(ElementId(*c)).is_some()));
        assert!(app.status.contains("Refreshed intersection"), "{}", app.status);
        let _ = id;
    }

    #[test]
    fn the_corridor_regions_are_made_through_the_corridor_model() {
        let mut doc = demo_doc();
        let (id, mut d) = intersection(&doc);
        let asm = doc.elements().iter().find(|e| e.category == civil::ASSEMBLY).unwrap().name.clone();
        let a = cj::assemblies_of(&doc);
        assert!(a.iter().any(|x| x.name == asm));
        let ground = doc.elements().iter().find(|e| e.category == civil::SURFACE).unwrap().id;
        d.inputs.corridor = Some(CorridorInputs { name: None, library: a, primary_approach_assembly: asm.clone(), primary_through_assembly: asm.clone(), secondary_approach_assembly: asm.clone(), secondary_mouth_assembly: asm.clone(), return_assembly: asm, approach_frequency: Frequency::uniform(10.0), through_frequency: Frequency::uniform(10.0), mouth_frequency: Frequency::uniform(10.0), return_frequency: Frequency::uniform(5.0), target_step_m: 1.0, end_inset_m: 0.2 });
        let name = doc.find(id).unwrap().name.clone();
        let r = cj::make_intersection(&mut doc, Some(id), &name, d.inputs, Some(ground.0), true, "test");
        match r {
            Ok((id, _)) => {
                let nd: IntersectionData = civil::get(&doc, id, INTERSECTION).unwrap();
                let cid = nd.corridor.expect("a corridor element");
                let cd: civil::CorridorData = civil::get(&doc, ElementId(cid), civil::CORRIDOR).unwrap();
                assert!(cd.model.baselines.len() >= 3, "a baseline per arm and per return: {}", cd.model.baselines.len());
                for b in &cd.model.baselines {
                    assert!(crate::alignment_glue::alignments(&doc).iter().any(|(_, n)| *n == b.alignment), "baseline {} names a real alignment", b.alignment);
                }
            }
            Err(e) => panic!("the corridor description was refused: {e}"),
        }
    }

    #[test]
    fn the_form_reports_what_is_missing_and_skips_quadrants_that_do_not_exist() {
        let doc = demo_doc();
        let (_, d) = intersection(&doc);
        let names: Vec<String> = cj::all_alignments(&doc).into_iter().map(|a| a.name).collect();
        let aligns = cj::all_alignments(&doc);
        let crossings = find_crossings(aligns.iter().find(|a| a.name == d.inputs.primary).unwrap(), aligns.iter().find(|a| a.name == d.inputs.secondary).unwrap(), 0.0);
        let mut f = IntForm::from_inputs(&d.inputs, &names);
        f.rets.iter_mut().for_each(|r| r.on = true);
        // A tee has two quadrants: the other two rows are not read even though ticked.
        let i = f.to_inputs(&names, &[], &crossings).unwrap();
        assert_eq!(i.returns.len(), 2);
        assert_eq!(i.primary_edges.left_offset_m, 3.5);
        // A blank number is named.
        f.p_left.clear();
        assert!(f.to_inputs(&names, &[], &crossings).unwrap_err().contains("left edge offset"));
        // The same road twice is refused.
        let mut g = IntForm::from_inputs(&d.inputs, &names);
        g.secondary = g.primary;
        assert!(g.to_inputs(&names, &[], &crossings).unwrap_err().contains("different"));
    }
}
