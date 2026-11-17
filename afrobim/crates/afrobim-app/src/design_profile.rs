//! `PROFILEDESIGN`: the profile editor window. A profile view of the designed alignment with the
//! ground line along the centreline when a survey surface exists, PVIs to click and drag, the PVI
//! table, a vertical exaggeration control and "Fit to ground".
//!
//! The vertical exaggeration starts blank, which means "fit the window"; the factor in use is
//! shown. Nothing here defaults a grade, a K or a curve length.

use std::collections::HashMap;

use afrobim_annotate::DisplayUnits;
use afrobim_design::{fit_to_ground, CurveKind, CurveSpec, FitInputs, FitResult, VPvi, VerticalDesign, FIT_LABEL};
use afrobim_kernel::{display_spacing, ElementId};
use egui::{pos2, Align2, Color32, FontId, PointerButton, Rect, RichText, Sense, Shape, Stroke, Vec2};

use crate::app::AfroBimApp;
use crate::design_glue::{self as glue, VEdit};
use crate::disp::{cell_changed, cell_text, parse_cell, Cell};
use crate::design_ui::{plan_text, BAD, GOOD, WARN};

/// The entries of "Fit to ground", all text and all blank to start.
#[derive(Default, Clone)]
pub struct FitForm {
    pub spacing: String,
    pub tolerance: String,
    pub min_segment: String,
    pub max_grade_percent: String,
    pub min_curve: String,
    pub k_crest: String,
    pub k_sag: String,
    /// The design speed row of the defaults basis (0 none, i the i-th speed) and the road type of the gradient default.
    pub speed_idx: usize,
    pub road_idx: usize,
}

struct PvDrag {
    pvi: usize,
    working: VerticalDesign,
    refused: Option<String>,
    last_ok: (f64, f64),
}

#[derive(Default)]
pub struct ProfileUi {
    pub open: bool,
    pub exag_text: String,
    pub add_mode: bool,
    pub pending: Vec<(f64, f64)>,
    pub surface: Option<ElementId>,
    drag: Option<PvDrag>,
    pub bufs: HashMap<(usize, u8), String>,
    bufs_for: String,
    modes: HashMap<usize, u8>,
    pub fit_open: bool,
    pub fit: FitForm,
    pub fit_result: Option<Result<FitResult, String>>,
    ground_key: (u64, Option<ElementId>, usize),
    ground: Vec<(f64, Option<f64>)>,
    tin: Option<(ElementId, usize, afrobim_corridor::Tin)>,
    typed: (String, String),
    problem: String,
}

fn parse(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

impl AfroBimApp {
    pub(crate) fn design_open_profile(&mut self) {
        match self.design_target() {
            Some(_) => {
                self.design.profile.open = true;
                if self.design.profile.surface.is_none() {
                    self.design.profile.surface = afrobim_corridor::surfaces(&self.doc).first().map(|s| s.0);
                }
            }
            None => self.status = "There is no designed alignment yet. Make one with ALIGNDESIGN.".into(),
        }
    }

    /// Fills the ground line along the centreline from the chosen surface, when it changed.
    fn profile_ground(&mut self, alignment: &afrobim_alignment::Alignment) {
        let surface = self.design.profile.surface;
        let size = surface.and_then(|s| self.doc.find(s)).map(|e| e.data.to_string().len()).unwrap_or(0);
        let key = (self.design.revision, surface, size);
        if self.design.profile.ground_key == key && !self.design.profile.ground.is_empty() {
            return;
        }
        self.design.profile.ground_key = key;
        self.design.profile.ground.clear();
        let Some(sid) = surface else { return };
        if self.design.profile.tin.as_ref().is_none_or(|(id, n, _)| *id != sid || *n != size) {
            self.design.profile.tin = afrobim_corridor::surface_of(&self.doc, sid).map(|t| (sid, size, t));
        }
        let Some((_, _, tin)) = &self.design.profile.tin else { return };
        let (s0, s1) = (alignment.start_station_m, alignment.end_station_m());
        let n = (((s1 - s0) / 2.5).ceil() as usize).clamp(20, 800);
        for i in 0..=n {
            let s = s0 + (s1 - s0) * i as f64 / n as f64;
            let z = alignment.point_at(s).and_then(|p| tin.elevation_at(p.x, p.y));
            self.design.profile.ground.push((s, z));
        }
    }

    pub(crate) fn profile_window(&mut self, ctx: &egui::Context) {
        let Some(design_id) = self.design.active else {
            self.design.profile.open = false;
            return;
        };
        let Ok(design) = glue::load(&self.doc, design_id) else {
            self.design.profile.open = false;
            return;
        };
        let Ok(built) = design.build() else {
            self.design.profile.open = false;
            return;
        };
        let alignment = built.alignment.clone();
        self.profile_ground(&alignment);
        let surfaces = afrobim_corridor::surfaces(&self.doc);
        let json = serde_json::to_string(&design.vertical).unwrap_or_default();
        if self.design.profile.bufs_for != json {
            self.design.profile.bufs.clear();
            self.design.profile.modes.clear();
            self.design.profile.bufs_for = json;
        }
        let mut open = true;
        let mut edit: Option<VEdit> = None;
        let mut create_from_pending = false;
        let mut accept_fit = false;
        let mut run_fit = false;
        let mut set_surface: Option<Option<ElementId>> = None;
        let name = design.horizontal.name.clone();
        let last = self.design.last.clone();
        let mut ui_state = std::mem::take(&mut self.design.profile);
        let mut ui_state_fit_toggle = false;
        let units = crate::anno_ui::units_of(&self.doc);
        let u = &units;
        let revision = self.design.revision;
        let _ = revision;
        let dlg = crate::ui_kit::Dialog::titled("Profile editor").subtitle("The vertical alignment: points of intersection and curves").width(1020.0).help("Click-drag a PVI to move it, or edit the PVI table; a refused move shows the engine's reason. Choose a design speed and road type to see the K and gradient defaults of the defaults basis with their citation: a default is a starting value you can change. Fit to ground proposes a profile from the ground surface; nothing is inserted until you accept it.").show_close(ctx, |ui| {
            crate::ui_kit::section(ui, &format!("{name}: stations {} to {}", crate::disp::station(u, alignment.start_station_m), crate::disp::station(u, alignment.end_station_m())), |ui| {
                crate::ui_kit::form(ui, "pf-head", |fm| {
                    fm.row("Ground surface", |ui| {
                        let current = ui_state.surface.and_then(|s| surfaces.iter().find(|x| x.0 == s)).map(|s| s.1.clone()).unwrap_or_else(|| "no surface".into());
                        egui::ComboBox::from_id_salt("pf-surface").selected_text(current).width(240.0).show_ui(ui, |ui| {
                            if ui.selectable_label(ui_state.surface.is_none(), "no surface").clicked() {
                                set_surface = Some(None);
                            }
                            for (id, n) in &surfaces {
                                if ui.selectable_label(ui_state.surface == Some(*id), n).clicked() {
                                    set_surface = Some(Some(*id));
                                }
                            }
                        });
                    });
                    fm.row("Vertical exaggeration", |ui| {
                        ui.horizontal(|ui| {
                            crate::ui_kit::num_field(ui, &mut ui_state.exag_text, ": 1 (blank fits)", &crate::ui_kit::NumRule::default().above(0.0));
                            crate::ui_kit::toggle(ui, &mut ui_state.add_mode, "Add PVIs by clicking");
                        });
                    });
                    fm.row("Design speed, road type", |ui| {
                        let speeds = crate::defaults_glue::speeds_of("link.vertical.crest_k_desirable_min");
                        ui.horizontal(|ui| {
                            let mut labels: Vec<String> = vec!["Speed: not chosen".into()];
                            labels.extend(speeds.iter().map(|v| format!("{} km/h", afrobim_defaults::fmt_num(*v))));
                            crate::ui_kit::combo_owned(ui, "fit-speed", &mut ui_state.fit.speed_idx, &labels);
                            crate::ui_kit::combo(ui, "fit-road", &mut ui_state.fit.road_idx, &["Road type: not chosen", "Motorway", "All-purpose dual", "All-purpose single"]);
                        });
                    });
                });
            });
            if surfaces.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "There is no survey surface in this document, so there is no ground line. Import one with SURFACE (LandXML) or make one with SURVEY (a point file).");
            }
            let exag = parse(&ui_state.exag_text).filter(|v| *v > 0.0);
            if !ui_state.exag_text.trim().is_empty() && exag.is_none() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, "The exaggeration must be a number greater than zero; blank fits the window.");
            }
            let shown_exag = profile_plot(ui, &alignment, &design, &ui_state.ground, &mut ui_state.drag, &mut ui_state.pending, ui_state.fit_result.as_ref().and_then(|r| r.as_ref().ok()), exag, ui_state.add_mode, u, &mut edit);
            ui.label(RichText::new(match shown_exag {
                Some(e) => format!("Vertical exaggeration in use: {e:.2} : 1. Click-drag a PVI to move it; refused moves show the engine's reason."),
                None => "No profile to draw yet.".to_string(),
            })
            .small()
            .weak());
            // Pending PVIs of a profile not yet made.
            if design.vertical.is_none() {
                ui.separator();
                crate::ui_kit::heading(ui, "This alignment has no profile yet.");
                ui.label("Add at least two PVIs: switch on \"Add PVIs by clicking\" and click on the plot, or type a station and a level. Nothing is assumed.");
                ui.horizontal(|ui| {
                    ui.label("Station");
                    ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut ui_state.typed.0).hint_text("1+250.000"));
                    ui.label(crate::disp::head(u, "Level"));
                    ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut ui_state.typed.1));
                    if ui.button("Add PVI").clicked() {
                        match (crate::disp::parse_station(u, &ui_state.typed.0), crate::disp::parse_len(u, &ui_state.typed.1)) {
                            (Some(s), Some(z)) => {
                                ui_state.pending.push((s, z));
                                ui_state.pending.sort_by(|a, b| a.0.total_cmp(&b.0));
                                ui_state.problem.clear();
                            }
                            _ => ui_state.problem = "Enter the station (1+250.000 or metres) and the level as a length.".into(),
                        }
                    }
                });
                for (i, p) in ui_state.pending.iter().enumerate() {
                    ui.label(format!("PVI {i}: station {}  level {}", crate::disp::station(u, p.0), crate::disp::len_u(u, p.1)));
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(ui_state.pending.len() >= 2, egui::Button::new("Create the profile from these PVIs")).clicked() {
                        create_from_pending = true;
                    }
                    if ui.button("Clear").clicked() {
                        ui_state.pending.clear();
                    }
                });
            }
            if !ui_state.problem.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &ui_state.problem);
            }
            if ui_state.fit_open {
                ui.separator();
                fit_section(ui, &mut ui_state, &mut run_fit, &mut accept_fit, design.vertical.is_some(), u);
            }
            if let Some(v) = &design.vertical {
                ui.separator();
                match v.solve() {
                    Ok(sol) => pvi_table(ui, v, &sol, &mut ui_state, u, &mut edit),
                    Err(e) => {
                        crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, format!("The profile cannot be solved: {e}"));
                    }
                }
            }
            if let Some(r) = &last {
                ui.label(RichText::new(r.note()).small().color(if r.problems.is_empty() { GOOD() } else { WARN() }));
            }
        }, &["Fit to ground..."]);
        if dlg.inner.as_ref().is_some_and(|(_, sec)| *sec == Some(0)) {
            ui_state_fit_toggle = true;
        }
        if dlg.closed {
            open = false;
        }
        if ui_state_fit_toggle {
            ui_state.fit_open = !ui_state.fit_open;
        }
        self.design.profile = ui_state;
        if let Some(s) = set_surface {
            self.design.profile.surface = s;
            self.design.invalidate();
        }
        if run_fit {
            self.profile_run_fit(&alignment);
        }
        if accept_fit {
            if let Some(Ok(fit)) = self.design.profile.fit_result.take() {
                let mut v = fit.design;
                v.name = design.horizontal.name.clone();
                self.profile_set(Some(v), "Fit to ground accepted: the profile is a starting point only, check every grade and curve.");
            }
        }
        if create_from_pending {
            let pending = std::mem::take(&mut self.design.profile.pending);
            let v = VerticalDesign::new(&design.horizontal.name, pending.iter().map(|p| VPvi::point(p.0, p.1)).collect());
            self.profile_set(Some(v), "Profile made from the PVIs entered. It has no vertical curves yet.");
        }
        if let Some(e) = edit {
            self.profile_apply(e);
        }
        if !open {
            self.design.profile.open = false;
        }
    }

    fn profile_set(&mut self, v: Option<VerticalDesign>, ok: &str) {
        let Some(d) = self.design.active else { return };
        match glue::set_vertical(&mut self.doc, d, v) {
            Ok(r) => {
                self.design_after_edit(r);
                self.log(ok.to_string());
            }
            Err(e) => {
                self.design.profile.problem = format!("Refused: {e}");
                self.status = format!("Refused: {e} Nothing was changed.");
            }
        }
    }

    fn profile_apply(&mut self, edit: VEdit) {
        if self.design.profile.add_mode || !matches!(edit, VEdit::InsertPvi(..)) || self.design.profile.add_mode {
            self.design.profile.problem.clear();
        }
        if !self.design_apply_v(edit) {
            self.design.profile.bufs.clear();
            self.design.profile.bufs_for.clear();
        }
    }

    fn profile_run_fit(&mut self, alignment: &afrobim_alignment::Alignment) {
        let f = self.design.profile.fit.clone();
        let Some(sid) = self.design.profile.surface else {
            self.design.profile.fit_result = Some(Err("Choose a survey surface for the ground first.".into()));
            return;
        };
        let Some(tin) = afrobim_corridor::surface_of(&self.doc, sid) else {
            self.design.profile.fit_result = Some(Err("That surface cannot be read.".into()));
            return;
        };
        let u = crate::anno_ui::units_of(&self.doc);
        let number = |what: &str, text: &str| parse(text).ok_or_else(|| format!("Enter the {what} as a number."));
        let length = |what: &str, text: &str| crate::disp::parse_len(&u, text).ok_or_else(|| format!("Enter the {what} as a length (a number in {}, or with m or mm).", crate::disp::suffix(&u)));
        let inputs = (|| -> Result<FitInputs, String> {
            Ok(FitInputs {
                sample_spacing_m: length("sample spacing", &f.spacing)?,
                tolerance_m: length("tolerance", &f.tolerance)?,
                min_segment_length_m: length("minimum segment length", &f.min_segment)?,
                max_grade: number("maximum grade (%)", &f.max_grade_percent)? / 100.0,
                min_curve_length_m: length("minimum curve length", &f.min_curve)?,
                // K is a length per percent: a bare number is in the display length unit per percent.
                k_crest: if f.k_crest.trim().is_empty() { None } else { Some(length("K for crests", &f.k_crest)?) },
                k_sag: if f.k_sag.trim().is_empty() { None } else { Some(length("K for sags", &f.k_sag)?) },
            })
        })();
        self.design.profile.fit_result = Some(inputs.and_then(|i| fit_to_ground(alignment, &tin, &i).map_err(|e| e.to_string())));
    }
}

fn fit_section(ui: &mut egui::Ui, st: &mut ProfileUi, run: &mut bool, accept: &mut bool, replaces: bool, u: &DisplayUnits) {
    crate::ui_kit::section(ui, "Fit to ground", |ui| {
        crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Proposes a profile from the ground by a least squares fit of straight segments. You enter every input: nothing is assumed, and the fit knows nothing of drainage, cover, sight distance, earthwork balance or any standard.");
    });
    let suffix = crate::disp::suffix(u);
    let speeds = crate::defaults_glue::speeds_of("link.vertical.crest_k_desirable_min");
    let speed = (st.fit.speed_idx > 0).then(|| speeds.get(st.fit.speed_idx - 1).copied()).flatten();
    let cond = speed.map(|v| afrobim_defaults::Conditions::none().design_speed(v));
    let k_factor = if u.length == afrobim_annotate::LengthUnit::Millimetre { 1000.0 } else { 1.0 };
    let roads = ["motorway", "all_purpose_dual", "all_purpose_single"];
    let road_key = (st.fit.road_idx > 0).then(|| afrobim_defaults::Conditions::none().label("road_type", roads[(st.fit.road_idx - 1).min(2)]));
    let rule = crate::ui_kit::NumRule::default();
    let req = crate::ui_kit::NumRule::required().above(0.0);
    let f = &mut st.fit;
    crate::ui_kit::form(ui, "fit-form", |fm| {
        fm.row("Sample spacing", |ui| {
            crate::ui_kit::num_field(ui, &mut f.spacing, suffix, &req);
        });
        fm.row("Tolerance on the ground", |ui| {
            crate::ui_kit::num_field(ui, &mut f.tolerance, suffix, &req);
        });
        fm.row("Minimum segment length", |ui| {
            crate::ui_kit::num_field(ui, &mut f.min_segment, suffix, &req);
        });
        fm.row("Minimum curve length", |ui| {
            crate::ui_kit::num_field(ui, &mut f.min_curve, suffix, &req);
        });
        fm.row_note("K for crests", "Optional; the design speed above gives the default", |ui| match &cond {
            Some(c) => {
                crate::defaults_glue::field(ui, "fit-kc", &mut f.k_crest, &format!("{suffix} per %"), "link.vertical.crest_k_desirable_min", c, k_factor, &rule);
            }
            None => {
                crate::ui_kit::num_field(ui, &mut f.k_crest, &format!("{suffix} per %"), &rule);
            }
        });
        fm.row_note("K for sags", "Optional; the design speed above gives the default", |ui| match &cond {
            Some(c) => {
                crate::defaults_glue::field(ui, "fit-ks", &mut f.k_sag, &format!("{suffix} per %"), "link.vertical.sag_k_desirable_min", c, k_factor, &rule);
            }
            None => {
                crate::ui_kit::num_field(ui, &mut f.k_sag, &format!("{suffix} per %"), &rule);
            }
        });
        fm.row_note("Maximum grade", "The road type above gives the default", |ui| match &road_key {
            Some(r) => {
                crate::defaults_glue::field(ui, "fit-grade", &mut f.max_grade_percent, "%", "link.gradient.desirable_max", r, 1.0, &req);
            }
            None => {
                crate::ui_kit::num_field(ui, &mut f.max_grade_percent, "%", &req);
            }
        });
    });
    if ui.button("Run the fit").clicked() {
        *run = true;
    }
    match &st.fit_result {
        Some(Err(e)) => {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, e);
        }
        Some(Ok(fit)) => {
            egui::Frame::new().fill(Color32::from_rgb(0x5A, 0x3A, 0x10)).inner_margin(8.0).corner_radius(4.0).show(ui, |ui| {
                ui.label(RichText::new(FIT_LABEL).strong().size(14.0).color(Color32::from_rgb(0xFF, 0xD0, 0x80)));
            });
            ui.label(format!(
                "{} PVIs proposed (drawn dashed in orange). Largest height above the ground {}, largest depth below it {}, RMS difference {}.",
                fit.design.pvis.len(),
                crate::disp::len_u(u, fit.max_above_ground_m),
                crate::disp::len_u(u, fit.max_below_ground_m),
                crate::disp::len_u(u, fit.rms_difference_m)
            ));
            if !fit.gaps.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("{} stretch(es) where the centreline is off the surface were left out.", fit.gaps.len()));
            }
            for n in &fit.notes {
                ui.label(RichText::new(n).small());
            }
            ui.horizontal(|ui| {
                let text = if replaces { "Accept: replace the current profile" } else { "Accept: use this profile" };
                if ui.button(text).clicked() {
                    *accept = true;
                }
                if ui.button("Discard").clicked() {
                    st.fit_result = None;
                }
            });
            crate::ui_kit::note(ui, "Nothing is inserted until you accept.");
        }
        None => {}
    }
}

/// Draws the profile and handles clicks and drags. Returns the exaggeration in use.
#[allow(clippy::too_many_arguments)]
fn profile_plot(
    ui: &mut egui::Ui,
    alignment: &afrobim_alignment::Alignment,
    design: &afrobim_design::AlignmentDesign,
    ground: &[(f64, Option<f64>)],
    drag: &mut Option<PvDrag>,
    pending: &mut Vec<(f64, f64)>,
    fit: Option<&FitResult>,
    exag: Option<f64>,
    add_mode: bool,
    u: &DisplayUnits,
    edit: &mut Option<VEdit>,
) -> Option<f64> {
    let width = ui.available_width().max(300.0);
    let (response, painter) = ui.allocate_painter(Vec2::new(width, 310.0), Sense::click_and_drag());
    let rect = response.rect;
    let dark = ui.visuals().dark_mode;
    painter.rect_filled(rect, 3.0, if dark { Color32::from_rgb(0x1B, 0x1F, 0x25) } else { Color32::from_rgb(0xF4, 0xF4, 0xF0) });
    let ink = if dark { Color32::from_gray(235) } else { Color32::from_gray(30) };
    let weak = if dark { Color32::from_rgb(0x9A, 0xA2, 0xAC) } else { Color32::from_rgb(0x55, 0x5B, 0x63) };
    let (s0, s1) = (alignment.start_station_m, alignment.end_station_m());
    let solved = design.vertical.as_ref().and_then(|v| v.solve().ok());
    let shown_design = drag.as_ref().map(|d| d.working.clone()).or_else(|| design.vertical.clone());
    let shown = shown_design.as_ref().and_then(|v| v.solve().ok());

    // The levels the plot must hold.
    let mut zs: Vec<f64> = ground.iter().filter_map(|g| g.1).collect();
    if let Some(sol) = &shown {
        for k in 0..=100 {
            if let (Some((a, b)), true) = (sol.profile.range(), true) {
                if let Ok(z) = sol.level_at(a + (b - a) * k as f64 / 100.0) {
                    zs.push(z);
                }
            }
        }
    }
    zs.extend(pending.iter().map(|p| p.1));
    if let Some(f) = fit {
        zs.extend(f.design.pvis.iter().map(|p| p.elevation_m));
    }
    if zs.is_empty() {
        painter.text(rect.center(), Align2::CENTER_CENTER, "Click Add PVIs by clicking, then click here to start a profile", FontId::proportional(13.0), weak);
        zs = vec![0.0, 10.0];
    }
    let (zmin, zmax) = zs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), z| (a.min(*z), b.max(*z)));
    let pad = ((zmax - zmin) * 0.1).max(1.0);
    let (zlo, zhi) = (zmin - pad, zmax + pad);
    let plot = Rect::from_min_max(rect.min + Vec2::new(58.0, 12.0), rect.max - Vec2::new(14.0, 26.0));
    let xs = plot.width() as f64 / (s1 - s0).max(1e-6);
    let fit_ys = plot.height() as f64 / (zhi - zlo).max(1e-6);
    let ys = match exag {
        Some(e) => xs * e,
        None => fit_ys,
    };
    let zmid = (zlo + zhi) / 2.0;
    let x_of = |s: f64| plot.left() + ((s - s0) * xs) as f32;
    let y_of = |z: f64| plot.center().y - ((z - zmid) * ys) as f32;
    let station_of = |x: f32| s0 + (x - plot.left()) as f64 / xs;
    let level_of = |y: f32| zmid + (plot.center().y - y) as f64 / ys;
    let clipped = painter.with_clip_rect(plot.expand(1.0));

    // Grid and scales.
    let grid = if dark { Color32::from_rgb(0x2A, 0x30, 0x38) } else { Color32::from_rgb(0xDD, 0xDD, 0xD6) };
    let sx = display_spacing(10.0, xs, 70.0);
    let mut s = (s0 / sx).ceil() * sx;
    while s <= s1 {
        painter.line_segment([pos2(x_of(s), plot.top()), pos2(x_of(s), plot.bottom())], Stroke::new(1.0_f32, grid));
        painter.text(pos2(x_of(s), plot.bottom() + 3.0), Align2::CENTER_TOP, afrobim_alignment::format_station(s, 0), FontId::proportional(10.0), weak);
        s += sx;
    }
    let sy = display_spacing(0.5, ys, 30.0);
    let mut z = ((zmid - plot.height() as f64 / ys / 2.0) / sy).ceil() * sy;
    while z <= zmid + plot.height() as f64 / ys / 2.0 {
        painter.line_segment([pos2(plot.left(), y_of(z)), pos2(plot.right(), y_of(z))], Stroke::new(1.0_f32, grid));
        painter.text(pos2(plot.left() - 4.0, y_of(z)), Align2::RIGHT_CENTER, crate::disp::axis(u, z), FontId::proportional(10.0), weak);
        z += sy;
    }
    painter.rect_stroke(plot, 0.0, Stroke::new(1.0_f32, weak), egui::StrokeKind::Inside);

    // Ground line, in pieces where the surface exists.
    let ground_colour = Color32::from_rgb(0xB0, 0x82, 0x4A);
    let mut piece: Vec<egui::Pos2> = Vec::new();
    for (s, z) in ground {
        match z {
            Some(z) => piece.push(pos2(x_of(*s), y_of(*z))),
            None => {
                if piece.len() > 1 {
                    clipped.add(Shape::line(std::mem::take(&mut piece), Stroke::new(1.5_f32, ground_colour)));
                }
                piece.clear();
            }
        }
    }
    if piece.len() > 1 {
        clipped.add(Shape::line(piece, Stroke::new(1.5_f32, ground_colour)));
    }
    if !ground.is_empty() {
        painter.text(plot.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, "ground along the centreline", FontId::proportional(10.0), ground_colour);
    }

    // The profile and its PVIs.
    if let (Some(sol), Some(v)) = (&shown, &shown_design) {
        if let Some((a, b)) = sol.profile.range() {
            let n = (((b - a) / 1.5).ceil() as usize).clamp(10, 1500);
            let pts: Vec<egui::Pos2> = (0..=n).filter_map(|k| {
                let s = a + (b - a) * k as f64 / n as f64;
                sol.level_at(s).ok().map(|z| pos2(x_of(s), y_of(z)))
            }).collect();
            clipped.add(Shape::line(pts, Stroke::new(2.0_f32, ink)));
        }
        for c in &sol.curves {
            for (s, z) in [(c.pvc_station_m, c.pvc_elevation_m), (c.pvt_station_m, c.pvt_elevation_m)] {
                clipped.circle_filled(pos2(x_of(s), y_of(z)), 2.5, Color32::from_rgb(0x4D, 0xA3, 0xFF));
            }
        }
        for (i, p) in v.pvis.iter().enumerate() {
            let at = pos2(x_of(p.station_m), y_of(p.elevation_m));
            let hot = drag.as_ref().is_some_and(|d| d.pvi == i);
            let r = Rect::from_center_size(at, Vec2::splat(11.0));
            clipped.rect_filled(r, 0.0, if hot { Color32::from_rgb(0xFF, 0x5A, 0x36) } else { Color32::from_rgb(0x2F, 0x7F, 0xE0) });
            clipped.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
            plan_text(&clipped, at + Vec2::new(8.0, -10.0), Align2::LEFT_BOTTOM, &format!("PVI {i}  {}", crate::disp::len(u, p.elevation_m)), 10.0, ink, dark);
        }
    }
    if let Some(f) = fit {
        if let Ok(sol) = f.design.solve() {
            if let Some((a, b)) = sol.profile.range() {
                let n = (((b - a) / 3.0).ceil() as usize).clamp(10, 800);
                let pts: Vec<egui::Pos2> = (0..=n).filter_map(|k| {
                    let s = a + (b - a) * k as f64 / n as f64;
                    sol.level_at(s).ok().map(|z| pos2(x_of(s), y_of(z)))
                }).collect();
                clipped.extend(Shape::dashed_line(&pts, Stroke::new(2.0_f32, Color32::from_rgb(0xFF, 0xA0, 0x30)), 8.0, 5.0));
                painter.text(plot.right_top() + Vec2::new(-6.0, 4.0), Align2::RIGHT_TOP, "STARTING POINT ONLY (not accepted)", FontId::proportional(11.0), Color32::from_rgb(0xFF, 0xA0, 0x30));
            }
        }
    }
    for (i, p) in pending.iter().enumerate() {
        let at = pos2(x_of(p.0), y_of(p.1));
        clipped.rect_filled(Rect::from_center_size(at, Vec2::splat(10.0)), 0.0, Color32::from_rgb(0xFF, 0xB0, 0x30));
        plan_text(&clipped, at + Vec2::new(8.0, -10.0), Align2::LEFT_BOTTOM, &format!("PVI {i}"), 10.0, ink, dark);
    }

    // Pointer.
    let hover = response.hover_pos();
    if let Some(h) = hover.filter(|h| plot.contains(*h)) {
        painter.text(rect.left_bottom() + Vec2::new(62.0, -2.0), Align2::LEFT_BOTTOM, format!("station {}   level {}", crate::disp::station(u, station_of(h.x)), crate::disp::len_u(u, level_of(h.y))), FontId::proportional(10.5), weak);
        ui.ctx().set_cursor_icon(if add_mode { egui::CursorIcon::Crosshair } else { egui::CursorIcon::Default });
    }
    let pvi_at = |pos: egui::Pos2| design.vertical.as_ref().and_then(|v| v.pvis.iter().position(|p| pos2(x_of(p.station_m), y_of(p.elevation_m)).distance(pos) <= 9.0));
    if response.drag_started_by(PointerButton::Primary) {
        if let (Some(o), Some(v)) = (ui.input(|i| i.pointer.press_origin()), design.vertical.as_ref()) {
            if let Some(i) = pvi_at(o) {
                *drag = Some(PvDrag { pvi: i, working: v.clone(), refused: None, last_ok: (v.pvis[i].station_m, v.pvis[i].elevation_m) });
            }
        }
    }
    if let (Some(pos), Some(base)) = (response.interact_pointer_pos(), design.vertical.as_ref()) {
        if drag.is_some() && response.dragged_by(PointerButton::Primary) {
            let (s, z) = (station_of(pos.x), level_of(pos.y));
            let pi = drag.as_ref().map(|d| d.pvi).unwrap_or(0);
            let result = glue::try_vertical(base, VEdit::MovePvi(pi, s, z));
            if let Some(d) = drag.as_mut() {
                match result {
                    Ok(v) => {
                        d.working = v;
                        d.refused = None;
                        d.last_ok = (s, z);
                    }
                    Err(e) => d.refused = Some(e),
                }
            }
        }
    }
    if response.drag_stopped_by(PointerButton::Primary) {
        if let Some(d) = drag.take() {
            if d.refused.is_none() {
                *edit = Some(VEdit::MovePvi(d.pvi, d.last_ok.0, d.last_ok.1));
            }
        }
    }
    if let Some(d) = drag.as_ref().and_then(|d| d.refused.as_ref()) {
        plan_text(&painter, plot.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, &format!("Refused: {d}"), 11.0, BAD(), dark);
    }
    if add_mode && response.clicked_by(PointerButton::Primary) {
        if let Some(pos) = response.interact_pointer_pos().filter(|p| plot.contains(*p)) {
            let (s, z) = (station_of(pos.x), level_of(pos.y));
            match design.vertical.as_ref() {
                Some(v) => {
                    let index = v.pvis.iter().position(|p| p.station_m > s).unwrap_or(v.pvis.len());
                    *edit = Some(VEdit::InsertPvi(index, s, z));
                }
                None => {
                    pending.push((s, z));
                    pending.sort_by(|a, b| a.0.total_cmp(&b.0));
                }
            }
        }
    }
    let _ = solved;
    Some(ys / xs)
}

/// The PVI table: station, level, grades, curve by length or K, and the high or low point.
fn pvi_table(ui: &mut egui::Ui, v: &VerticalDesign, sol: &afrobim_design::VerticalSolution, st: &mut ProfileUi, u: &DisplayUnits, edit: &mut Option<VEdit>) {
    crate::ui_kit::heading(ui, "PVI table");
    let k_speeds = crate::defaults_glue::speeds_of("link.vertical.crest_k_desirable_min");
    let k_cond = (st.fit.speed_idx > 0).then(|| k_speeds.get(st.fit.speed_idx - 1).copied()).flatten().map(|v| afrobim_defaults::Conditions::none().design_speed(v));
    let k_unit = if u.length == afrobim_annotate::LengthUnit::Millimetre { 1000.0 } else { 1.0 };
    ui.label(RichText::new(format!("Stations are shown as 1+250.000 and may be typed that way or as a number of metres. Lengths and levels are in the document's display unit ({}; a trailing m or mm overrides it when typing). Grades are in percent. Give a vertical curve by its length or by K ({} of curve per 1 % change of grade); the other is worked out. No K and no length are assumed.", crate::disp::suffix(u), crate::disp::suffix(u))).small().weak());
    egui::ScrollArea::both().id_salt("pvi-table").max_height(190.0).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("pvi-grid").num_columns(11).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            let heads = ["PVI".to_string(), "Station".to_string(), crate::disp::head(u, "Level"), "Grade in %".to_string(), "Grade out %".to_string(), "Curve by".to_string(), "Value".to_string(), crate::disp::head(u, "L"), crate::disp::k_head(u), "High or low point".to_string(), String::new()];
            for h in heads {
                crate::ui_kit::th(ui, &h);
            }
            ui.end_row();
            for (i, p) in v.pvis.iter().enumerate() {
                ui.label(format!("{i}"));
                for (k, value) in [(0u8, p.station_m), (1u8, p.elevation_m)] {
                    let text = st.bufs.entry((i, k)).or_insert_with(|| cell_text(u, if k == 0 { Cell::PviStation } else { Cell::PviLevel }, value));
                    let r = ui.add_sized([86.0, 20.0], egui::TextEdit::singleline(text));
                    if r.lost_focus() {
                        let typed = |k: u8, c: Cell| st.bufs.get(&(i, k)).and_then(|t| parse_cell(u, c, t).ok().flatten());
                        match (typed(0, Cell::PviStation), typed(1, Cell::PviLevel)) {
                            (Some(s), Some(z)) => {
                                if cell_changed(u, Cell::PviStation, p.station_m, Some(s)) || cell_changed(u, Cell::PviLevel, p.elevation_m, Some(z)) {
                                    *edit = Some(VEdit::MovePvi(i, s, z));
                                }
                            }
                            _ => st.problem = format!("PVI {i}: the station must be 1+250.000 or metres and the level a length."),
                        }
                    }
                }
                let gin = if i > 0 { sol.grades.get(i - 1).map(|g| g.grade * 100.0) } else { None };
                let gout = sol.grades.get(i).map(|g| g.grade * 100.0);
                ui.label(gin.map(|g| format!("{g:.3}")).unwrap_or_default());
                ui.label(gout.map(|g| format!("{g:.3}")).unwrap_or_default());
                let interior = i > 0 && i + 1 < v.pvis.len();
                let curve = sol.curves.iter().find(|c| c.pvi_index == i);
                if interior {
                    let current_mode = match p.curve {
                        CurveSpec::None => 0u8,
                        CurveSpec::Length { .. } => 1,
                        CurveSpec::K { .. } => 2,
                    };
                    let mode = *st.modes.entry(i).or_insert(current_mode);
                    let mut new_mode = mode;
                    egui::ComboBox::from_id_salt(("vmode", i)).width(70.0).selected_text(["none", "length", "K"][mode as usize]).show_ui(ui, |ui| {
                        for (m, l) in ["none", "length", "K"].iter().enumerate() {
                            ui.selectable_value(&mut new_mode, m as u8, *l);
                        }
                    });
                    if new_mode != mode {
                        st.modes.insert(i, new_mode);
                        if new_mode == 0 {
                            *edit = Some(VEdit::NoCurve(i));
                        }
                    }
                    if new_mode != 0 {
                        let value = match p.curve {
                            CurveSpec::Length { length_m } if new_mode == 1 => cell_text(u, Cell::CurveLength, length_m),
                            CurveSpec::K { k } if new_mode == 2 => cell_text(u, Cell::CurveK, k),
                            _ => String::new(),
                        };
                        let text = st.bufs.entry((i, 2 + new_mode)).or_insert(value);
                        let r = ui.add_sized([70.0, 20.0], egui::TextEdit::singleline(text).hint_text(if new_mode == 1 { crate::disp::suffix(u).to_string() } else { format!("{} per %", crate::disp::suffix(u)) }));
                        if new_mode == 2 {
                            if let (Some(c), Some(g_in), Some(g_out)) = (&k_cond, gin, gout) {
                                let key = if g_out < g_in { "link.vertical.crest_k_desirable_min" } else { "link.vertical.sag_k_desirable_min" };
                                if let Some(n) = crate::defaults_glue::get(key, c).and_then(|v| v.number()) {
                                    let t = afrobim_defaults::fmt_num(n * k_unit);
                                    if ui.small_button("Default").on_hover_text(format!("{} K of the basis for a {}: {t} {} per %", "Desirable minimum", if g_out < g_in { "crest" } else { "sag" }, crate::disp::suffix(u))).clicked() {
                                        text.clone_from(&t);
                                        if let Some(x) = parse_cell(u, Cell::CurveK, &t).ok().flatten() {
                                            *edit = Some(VEdit::CurveK(i, x));
                                        }
                                    }
                                }
                            }
                        }
                        if r.lost_focus() {
                            let t = st.bufs.get(&(i, 2 + new_mode)).cloned().unwrap_or_default();
                            match parse_cell(u, if new_mode == 1 { Cell::CurveLength } else { Cell::CurveK }, &t).ok().flatten() {
                                Some(x) => *edit = Some(if new_mode == 1 { VEdit::CurveLength(i, x) } else { VEdit::CurveK(i, x) }),
                                None => st.problem = format!("PVI {i}: the curve {} must be a length greater than zero.", if new_mode == 1 { "length" } else { "K (a length per percent)" }),
                            }
                        }
                    } else {
                        ui.label("");
                    }
                } else {
                    ui.label("");
                    ui.label("");
                }
                ui.label(curve.map(|c| crate::disp::len(u, c.length_m)).unwrap_or_default());
                ui.label(curve.map(|c| crate::disp::k(u, c.k)).unwrap_or_default());
                ui.label(match curve.and_then(|c| c.turning_point.map(|t| (c.kind, t))) {
                    Some((kind, (s, z))) => format!("{} {} / {}", if kind == CurveKind::Crest { "high" } else { "low" }, crate::disp::station(u, s), crate::disp::len(u, z)),
                    None => String::new(),
                });
                ui.horizontal(|ui| {
                    if ui.small_button("Del").clicked() {
                        *edit = Some(VEdit::DeletePvi(i));
                    }
                });
                ui.end_row();
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label("Add a PVI at station");
        ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut st.typed.0).hint_text("1+250.000"));
        ui.label("level");
        ui.add_sized([90.0, 20.0], egui::TextEdit::singleline(&mut st.typed.1).hint_text("blank: on the profile"));
        if ui.button("Add PVI").clicked() {
            match (crate::disp::parse_station(u, &st.typed.0), st.typed.1.trim().is_empty()) {
                (Some(s), true) => *edit = Some(VEdit::InsertAt(s)),
                (Some(s), false) => match crate::disp::parse_len(u, &st.typed.1) {
                    Some(z) => {
                        let index = v.pvis.iter().position(|p| p.station_m > s).unwrap_or(v.pvis.len());
                        *edit = Some(VEdit::InsertPvi(index, s, z));
                    }
                    None => st.problem = "The level must be a length.".into(),
                },
                _ => st.problem = "Enter the station as 1+250.000 or as a number of metres.".into(),
            }
        }
    });
    for w in &sol.warnings {
        crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, w);
    }
}
