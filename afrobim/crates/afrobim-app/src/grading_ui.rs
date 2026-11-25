// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The grading commands: feature lines (create by picking, from an alignment and profile, draped on a
//! surface, stepped offset), the feature line editor (elevation table with grades, quick edit, fillet,
//! join, break, weed), sites and their crossings, the grading creation dialog, grading groups with
//! volumes, raise and lower and auto-balance, and the helpers (pad, pond, ditch or berm).
//! The calculations are the `afrobim-grading` engine's.

use std::sync::Arc;

use afrobim_corridor::Tin;
use afrobim_grading::*;
use std::result::Result;
use afrobim_kernel::{ElementId, SelectMode};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::background::Job;
use crate::civil::{self, FEATURELINE, GRADING};
use crate::civil_dlg::{self as d, fmt};
use crate::civil_grading::{self as cg, FeatureLineData, GradingData};
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

#[derive(Default)]
pub struct GradeUi {
    pub fl_new: Option<FlNew>,
    pub fl_edit: Option<FlEdit>,
    pub site: Option<SiteForm>,
    pub create: Option<GradeNew>,
    pub group: Option<GroupForm>,
    pub group_out: Option<GroupOut>,
    pub helpers: Option<HelpForm>,
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

fn lines_combo(ui: &mut egui::Ui, id: &str, doc: &afrobim_kernel::Document, sel: &mut Option<ElementId>) {
    let items = civil::of_category(doc, FEATURELINE);
    d::pick_from(ui, id, &items, sel);
}

/// A slope as an input: a ratio (horizontal per 1 vertical) or a grade in percent.
#[derive(Clone, Default)]
pub struct SlopeIn {
    pub percent: bool,
    pub v: String,
}

impl SlopeIn {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut i = self.percent as usize;
            ui_kit::segmented(ui, &mut i, &["h : 1", "%"]);
            self.percent = i == 1;
            d::num_rule(ui, &mut self.v, if self.percent { "%" } else { "h per 1 v" }, NumRule::required().above(0.0));
        });
    }
    fn get(&self, what: &str) -> Result<Slope, String> {
        let v = num(what, &self.v)?;
        if v <= 0.0 {
            return Err(format!("{what} must be greater than zero."));
        }
        Ok(if self.percent { Slope::GradePercent(v) } else { Slope::Ratio(v) })
    }
}

// ─── CREATE A FEATURE LINE ──────────────────────────────────────────────────

pub struct FlNew {
    mode: usize,
    name: String,
    site: String,
    closed: bool,
    rows: Vec<(String, String, String)>,
    surface: Option<ElementId>,
    add_points: bool,
    alignment: Option<ElementId>,
    s0: String,
    s1: String,
    interval: String,
    source: Option<ElementId>,
    offset: String,
    grade_step: bool,
    step: String,
    facet_deg: String,
    message: String,
}

const FL_MODES: [&str; 4] = ["By picking", "From an alignment and profile", "Draped on a surface", "Stepped offset"];

impl AfroBimApp {
    pub(crate) fn grading_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        let example = self.options.civil_demo;
        match cv {
            FeatureLine => {
                let site = cg::sites(&self.doc).first().cloned().unwrap_or_else(|| "Site 1".into());
                self.civil_ui.grade.fl_new = Some(FlNew { mode: 0, name: String::new(), site, closed: false, rows: vec![Default::default(); 2], surface: civil::surface_choices(&self.doc).first().map(|s| s.0), add_points: false, alignment: crate::alignment_glue::alignments(&self.doc).first().map(|a| a.0), s0: String::new(), s1: String::new(), interval: if example { "10".into() } else { String::new() }, source: self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == FEATURELINE)), offset: String::new(), grade_step: false, step: String::new(), facet_deg: if example { "5".into() } else { String::new() }, message: String::new() });
            }
            FeatureLineEdit => {
                if civil::of_category(&self.doc, FEATURELINE).is_empty() {
                    self.status = "There is no feature line yet: make one with FEATURELINE first.".into();
                    return;
                }
                let line = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == FEATURELINE)).or_else(|| civil::of_category(&self.doc, FEATURELINE).first().map(|l| l.0));
                self.civil_ui.grade.fl_edit = Some(FlEdit::new(line));
            }
            GradeSite => {
                let sites = cg::sites(&self.doc);
                if sites.is_empty() {
                    self.status = "There is no feature line yet: make one with FEATURELINE first.".into();
                    return;
                }
                let sel = self.doc.primary().and_then(|id| civil::get::<FeatureLineData>(&self.doc, id, FEATURELINE)).map(|d| d.site).unwrap_or_else(|| sites[0].clone());
                self.civil_ui.grade.site = Some(SiteForm { site: sel, facet_deg: if example { "5".into() } else { String::new() }, priorities: vec![], loaded: String::new(), report: vec![], message: String::new() });
            }
            GradeCreate => {
                if civil::of_category(&self.doc, FEATURELINE).is_empty() {
                    self.status = "There is no feature line yet: make one with FEATURELINE first.".into();
                    return;
                }
                let fp = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == FEATURELINE)).or_else(|| civil::of_category(&self.doc, FEATURELINE).first().map(|l| l.0));
                self.civil_ui.grade.create = Some(GradeNew::new(fp, civil::surface_choices(&self.doc).first().map(|s| s.0), example));
            }
            GradeGroup => {
                if civil::of_category(&self.doc, GRADING).is_empty() {
                    self.status = "There is no grading group yet: make one with GRADING first.".into();
                    return;
                }
                let g = self.doc.primary().filter(|id| self.doc.find(*id).is_some_and(|e| e.category == GRADING)).or_else(|| civil::of_category(&self.doc, GRADING).first().map(|l| l.0));
                self.civil_ui.grade.group = Some(GroupForm { group: g, base: None, cut_factor: String::new(), fill_factor: String::new(), step: String::new(), target: String::new(), min_shift: String::new(), max_shift: String::new(), tolerance: String::new(), max_iter: String::new(), message: String::new() });
            }
            GradeHelpers => {
                self.civil_ui.grade.helpers = Some(HelpForm::new(self.options.civil_demo, civil::surface_choices(&self.doc).first().map(|s| s.0)));
            }
            _ => {}
        }
    }

    fn fl_new_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.grade.fl_new.take() else { return };
        for (e, n) in std::mem::take(&mut self.civil_ui.queue) {
            let row = (format!("{e:.3}"), format!("{n:.3}"), f.surface.and_then(|s| self.tin_for(s)).and_then(|t| t.elevation_at(e, n)).map(|z| format!("{z:.3}")).unwrap_or_default());
            match f.rows.iter().position(|r| r.0.trim().is_empty() && r.1.trim().is_empty()) {
                Some(i) => f.rows[i] = row,
                None => f.rows.push(row),
            }
        }
        let surfaces = civil::surface_choices(&self.doc);
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let sites = cg::sites(&self.doc);
        let mut go = false;
        let mut pick = false;
        let busy_pick = self.civil_ui.pick.is_some();
        let demo = self.options.civil_demo;
        let out = Dialog::new("Create feature line", crate::glyphs::by_name("FEATURELINE")).width(680.0).resizable(true).max_body_height(600.0).help("A feature line is a 3D line made of PIs (plan geometry) and elevation points. Lines of the same site share one elevation where they cross. Pick the PIs in the model with their levels typed or read from a surface, take a line from an alignment and its design profile, drape a plan line on a surface, or offset another feature line by a distance and a level difference or grade.").show(
            ctx,
            |ui| {
                if demo {
                    ui_kit::callout(ui, Level::Info, "Some fields hold example values for the demonstration: they are not from any standard or design.");
                }
                ui_kit::tabs(ui, &mut f.mode, &FL_MODES);
                ui_kit::form(ui, "fl-head", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Platform edge"));
                    fm.row_note("Site", "Lines of one site share elevations at crossings", |ui| {
                        ui.horizontal(|ui| {
                            d::text(ui, &mut f.site, "site name");
                            if !sites.is_empty() {
                                let mut i = sites.iter().position(|s| *s == f.site).map(|i| i + 1).unwrap_or(0);
                                let mut labels = vec!["(typed)".to_string()];
                                labels.extend(sites.iter().cloned());
                                if ui_kit::combo_owned(ui, "fl-site", &mut i, &labels) && i > 0 {
                                    f.site = sites[i - 1].clone();
                                }
                            }
                        });
                    });
                });
                match f.mode {
                    0 | 2 => {
                        ui_kit::section(ui, "Points, in order", |ui| {
                            egui::Grid::new("fl-rows").num_columns(5).spacing([6.0, 4.0]).show(ui, |ui| {
                                for h in ["#", "Easting (m)", "Northing (m)", if f.mode == 0 { "Level (m)" } else { "(level from the surface)" }, ""] {
                                    ui.label(RichText::new(h).small().strong());
                                }
                                ui.end_row();
                                let mut remove = None;
                                for (i, r) in f.rows.iter_mut().enumerate() {
                                    ui.label((i + 1).to_string());
                                    ui.add(egui::TextEdit::singleline(&mut r.0).desired_width(100.0));
                                    ui.add(egui::TextEdit::singleline(&mut r.1).desired_width(100.0));
                                    ui.add_enabled(f.mode == 0, egui::TextEdit::singleline(&mut r.2).desired_width(80.0).hint_text("from surface"));
                                    if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this point").clicked() {
                                        remove = Some(i);
                                    }
                                    ui.end_row();
                                }
                                if let Some(i) = remove {
                                    f.rows.remove(i);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Add a row").clicked() {
                                    f.rows.push(Default::default());
                                }
                                if ui.selectable_label(busy_pick, "Pick in the model").clicked() {
                                    pick = true;
                                }
                            });
                            if f.mode == 0 {
                                ui_kit::toggle(ui, &mut f.closed, "Closed (the last point joins the first)");
                            }
                        });
                        ui_kit::form(ui, "fl-surf", |fm| {
                            fm.row_note("Surface", if f.mode == 0 { "Gives the level of a point whose level is blank" } else { "The line is draped on it" }, |ui| {
                                d::pick_from(ui, "fl-surface", &surfaces, &mut f.surface);
                            });
                            if f.mode == 2 {
                                fm.row("Elevation points", |ui| {
                                    ui_kit::toggle(ui, &mut f.add_points, "Add one wherever the line crosses a triangle edge");
                                });
                            }
                        });
                    }
                    1 => {
                        ui_kit::form(ui, "fl-al", |fm| {
                            fm.row("Alignment", |ui| {
                                d::pick_from(ui, "fl-align", &aligns, &mut f.alignment);
                            });
                            fm.row_note("From station", "Blank: the start", |ui| {
                                d::num_opt(ui, &mut f.s0, "m").ok();
                            });
                            fm.row_note("To station", "Blank: the end", |ui| {
                                d::num_opt(ui, &mut f.s1, "m").ok();
                            });
                            fm.row_note("Sample interval", "Plan points and levels are taken at this spacing and at every PVI", |ui| {
                                d::num_rule(ui, &mut f.interval, "m", NumRule::required().above(0.0));
                            });
                        });
                        ui_kit::note(ui, "The levels are those of the alignment's design profile. A vertical curve is followed by the sample points, not exactly.");
                    }
                    _ => {
                        ui_kit::form(ui, "fl-off", |fm| {
                            fm.row("Offset from", |ui| lines_combo(ui, "fl-source", &self.doc, &mut f.source));
                            fm.row_note("Offset", "Positive to the left of the line", |ui| {
                                d::num(ui, &mut f.offset, "m");
                            });
                            fm.row("Elevation step", |ui| {
                                ui_kit::segmented(ui, &mut (f.grade_step as usize), &["Difference", "Grade"]);
                            });
                            fm.row(if f.grade_step { "Grade across the offset" } else { "Level difference" }, |ui| {
                                d::num(ui, &mut f.step, if f.grade_step { "%" } else { "m" });
                            });
                            fm.row_note("Facet angle", "Arcs are flattened to straight spans at this angle", |ui| {
                                d::num_rule(ui, &mut f.facet_deg, "deg", NumRule::required().above(0.0));
                            });
                        });
                    }
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Create feature line").icon(crate::glyphs::by_name("FEATURELINE")).show(ui);
                go = o.primary;
                o
            },
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = true;
            self.status = "Click the points of the feature line in the model, in order.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if go {
            match self.fl_create(&f) {
                Ok(m) => {
                    self.civil_ui.pick = None;
                    self.status = m;
                    return;
                }
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.grade.fl_new = Some(f);
    }

    fn fl_create(&mut self, f: &FlNew) -> Result<String, String> {
        let name = if f.name.trim().is_empty() { "Feature line".to_string() } else { f.name.trim().to_string() };
        let site = if f.site.trim().is_empty() { "Site 1".to_string() } else { f.site.trim().to_string() };
        let mut note = String::new();
        let tin: Option<Arc<Tin>> = f.surface.and_then(|s| self.tin_for(s));
        let line = match f.mode {
            0 => {
                let mut pts = Vec::new();
                for (i, r) in f.rows.iter().enumerate().filter(|(_, r)| !r.0.trim().is_empty() || !r.1.trim().is_empty()) {
                    let (e, n) = (num(&format!("Easting of row {}", i + 1), &r.0)?, num(&format!("Northing of row {}", i + 1), &r.1)?);
                    let z = match opt("A level", &r.2)? {
                        Some(z) => z,
                        None => tin.as_ref().and_then(|t| t.elevation_at(e, n)).ok_or_else(|| format!("Row {} has no level and the surface does not cover it.", i + 1))?,
                    };
                    pts.push([e, n, z]);
                }
                if f.closed { FeatureLine::closed_from_points(&name, &pts) } else { FeatureLine::from_points(&name, &pts) }.map_err(|e| e.to_string())?
            }
            1 => {
                let al = f.alignment.and_then(|id| crate::alignment_glue::alignment_of(&self.doc, id)).ok_or("Choose an alignment.")?;
                let prof = al.profile.as_ref().ok_or("The alignment has no design profile.")?;
                let interval = num("The interval", &f.interval)?;
                if interval <= 0.0 {
                    return Err("The interval must be greater than zero.".into());
                }
                let s0 = opt("The from station", &f.s0)?.unwrap_or(al.start_station_m);
                let s1 = opt("The to station", &f.s1)?.unwrap_or(al.end_station_m());
                if s1 <= s0 {
                    return Err("The to station must be above the from station.".into());
                }
                let mut stations = vec![s0];
                let mut s = s0 + interval;
                while s < s1 - 1e-9 {
                    stations.push(s);
                    s += interval;
                }
                stations.extend(prof.pvis.iter().map(|p| p.station_m).filter(|p| *p > s0 + 1e-6 && *p < s1 - 1e-6));
                stations.push(s1);
                stations.sort_by(|a, b| a.partial_cmp(b).unwrap());
                stations.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                let plan: Vec<[f64; 2]> = stations.iter().map(|s| al.point_at(*s).map(|p| [p.x, p.y]).ok_or_else(|| format!("Station {s:.3} is off the alignment."))).collect::<Result<_, _>>()?;
                let profile: Vec<[f64; 2]> = stations.iter().map(|s| prof.elevation_at(*s).map(|z| [*s, z]).map_err(|e| format!("The profile at station {s:.3}: {e}"))).collect::<Result<_, _>>()?;
                FeatureLine::from_alignment_profile(&name, &plan, &stations, &profile).map_err(|e| e.to_string())?
            }
            2 => {
                let t = tin.as_ref().ok_or("Choose a surface to drape on.")?;
                let mut plan = Vec::new();
                for (i, r) in f.rows.iter().enumerate().filter(|(_, r)| !r.0.trim().is_empty() || !r.1.trim().is_empty()) {
                    plan.push([num(&format!("Easting of row {}", i + 1), &r.0)?, num(&format!("Northing of row {}", i + 1), &r.1)?]);
                }
                let (line, rep) = FeatureLine::from_polyline_on_surface(&name, &plan, t, f.add_points).map_err(|e| e.to_string())?;
                note = format!(" {} level(s) read from the surface, {} point(s) added.{}", rep.vertices_set, rep.points_added, rep.warnings.first().map(|w| format!(" {w}")).unwrap_or_default());
                line
            }
            _ => {
                let src = f.source.and_then(|id| civil::get::<FeatureLineData>(&self.doc, id, FEATURELINE)).ok_or("Choose the feature line to offset.")?;
                let step = if f.grade_step { ElevationStep::GradePercent(num("The grade", &f.step)?) } else { ElevationStep::Difference(num("The level difference", &f.step)?) };
                src.line.offset_stepped(num("The offset", &f.offset)?, step, num("The facet angle", &f.facet_deg)?.to_radians()).map_err(|e| e.to_string())?
            }
        };
        let mut line = line;
        line.last_edited = cg::next_edit_counter(&self.doc, &site);
        let id = cg::add_line(&mut self.doc, &name, line, &site, FL_MODES[f.mode], false);
        self.doc.select(&[id], SelectMode::Replace);
        self.schedule_commit_now();
        Ok(format!("Created feature line '{name}' in site '{site}'.{note}"))
    }
}

// ─── THE FEATURE LINE EDITOR ────────────────────────────────────────────────

pub struct FlEdit {
    line: Option<ElementId>,
    loaded: Option<ElementId>,
    z: Vec<String>,
    g: Vec<String>,
    z0: Vec<String>,
    g0: Vec<String>,
    tab: usize,
    pi: String,
    radius: String,
    other: Option<ElementId>,
    tol: String,
    station: String,
    delta: String,
    plan_tol: String,
    elev_tol: String,
    span: String,
    vertex: String,
    ins_station: String,
    ins_z: String,
    surface: Option<ElementId>,
    add_points: bool,
    message: String,
}

impl FlEdit {
    fn new(line: Option<ElementId>) -> FlEdit {
        FlEdit { line, loaded: None, z: vec![], g: vec![], z0: vec![], g0: vec![], tab: 0, pi: String::new(), radius: String::new(), other: None, tol: String::new(), station: String::new(), delta: String::new(), plan_tol: String::new(), elev_tol: String::new(), span: String::new(), vertex: String::new(), ins_station: String::new(), ins_z: String::new(), surface: None, add_points: false, message: String::new() }
    }

    fn load(&mut self, data: &FeatureLineData) {
        let grades = data.line.grade_table().unwrap_or_default();
        self.z = data.line.vertices.iter().map(|v| format!("{:.4}", v.z)).collect();
        self.z0 = self.z.clone();
        self.g = (0..data.line.vertices.len()).map(|i| grades.iter().find(|r| r.from == i).map(|r| format!("{:.4}", r.grade_percent)).unwrap_or_default()).collect();
        self.g0 = self.g.clone();
    }
}

impl AfroBimApp {
    fn fl_edit_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.grade.fl_edit.take() else { return };
        let lines = civil::of_category(&self.doc, FEATURELINE);
        let data = f.line.and_then(|id| civil::get::<FeatureLineData>(&self.doc, id, FEATURELINE));
        if f.line != f.loaded {
            if let Some(d) = &data {
                f.load(d);
            }
            f.loaded = f.line;
        }
        let surfaces = civil::surface_choices(&self.doc);
        let mut action = 0u8;
        let mut csv = false;
        let out = Dialog::new("Feature line editor", crate::glyphs::by_name("FLEDIT")).width(860.0).resizable(true).max_body_height(640.0).help("The table lists every vertex with its level and the grade to the next one. Change a level or a grade and press Apply: a grade holds the start of the segment and moves the next level. The tools below change the line itself; a change that cannot be made is refused with the reason and nothing is changed.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "fe-head", |fm| {
                    fm.row("Feature line", |ui| {
                        d::pick_from(ui, "fe-line", &lines, &mut f.line);
                    });
                });
                let Some(dt) = &data else { return };
                let rows = dt.line.grade_table().unwrap_or_default();
                ui.label(RichText::new(format!("Site {}   {} vertices   length {} m{}", dt.site, dt.line.vertices.len(), dt.line.length().map(|l| fmt(l, 3)).unwrap_or_default(), if dt.line.closed { "   closed" } else { "" })).small().weak());
                egui::ScrollArea::vertical().id_salt("fe-table").max_height(250.0).show(ui, |ui| {
                    egui::Grid::new("fe-grid").striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                        for h in ["#", "Kind", "Easting", "Northing", "Level (m)", "Grade to next (%)", "Station", "Slope h:1"] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        for (i, v) in dt.line.vertices.iter().enumerate() {
                            ui.label(i.to_string());
                            ui.label(if v.kind == VertexKind::Pi { "PI" } else { "elev." });
                            ui.label(fmt(v.x, 3));
                            ui.label(fmt(v.y, 3));
                            if i < f.z.len() {
                                ui.add(egui::TextEdit::singleline(&mut f.z[i]).desired_width(80.0));
                            }
                            if i < f.g.len() {
                                let has = rows.iter().any(|r| r.from == i);
                                ui.add_enabled(has, egui::TextEdit::singleline(&mut f.g[i]).desired_width(80.0));
                            }
                            let r = rows.iter().find(|r| r.from == i);
                            ui.label(r.map(|r| fmt(r.station_from, 3)).unwrap_or_default());
                            ui.label(r.and_then(|r| r.slope_h_per_v).map(|s| fmt(s, 2)).unwrap_or_default());
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("Apply the table edits").clicked() {
                        action = 1;
                    }
                    if ui.button("Grade table as CSV...").clicked() {
                        csv = true;
                    }
                });
                ui_kit::tabs(ui, &mut f.tab, &["Shape", "Combine", "Levels"]);
                ui_kit::form(ui, "fe-tools", |fm| match f.tab {
                    0 => {
                        fm.row_note("Fillet at PI", "PI vertex number and radius", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.pi, "vertex");
                                d::num_rule(ui, &mut f.radius, "m", NumRule::default().above(0.0)).map(|_| ());
                                if ui.button("Fillet").clicked() {
                                    action = 2;
                                }
                            });
                        });
                        fm.row_note("Weed", "Removes vertices that change the line by less than the tolerances", |ui| {
                            ui.horizontal(|ui| {
                                d::num_rule(ui, &mut f.plan_tol, "m plan", NumRule::default().above(0.0));
                                d::num_rule(ui, &mut f.elev_tol, "m level", NumRule::default().above(0.0));
                                d::num_rule(ui, &mut f.span, "m span", NumRule::default().above(0.0));
                                if ui.button("Weed").clicked() {
                                    action = 3;
                                }
                            });
                        });
                        fm.row_note("Delete a vertex", "", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.vertex, "vertex");
                                if ui.button("Delete").clicked() {
                                    action = 4;
                                }
                            });
                        });
                        fm.row("Reverse", |ui| {
                            if ui.button("Reverse the direction").clicked() {
                                action = 5;
                            }
                        });
                    }
                    1 => {
                        fm.row_note("Join to", "The end of this line to the start of the other", |ui| {
                            ui.horizontal(|ui| {
                                d::pick_from(ui, "fe-other", &lines, &mut f.other);
                                d::num_rule(ui, &mut f.tol, "m gap", NumRule::default().min(0.0));
                                if ui.button("Join").clicked() {
                                    action = 6;
                                }
                            });
                        });
                        fm.row_note("Break at station", "Splits into two lines", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.station, "station");
                                if ui.button("Break").clicked() {
                                    action = 7;
                                }
                            });
                        });
                    }
                    _ => {
                        fm.row_note("Raise or lower all", "Metres, negative to lower", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.delta, "m");
                                if ui.button("Apply").clicked() {
                                    action = 8;
                                }
                            });
                        });
                        fm.row_note("Insert an elevation point", "Station along the line and level", |ui| {
                            ui.horizontal(|ui| {
                                d::text(ui, &mut f.ins_station, "station");
                                d::text(ui, &mut f.ins_z, "level");
                                if ui.button("Insert").clicked() {
                                    action = 9;
                                }
                            });
                        });
                        fm.row_note("Levels from a surface", "", |ui| {
                            ui.horizontal(|ui| {
                                d::pick_from(ui, "fe-surf", &surfaces, &mut f.surface);
                                ui_kit::toggle(ui, &mut f.add_points, "add points at edges");
                                if ui.button("Set levels").clicked() {
                                    action = 10;
                                }
                            });
                        });
                    }
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
        );
        if csv {
            if let Some(dt) = &data {
                if let Ok(rows) = dt.line.grade_table() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("grade-table.csv").save_file() {
                        self.status = match std::fs::write(&p, grade_table_csv(&rows)) {
                            Ok(()) => format!("Wrote {}.", p.display()),
                            Err(e) => format!("The file could not be written: {e}"),
                        };
                    }
                }
            }
        }
        if action > 0 {
            f.message = match self.fl_edit_apply(&mut f, action) {
                Ok(m) => m,
                Err(e) => format!("Refused: {e} Nothing was changed."),
            };
            f.loaded = None;
        }
        if out.closed {
            return;
        }
        self.civil_ui.grade.fl_edit = Some(f);
    }

    pub(crate) fn fl_edit_apply(&mut self, f: &mut FlEdit, action: u8) -> Result<String, String> {
        let id = f.line.ok_or("Choose a feature line.")?;
        let mut data = civil::get::<FeatureLineData>(&self.doc, id, FEATURELINE).ok_or("The feature line cannot be read.")?;
        let counter = cg::next_edit_counter(&self.doc, &data.site);
        let idx = |what: &str, t: &str| t.trim().parse::<usize>().map_err(|_| format!("{what} is not a vertex number."));
        let msg: String = match action {
            1 => {
                let mut rows = Vec::new();
                for i in 0..data.line.vertices.len() {
                    let (z, g) = (f.z.get(i).cloned().unwrap_or_default(), f.g.get(i).cloned().unwrap_or_default());
                    let z_changed = f.z0.get(i) != Some(&z);
                    let g_changed = f.g0.get(i) != Some(&g) && !g.trim().is_empty();
                    if z_changed || g_changed {
                        rows.push(QuickEdit { vertex: i, elevation: if z_changed { Some(num("A level", &z)?) } else { None }, grade_to_next_percent: if g_changed { Some(num("A grade", &g)?) } else { None } });
                    }
                }
                if rows.is_empty() {
                    return Err("No level or grade was changed.".into());
                }
                data.line.quick_edit(&rows).map_err(|e| e.to_string())?;
                format!("Applied {} edit(s).", rows.len())
            }
            2 => {
                let r = data.line.fillet(idx("The PI", &f.pi)?, num("The radius", &f.radius)?).map_err(|e| e.to_string())?;
                format!("Fillet: deflection {:.2} deg, tangent length {:.3} m, arc length {:.3} m, {} elevation point(s) removed.", r.deflection.to_degrees(), r.tangent_length, r.arc_length, r.elevation_points_removed)
            }
            3 => {
                let r = data.line.weed(WeedTolerances { plan_distance: num("The plan tolerance", &f.plan_tol)?, elevation: num("The level tolerance", &f.elev_tol)?, max_span_length: num("The span", &f.span)? }).map_err(|e| e.to_string())?;
                format!("Weeded: {} PI(s) and {} elevation point(s) removed.", r.pis_removed, r.elevation_points_removed)
            }
            4 => {
                data.line.delete_vertex(idx("The vertex", &f.vertex)?).map_err(|e| e.to_string())?;
                "Deleted the vertex.".into()
            }
            5 => {
                data.line.reverse();
                "Reversed.".into()
            }
            6 => {
                let other = f.other.and_then(|o| civil::get::<FeatureLineData>(&self.doc, o, FEATURELINE)).ok_or("Choose the line to join.")?;
                let j = data.line.join(&other.line, num("The gap tolerance", &f.tol)?).map_err(|e| e.to_string())?;
                data.line = j.line;
                let other_id = f.other.unwrap();
                self.doc.remove(other_id);
                format!("Joined: gap {:.4} m, level mismatch {:.4} m. The other line was merged into this one.", j.gap, j.elevation_mismatch)
            }
            7 => {
                let parts = data.line.break_at(num("The station", &f.station)?).map_err(|e| e.to_string())?;
                if parts.len() < 2 {
                    return Err("The break gave one line.".into());
                }
                let mut first = parts[0].clone();
                first.last_edited = counter;
                data.line = first;
                let mut second = parts[1].clone();
                second.last_edited = counter;
                let name = format!("{} (2)", self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default());
                cg::add_line(&mut self.doc, &name, second, &data.site, "Broken from a feature line", false);
                format!("Broke the line in two; the second part is '{name}'.")
            }
            8 => {
                data.line.raise_lower(num("The change", &f.delta)?).map_err(|e| e.to_string())?;
                "Raised or lowered the whole line.".into()
            }
            9 => {
                data.line.insert_elevation_point(num("The station", &f.ins_station)?, num("The level", &f.ins_z)?).map_err(|e| e.to_string())?;
                "Inserted an elevation point.".into()
            }
            _ => {
                let t = f.surface.and_then(|s| self.tin_for(s)).ok_or("Choose a surface.")?;
                let r = data.line.set_elevations_from_surface(&t, f.add_points).map_err(|e| e.to_string())?;
                format!("Levels read from the surface for {} vertex(es), {} point(s) added.", r.vertices_set, r.points_added)
            }
        };
        data.line.last_edited = counter;
        cg::store_line(&mut self.doc, id, &data);
        self.schedule_commit_now();
        Ok(msg)
    }
}

// ─── SITES ──────────────────────────────────────────────────────────────────

pub struct SiteForm {
    site: String,
    facet_deg: String,
    priorities: Vec<String>,
    loaded: String,
    report: Vec<CrossingReport>,
    message: String,
}

impl AfroBimApp {
    fn site_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.grade.site.take() else { return };
        let sites = cg::sites(&self.doc);
        let lines = cg::lines_of(&self.doc, &f.site);
        if f.loaded != f.site || f.priorities.len() != lines.len() {
            f.priorities = lines.iter().map(|l| l.2.line.style_priority.to_string()).collect();
            f.loaded = f.site.clone();
            f.report.clear();
        }
        let mut resolve = false;
        let mut apply = false;
        let mut csv = false;
        let out = Dialog::new("Sites and crossings", crate::glyphs::by_name("SITE")).width(820.0).resizable(true).max_body_height(600.0).help("Feature lines of one site share a single elevation where they cross in plan. The line with the higher style priority wins; with equal priority the line edited last wins; with both equal the line made first wins and the report says it was a tie. Every crossing is reported with both levels before and after.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "st-head", |fm| {
                    fm.row("Site", |ui| {
                        let mut i = sites.iter().position(|s| *s == f.site).unwrap_or(0);
                        if ui_kit::combo_owned(ui, "st-site", &mut i, &sites) {
                            f.site = sites[i].clone();
                        }
                    });
                    fm.row_note("Facet angle", "Arcs are flattened at this angle to find crossings", |ui| {
                        d::num_rule(ui, &mut f.facet_deg, "deg", NumRule::required().above(0.0));
                    });
                });
                ui_kit::section(ui, "Feature lines of the site", |ui| {
                    egui::Grid::new("st-lines").striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                        for h in ["Name", "Style priority", "Edited (counter)", "Length (m)", "Closed"] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        for (i, (_, name, dt)) in lines.iter().enumerate() {
                            ui.label(name);
                            if let Some(p) = f.priorities.get_mut(i) {
                                ui.add(egui::TextEdit::singleline(p).desired_width(50.0));
                            }
                            ui.label(dt.line.last_edited.to_string());
                            ui.label(dt.line.length().map(|l| fmt(l, 2)).unwrap_or_default());
                            ui.label(if dt.line.closed { "yes" } else { "no" });
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("Apply the priorities").clicked() {
                        apply = true;
                    }
                    if ui.button("Resolve the crossings").clicked() {
                        resolve = true;
                    }
                    if ui.add_enabled(!f.report.is_empty(), egui::Button::new("Crossings as CSV...")).clicked() {
                        csv = true;
                    }
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
                if !f.report.is_empty() {
                    let rows: Vec<Vec<String>> = f.report.iter().map(|c| vec![c.line_a.clone(), c.line_b.clone(), fmt(c.x, 3), fmt(c.y, 3), fmt(c.z_a, 3), fmt(c.z_b, 3), fmt(c.z_used, 3), c.winner.clone(), format!("{:?}", c.rule), if c.changed { "yes".into() } else { "no".into() }]).collect();
                    egui::ScrollArea::both().max_height(220.0).show(ui, |ui| {
                        ui_kit::results_table(ui, "st-report", &["Line A", "Line B", "Easting", "Northing", "Level A", "Level B", "Level used", "Winner", "Rule", "Changed"], &[Col::Text, Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Text, Col::Text, Col::Text], &rows);
                    });
                }
            },
        );
        if apply {
            for (i, (id, _, dt)) in lines.iter().enumerate() {
                if let Some(p) = f.priorities.get(i).and_then(|p| p.trim().parse::<i32>().ok()) {
                    if p != dt.line.style_priority {
                        let mut n = dt.clone();
                        n.line.style_priority = p;
                        cg::store_line(&mut self.doc, *id, &n);
                    }
                }
            }
            f.message = "Priorities applied.".into();
            self.schedule_commit_now();
        }
        if resolve {
            f.message = match self.site_resolve(&mut f) {
                Ok(m) => m,
                Err(e) => format!("Refused: {e}"),
            };
        }
        if csv {
            if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("crossings.csv").save_file() {
                self.status = match std::fs::write(&p, crossings_csv(&f.report)) {
                    Ok(()) => format!("Wrote {}.", p.display()),
                    Err(e) => format!("The file could not be written: {e}"),
                };
            }
        }
        if out.closed {
            return;
        }
        self.civil_ui.grade.site = Some(f);
    }

    pub(crate) fn site_resolve(&mut self, f: &mut SiteForm) -> Result<String, String> {
        let facet = num("The facet angle", &f.facet_deg)?.to_radians();
        let mut site = cg::site_from(self.doc.elements(), &f.site);
        let before: Vec<(String, u64)> = site.feature_lines.iter().map(|l| (l.name.clone(), l.fingerprint())).collect();
        let report = site.resolve_crossings(facet).map_err(|e| e.to_string())?;
        let mut changed = 0;
        for l in &site.feature_lines {
            if before.iter().any(|(n, fp)| *n == l.name && *fp != l.fingerprint()) {
                if let Some((id, _, mut dt)) = cg::lines_of(&self.doc, &f.site).into_iter().find(|(_, n, _)| *n == l.name) {
                    dt.line = l.clone();
                    cg::store_line(&mut self.doc, id, &dt);
                    changed += 1;
                }
            }
        }
        let n = report.len();
        f.report = report;
        self.schedule_commit_now();
        Ok(format!("{n} crossing(s) found; {changed} line(s) changed."))
    }
}

// ─── CREATE A GRADING ───────────────────────────────────────────────────────

pub struct GradeNew {
    name: String,
    footprint: Option<ElementId>,
    /// An existing group to add the grading to, or a new one.
    group: Option<ElementId>,
    side: usize,
    kind: usize,
    cut: SlopeIn,
    fill: SlopeIn,
    max_distance: String,
    distance: String,
    direction: usize,
    elevation: String,
    delta: String,
    from: String,
    to: String,
    ground: Option<ElementId>,
    ray_spacing: String,
    facet_deg: String,
    tick: String,
    infill: bool,
    message: String,
}

impl GradeNew {
    fn new(fp: Option<ElementId>, ground: Option<ElementId>, example: bool) -> GradeNew {
        let ex = |s: &str| if example { s.to_string() } else { String::new() };
        GradeNew { name: String::new(), footprint: fp, group: None, side: 0, kind: 0, cut: SlopeIn { percent: false, v: ex("2") }, fill: SlopeIn { percent: false, v: ex("3") }, max_distance: ex("60"), distance: String::new(), direction: 0, elevation: String::new(), delta: String::new(), from: String::new(), to: String::new(), ground, ray_spacing: ex("2"), facet_deg: ex("10"), tick: ex("2"), infill: false, message: String::new() }
    }
}

const CRITERIA: [&str; 4] = ["Target surface (daylight)", "Target distance", "Target elevation", "Relative elevation"];

impl AfroBimApp {
    fn grade_new_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.grade.create.take() else { return };
        let surfaces = civil::surface_choices(&self.doc);
        let groups = civil::of_category(&self.doc, GRADING);
        let mut go = false;
        let side_note = crate::defaults_glue::get("earthworks.side_slopes.no_value", &afrobim_defaults::Conditions::none());
        let demo = self.options.civil_demo;
        let out = Dialog::new("Create grading", crate::glyphs::by_name("GRADING")).width(700.0).resizable(true).max_body_height(620.0).help("A grading projects slopes from a feature line footprint, on the side you choose, until it meets the target. Choose the criterion and its slopes. Cut and fill slopes, the ray spacing and the facet angle are inputs.").show(
            ctx,
            |ui| {
                if demo {
                    ui_kit::callout(ui, Level::Info, "Some fields hold example values for the demonstration: they are not from any standard or design.");
                }
                ui_kit::form(ui, "gn-head", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Platform batter"));
                    fm.row("Footprint", |ui| lines_combo(ui, "gn-fp", &self.doc, &mut f.footprint));
                    fm.row("Side", |ui| {
                        ui_kit::segmented(ui, &mut f.side, &["Left of the line", "Right of the line"]);
                    });
                    fm.row_note("Group", "A grading group builds one surface", |ui| {
                        let mut items = vec![(ElementId(u64::MAX), "A new group".to_string())];
                        items.extend(groups.iter().cloned());
                        let mut sel = Some(f.group.unwrap_or(ElementId(u64::MAX)));
                        if d::pick_from(ui, "gn-group", &items, &mut sel) {
                            f.group = sel.filter(|s| s.0 != u64::MAX);
                        }
                    });
                    fm.row_note("Target surface", "Ground the slopes daylight to", |ui| {
                        d::pick_from(ui, "gn-ground", &surfaces, &mut f.ground);
                    });
                });
                ui_kit::section(ui, "Criterion", |ui| {
                    ui_kit::combo(ui, "gn-crit", &mut f.kind, &CRITERIA);
                    ui_kit::form(ui, "gn-crit-f", |fm| match f.kind {
                        0 => {
                            fm.row("Cut slope", |ui| f.cut.ui(ui));
                            fm.row("Fill slope", |ui| f.fill.ui(ui));
                            fm.row_note("Search distance", "Greatest distance from the footprint a ray looks for the ground", |ui| {
                                d::num_rule(ui, &mut f.max_distance, "m", NumRule::required().above(0.0));
                            });
                        }
                        1 => {
                            fm.row("Distance", |ui| {
                                d::num_rule(ui, &mut f.distance, "m", NumRule::required().above(0.0));
                            });
                            fm.row("Slope", |ui| f.fill.ui(ui));
                            fm.row("Direction", |ui| {
                                ui_kit::segmented(ui, &mut f.direction, &["Rise", "Fall"]);
                            });
                        }
                        2 => {
                            fm.row("Elevation", |ui| {
                                d::num(ui, &mut f.elevation, "m");
                            });
                            fm.row_note("Cut slope", "Used where the target is above the footprint", |ui| f.cut.ui(ui));
                            fm.row_note("Fill slope", "Used where the target is below", |ui| f.fill.ui(ui));
                        }
                        _ => {
                            fm.row_note("Elevation change", "Relative to the footprint, negative for down", |ui| {
                                d::num(ui, &mut f.delta, "m");
                            });
                            fm.row("Slope", |ui| f.fill.ui(ui));
                        }
                    });
                    match &side_note {
                        Some(n) => {
                            ui_kit::callout(ui, Level::Info, format!("No default side slope: {} ({})", crate::defaults_glue::summary_of(&n.value), crate::defaults_glue::citation_line(n)));
                        }
                        None => {
                            ui_kit::note(ui, "No defaults basis is chosen: the slopes are inputs.");
                        }
                    }
                });
                ui_kit::section(ui, "Range and projection", |ui| {
                    ui_kit::form(ui, "gn-proj", |fm| {
                        fm.row_note("From station", "Along the footprint; blank for the whole line", |ui| {
                            d::num_opt(ui, &mut f.from, "m").ok();
                        });
                        fm.row_note("To station", "", |ui| {
                            d::num_opt(ui, &mut f.to, "m").ok();
                        });
                        fm.row_note("Ray spacing", "Rays are at most this far apart along the footprint", |ui| {
                            d::num_rule(ui, &mut f.ray_spacing, "m", NumRule::required().above(0.0));
                        });
                        fm.row_note("Facet angle", "Rays are at most this far apart in direction on arcs and corners", |ui| {
                            d::num_rule(ui, &mut f.facet_deg, "deg", NumRule::required().above(0.0));
                        });
                        fm.row_note("Slope ticks", "Spacing of the slope pattern in plan; blank for none", |ui| {
                            d::num_opt_rule(ui, &mut f.tick, "m", &NumRule::default().above(0.0)).ok();
                        });
                        fm.row("Infill", |ui| {
                            ui_kit::toggle(ui, &mut f.infill, "Fill a closed footprint with a surface");
                        });
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Create grading").icon(crate::glyphs::by_name("GRADING")).show(ui);
                go = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if go {
            match self.grade_create(&f) {
                Ok(()) => return,
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.grade.create = Some(f);
    }

    fn grade_create(&mut self, f: &GradeNew) -> Result<(), String> {
        let fp_id = f.footprint.ok_or("Choose the footprint feature line.")?;
        let fp = civil::get::<FeatureLineData>(&self.doc, fp_id, FEATURELINE).ok_or("The footprint cannot be read.")?;
        let fp_name = self.doc.find(fp_id).map(|e| e.name.clone()).unwrap_or_default();
        let crit = match f.kind {
            0 => Criterion::TargetSurface { cut_slope: f.cut.get("The cut slope")?, fill_slope: f.fill.get("The fill slope")?, max_distance: num("The search distance", &f.max_distance)? },
            1 => Criterion::TargetDistance { distance: num("The distance", &f.distance)?, slope: f.fill.get("The slope")?, direction: if f.direction == 0 { Direction::Rise } else { Direction::Fall } },
            2 => Criterion::TargetElevation { elevation: num("The elevation", &f.elevation)?, cut_slope: f.cut.get("The cut slope")?, fill_slope: f.fill.get("The fill slope")? },
            _ => Criterion::TargetRelativeElevation { delta: num("The elevation change", &f.delta)?, slope: f.fill.get("The slope")? },
        };
        crit.validate().map_err(|e| e.to_string())?;
        if f.kind == 0 && f.ground.is_none() {
            return Err("A target surface criterion needs a target surface.".into());
        }
        let name = if f.name.trim().is_empty() { format!("{fp_name} grading") } else { f.name.trim().to_string() };
        let mut g = Grading::uniform(&name, &fp_name, if f.side == 0 { Side::Left } else { Side::Right }, crit);
        if let (Some(a), Some(b)) = (opt("The from station", &f.from)?, opt("The to station", &f.to)?) {
            g = g.with_range(a, b);
        }
        let params = ProjectionParams { ray_spacing: num("The ray spacing", &f.ray_spacing)?, facet_angle: num("The facet angle", &f.facet_deg)?.to_radians() };
        params.validate().map_err(|e| e.to_string())?;
        let tick = opt("The tick spacing", &f.tick)?;
        match f.group {
            Some(gid) => {
                let mut data = civil::get::<GradingData>(&self.doc, gid, GRADING).ok_or("The group cannot be read.")?;
                data.group.gradings.push(g);
                if f.infill && fp.line.closed {
                    data.group.infills.push(Infill::Footprint { footprint: fp_name.clone() });
                }
                self.store_grading(gid, &data);
                self.doc.select(&[gid], SelectMode::Replace);
                self.status = format!("Added grading '{name}' to the group. The surface rebuilds in the background.");
            }
            None => {
                let mut group = GradingGroup::new(format!("{name} group"), params);
                group.gradings.push(g);
                if f.infill && fp.line.closed {
                    group.infills.push(Infill::Footprint { footprint: fp_name.clone() });
                }
                let data = GradingData { group, site: fp.site.clone(), ground: f.ground.map(|g| g.0), tick_spacing_m: tick, base: f.ground.map(|g| g.0), example: false };
                let id = self.add_grading(&format!("{name} group"), &data, "Grading dialog");
                self.doc.select(&[id], SelectMode::Replace);
                self.status = format!("Created grading group '{name} group'. Its surface builds in the background and is listed under Surfaces.");
            }
        }
        self.schedule_commit_now();
        Ok(())
    }

    pub(crate) fn add_grading(&mut self, name: &str, data: &GradingData, source: &str) -> ElementId {
        cg::add_grading_element(&mut self.doc, name, data, source)
    }

    pub(crate) fn store_grading(&mut self, id: ElementId, data: &GradingData) {
        cg::store_grading_element(&mut self.doc, id, data);
    }
}

// ─── GROUPS, VOLUMES, BALANCE ───────────────────────────────────────────────

pub struct GroupForm {
    group: Option<ElementId>,
    base: Option<ElementId>,
    cut_factor: String,
    fill_factor: String,
    step: String,
    target: String,
    min_shift: String,
    max_shift: String,
    tolerance: String,
    max_iter: String,
    message: String,
}

pub struct GroupOut {
    pub group: ElementId,
    pub name: String,
    pub volume: Option<Result<(afrobim_terrain::VolumeResult, AdjustedVolume), String>>,
    pub balance: Option<Result<BalanceResult, String>>,
}

pub enum GradeJob {
    Volume(GroupOut),
    Balance(GroupOut, f64),
}

impl AfroBimApp {
    fn grade_group_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.grade.group.take() else { return };
        let groups = civil::of_category(&self.doc, GRADING);
        let surfaces = civil::surface_choices(&self.doc);
        let data = f.group.and_then(|g| civil::get::<GradingData>(&self.doc, g, GRADING));
        let busy = self.bg.grade.is_some();
        let mut act = 0u8;
        let out_ref = self.civil_ui.grade.group_out.as_ref().filter(|o| Some(o.group) == f.group);
        let pos = NumRule::default().above(0.0);
        let out = Dialog::new("Grading groups and volumes", crate::glyphs::by_name("GRADINGGROUP")).width(820.0).resizable(true).max_body_height(640.0).help("A grading group builds one surface. Volumes are measured between the base surface and the group surface (cut where the base is higher). Cut and fill factors are yours; blank is 1. Raise and lower moves every footprint of the group. Auto-balance finds the shift that gives the target net volume, between the limits you give, and shows every evaluation.").show_body(
            ctx,
            |ui| {
                ui_kit::form(ui, "gg-head", |fm| {
                    fm.row("Group", |ui| {
                        d::pick_from(ui, "gg-group", &groups, &mut f.group);
                    });
                    fm.row_note("Base surface", "Blank: the group's own target surface", |ui| {
                        d::pick_from(ui, "gg-base", &surfaces, &mut f.base);
                    });
                });
                let Some(dt) = &data else { return };
                ui.label(RichText::new(format!("{} grading(s), {} infill(s), shift {} m, site {}", dt.group.gradings.len(), dt.group.infills.len(), fmt(dt.group.shift, 3), dt.site)).small().weak());
                ui_kit::section(ui, "Volumes", |ui| {
                    ui_kit::form(ui, "gg-vol", |fm| {
                        fm.row_note("Cut factor", "Blank for none (1)", |ui| {
                            d::num_opt_rule(ui, &mut f.cut_factor, "", &pos).ok();
                        });
                        fm.row_note("Fill factor", "Blank for none (1)", |ui| {
                            d::num_opt_rule(ui, &mut f.fill_factor, "", &pos).ok();
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!busy, egui::Button::new("Calculate volumes")).clicked() {
                            act = 1;
                        }
                        if ui.add_enabled(out_ref.and_then(|o| o.volume.as_ref()).is_some_and(|v| v.is_ok()), egui::Button::new("Send to the bill")).clicked() {
                            act = 5;
                        }
                    });
                    if let Some(v) = out_ref.and_then(|o| o.volume.as_ref()) {
                        match v {
                            Ok((raw, adj)) => ui_kit::results_table(ui, "gg-vres", &["", "Cut m3", "Fill m3", "Net m3 (cut less fill)"], &[Col::Text, Col::Num, Col::Num, Col::Num], &[vec!["Measured".into(), fmt(raw.cut, 1), fmt(raw.fill, 1), fmt(raw.net, 1)], vec!["With the factors".into(), fmt(adj.cut, 1), fmt(adj.fill, 1), fmt(adj.net, 1)]]),
                            Err(e) => {
                                ui_kit::callout(ui, Level::Warning, e.clone());
                            }
                        }
                    }
                });
                ui_kit::section(ui, "Raise and lower", |ui| {
                    ui.horizontal(|ui| {
                        d::num_rule(ui, &mut f.step, "m", NumRule::default().above(0.0));
                        if ui.button("Raise").clicked() {
                            act = 2;
                        }
                        if ui.button("Lower").clicked() {
                            act = 3;
                        }
                    });
                });
                ui_kit::section(ui, "Auto-balance", |ui| {
                    ui_kit::form(ui, "gg-bal", |fm| {
                        fm.row_note("Target net volume", "Cut less fill after the factors; blank for 0", |ui| {
                            d::num_opt(ui, &mut f.target, "m3").ok();
                        });
                        fm.row("Smallest shift", |ui| {
                            d::num(ui, &mut f.min_shift, "m");
                        });
                        fm.row("Largest shift", |ui| {
                            d::num(ui, &mut f.max_shift, "m");
                        });
                        fm.row("Tolerance", |ui| {
                            d::num_rule(ui, &mut f.tolerance, "m3", NumRule::required().min(0.0));
                        });
                        fm.row("Greatest number of iterations", |ui| {
                            d::num_rule(ui, &mut f.max_iter, "", NumRule::required().above(0.0));
                        });
                    });
                    if ui.add_enabled(!busy, egui::Button::new("Find the balance")).clicked() {
                        act = 4;
                    }
                    if busy {
                        ui.label(RichText::new("Working (see the progress window)...").weak());
                    }
                    match out_ref.and_then(|o| o.balance.as_ref()) {
                        Some(Ok(b)) => {
                            ui.label(format!("{} after {} evaluation(s): shift {} m, net {} m3.", if b.converged { "Converged" } else { "Did not converge" }, b.iterations, fmt(b.shift, 4), fmt(b.net, 1)));
                            let rows: Vec<Vec<String>> = b.history.iter().enumerate().map(|(i, h)| vec![(i + 1).to_string(), fmt(h.shift, 4), fmt(h.cut, 1), fmt(h.fill, 1), fmt(h.net, 1)]).collect();
                            egui::ScrollArea::vertical().id_salt("gg-hist").max_height(140.0).show(ui, |ui| {
                                ui_kit::results_table(ui, "gg-hist", &["Iteration", "Shift m", "Cut m3", "Fill m3", "Net m3"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
                            });
                        }
                        Some(Err(e)) => {
                            ui_kit::callout(ui, Level::Warning, e.clone());
                        }
                        None => {}
                    }
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
        );
        if act > 0 {
            f.message = match self.grade_group_act(&f, act) {
                Ok(m) => m,
                Err(e) => format!("Refused: {e}"),
            };
        }
        if out.closed {
            return;
        }
        self.civil_ui.grade.group = Some(f);
    }

    fn grade_group_act(&mut self, f: &GroupForm, act: u8) -> Result<String, String> {
        let gid = f.group.ok_or("Choose a group.")?;
        let mut data = civil::get::<GradingData>(&self.doc, gid, GRADING).ok_or("The group cannot be read.")?;
        let name = self.doc.find(gid).map(|e| e.name.clone()).unwrap_or_default();
        let factor = |t: &str| -> Result<f64, String> { Ok(opt("A factor", t)?.unwrap_or(1.0)) };
        match act {
            2 | 3 => {
                let step = num("The step", &f.step)?;
                data.group.raise_lower(if act == 2 { step } else { -step }).map_err(|e| e.to_string())?;
                self.store_grading(gid, &data);
                self.schedule_commit_now();
                Ok(format!("The group is now shifted by {} m. The surface rebuilds.", fmt(data.group.shift, 3)))
            }
            5 => {
                let o = self.civil_ui.grade.group_out.as_ref().filter(|o| o.group == gid).ok_or("Calculate the volumes first.")?;
                let Some(Ok((raw, adj))) = &o.volume else { return Err("Calculate the volumes first.".into()) };
                let lines: Vec<(String, String, f64, String)> = vec![("Grading".into(), format!("Cut, {name}"), raw.cut, "m3".into()), ("Grading".into(), format!("Fill, {name}"), raw.fill, "m3".into()), ("Grading".into(), format!("Net after factors, {name}"), adj.net, "m3".into())];
                if let Some(e) = self.doc.find_mut(gid) {
                    e.properties.insert(crate::civil_quantities::PROP_BILL.into(), serde_json::to_string(&lines).unwrap_or_default());
                }
                self.schedule_commit_now();
                Ok("Sent the volumes to the bill (category Grading).".into())
            }
            _ => {
                let fs = ShrinkSwell { cut_factor: factor(&f.cut_factor)?, fill_factor: factor(&f.fill_factor)? };
                let ctx = self.civil_ctx();
                let base_id = f.base.map(|b| b.0).or(data.base).or(data.ground).ok_or("The group has no target surface to measure against: choose a base surface.")?;
                let base = ctx.tin(base_id).ok_or("The base surface has not been built yet.")?;
                let fp = crate::background::fingerprint(&self.doc);
                if act == 1 {
                    let d2 = data.clone();
                    self.bg.grade = Some(Job::spawn("Grading volumes", fp, move || {
                        let r = (|| {
                            let b = crate::civil_grading::run_group(&d2, &ctx)?;
                            let v = b.volume_against(&base, None).map_err(|e| e.to_string())?;
                            let adj = fs.apply(&v).map_err(|e| e.to_string())?;
                            Ok((v, adj))
                        })();
                        GradeJob::Volume(GroupOut { group: gid, name, volume: Some(r), balance: None })
                    }));
                    Ok("Calculating the volumes in the background.".into())
                } else {
                    let opts = BalanceOptions { target_net: opt("The target net volume", &f.target)?.unwrap_or(0.0), factors: fs, min_shift: num("The smallest shift", &f.min_shift)?, max_shift: num("The largest shift", &f.max_shift)?, tolerance: num("The tolerance", &f.tolerance)?, max_iterations: num("The iterations", &f.max_iter)? as usize };
                    let mut d2 = data.clone();
                    self.bg.grade = Some(Job::spawn("Auto-balancing a grading group", fp, move || {
                        let r = (|| {
                            let site = cg::site_from(&ctx.elements, &d2.site);
                            let ground = d2.ground.and_then(|g| ctx.tin(g));
                            d2.group.auto_balance(&site, ground.as_deref(), &base, None, &opts).map_err(|e| e.to_string())
                        })();
                        let shift = d2.group.shift;
                        GradeJob::Balance(GroupOut { group: gid, name, volume: None, balance: Some(r) }, shift)
                    }));
                    Ok("Balancing in the background.".into())
                }
            }
        }
    }

    pub(crate) fn grade_tick(&mut self) {
        if let Some(polled) = crate::background::take(&mut self.bg.grade, &self.doc) {
            match polled {
                crate::background::Poll::Done(GradeJob::Volume(o)) => {
                    self.status = format!("Volumes of {} calculated.", o.name);
                    self.civil_ui.grade.group_out = Some(o);
                }
                crate::background::Poll::Done(GradeJob::Balance(o, shift)) => {
                    if let Some(Ok(b)) = &o.balance {
                        if let Some(mut data) = civil::get::<GradingData>(&self.doc, o.group, GRADING) {
                            data.group.shift = shift;
                            self.store_grading(o.group, &data);
                            self.schedule_commit_now();
                        }
                        self.status = format!("Balanced {}: shift {:.4} m after {} evaluation(s).", o.name, b.shift, b.iterations);
                    } else {
                        self.status = format!("Balance of {} did not finish: {}", o.name, o.balance.as_ref().and_then(|b| b.as_ref().err()).cloned().unwrap_or_default());
                    }
                    self.civil_ui.grade.group_out = Some(o);
                }
                other => self.status = crate::background::discard_message("Grading calculation", &other),
            }
        }
    }
}

// ─── HELPERS: PAD, POND, DITCH OR BERM ──────────────────────────────────────

pub struct HelpForm {
    tab: usize,
    name: String,
    rows: Vec<(String, String)>,
    plane: bool,
    level: String,
    z0: String,
    gx: String,
    gy: String,
    cut: SlopeIn,
    fill: SlopeIn,
    max_distance: String,
    ray_spacing: String,
    facet_deg: String,
    ground: Option<ElementId>,
    rim: Option<ElementId>,
    floor: String,
    inside: SlopeIn,
    levels: String,
    pond_group: Option<ElementId>,
    line: Option<ElementId>,
    side: usize,
    steps: Vec<(String, String)>,
    message: String,
    stage: Option<Result<Vec<afrobim_terrain::StagePoint>, String>>,
}

impl HelpForm {
    fn new(example: bool, ground: Option<ElementId>) -> HelpForm {
        let ex = |s: &str| if example { s.to_string() } else { String::new() };
        HelpForm { tab: 0, name: String::new(), rows: vec![Default::default(); 4], plane: false, level: String::new(), z0: String::new(), gx: String::new(), gy: String::new(), cut: SlopeIn { percent: false, v: ex("2") }, fill: SlopeIn { percent: false, v: ex("3") }, max_distance: ex("60"), ray_spacing: ex("2"), facet_deg: ex("10"), ground, rim: None, floor: String::new(), inside: SlopeIn { percent: false, v: ex("3") }, levels: String::new(), pond_group: None, line: None, side: 0, steps: vec![Default::default(); 3], message: String::new(), stage: None }
    }
}

impl AfroBimApp {
    fn helpers_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.grade.helpers.take() else { return };
        for (e, n) in std::mem::take(&mut self.civil_ui.queue) {
            match f.rows.iter().position(|r| r.0.trim().is_empty() && r.1.trim().is_empty()) {
                Some(i) => f.rows[i] = (format!("{e:.3}"), format!("{n:.3}")),
                None => f.rows.push((format!("{e:.3}"), format!("{n:.3}"))),
            }
        }
        let surfaces = civil::surface_choices(&self.doc);
        let groups = civil::of_category(&self.doc, GRADING);
        let mut act = 0u8;
        let mut pick = false;
        let demo = self.options.civil_demo;
        let out = Dialog::new("Grading helpers", crate::glyphs::by_name("GRADEHELPERS")).width(720.0).resizable(true).max_body_height(620.0).help("A building pad is graded from its outline to the ground; a pond is graded down from a closed rim line to a floor; a ditch or berm is a stepped template swept along a line. Every level, slope and distance is an input.").show_body(
            ctx,
            |ui| {
                if demo {
                    ui_kit::callout(ui, Level::Info, "Some fields hold example values for the demonstration: they are not from any standard or design.");
                }
                ui_kit::tabs(ui, &mut f.tab, &["Building pad", "Pond", "Ditch or berm"]);
                ui_kit::form(ui, "hp-head", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Pad A"));
                    if f.tab != 2 {
                        fm.row_note("Ground surface", "The slopes daylight to it", |ui| {
                            d::pick_from(ui, "hp-ground", &surfaces, &mut f.ground);
                        });
                    }
                });
                match f.tab {
                    0 => {
                        ui_kit::section(ui, "Outline", |ui| {
                            egui::Grid::new("hp-rows").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
                                for h in ["#", "Easting (m)", "Northing (m)", ""] {
                                    ui.label(RichText::new(h).small().strong());
                                }
                                ui.end_row();
                                let mut remove = None;
                                for (i, r) in f.rows.iter_mut().enumerate() {
                                    ui.label((i + 1).to_string());
                                    ui.add(egui::TextEdit::singleline(&mut r.0).desired_width(100.0));
                                    ui.add(egui::TextEdit::singleline(&mut r.1).desired_width(100.0));
                                    if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this corner").clicked() {
                                        remove = Some(i);
                                    }
                                    ui.end_row();
                                }
                                if let Some(i) = remove {
                                    f.rows.remove(i);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Add a corner").clicked() {
                                    f.rows.push(Default::default());
                                }
                                if ui.button("Pick in the model").clicked() {
                                    pick = true;
                                }
                            });
                        });
                        ui_kit::form(ui, "hp-pad", |fm| {
                            fm.row("Platform", |ui| {
                                ui_kit::segmented(ui, &mut (f.plane as usize), &["Level", "Plane with crossfall"]);
                            });
                            if !f.plane {
                                fm.row("Platform level", |ui| {
                                    d::num(ui, &mut f.level, "m");
                                });
                            } else {
                                fm.row("Level at the first corner", |ui| {
                                    d::num(ui, &mut f.z0, "m");
                                });
                                fm.row("Crossfall in easting", |ui| {
                                    d::num(ui, &mut f.gx, "%");
                                });
                                fm.row("Crossfall in northing", |ui| {
                                    d::num(ui, &mut f.gy, "%");
                                });
                            }
                            fm.row("Cut slope", |ui| f.cut.ui(ui));
                            fm.row("Fill slope", |ui| f.fill.ui(ui));
                            fm.row("Search distance", |ui| {
                                d::num_rule(ui, &mut f.max_distance, "m", NumRule::required().above(0.0));
                            });
                        });
                    }
                    1 => {
                        ui_kit::form(ui, "hp-pond", |fm| {
                            fm.row_note("Rim", "A closed feature line", |ui| lines_combo(ui, "hp-rim", &self.doc, &mut f.rim));
                            fm.row("Floor level", |ui| {
                                d::num(ui, &mut f.floor, "m");
                            });
                            fm.row("Inside slope", |ui| f.inside.ui(ui));
                        });
                    }
                    _ => {
                        ui_kit::form(ui, "hp-ditch", |fm| {
                            fm.row("Along", |ui| lines_combo(ui, "hp-line", &self.doc, &mut f.line));
                            fm.row("Side", |ui| {
                                ui_kit::segmented(ui, &mut f.side, &["Left", "Right"]);
                            });
                        });
                        ui_kit::section(ui, "Template steps, outwards", |ui| {
                            egui::Grid::new("hp-steps").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
                                for h in ["#", "Offset (m)", "Level change (m)"] {
                                    ui.label(RichText::new(h).small().strong());
                                }
                                ui.end_row();
                                for (i, r) in f.steps.iter_mut().enumerate() {
                                    ui.label((i + 1).to_string());
                                    ui.add(egui::TextEdit::singleline(&mut r.0).desired_width(90.0));
                                    ui.add(egui::TextEdit::singleline(&mut r.1).desired_width(90.0));
                                    ui.end_row();
                                }
                            });
                            if ui.button("Add a step").clicked() {
                                f.steps.push(Default::default());
                            }
                            ui_kit::note(ui, "Each step is an offset from the one before and a change of level from it: down then up is a ditch, up then down a berm.");
                        });
                    }
                }
                if f.tab != 2 {
                    ui_kit::form(ui, "hp-proj", |fm| {
                        fm.row("Ray spacing", |ui| {
                            d::num_rule(ui, &mut f.ray_spacing, "m", NumRule::required().above(0.0));
                        });
                        fm.row("Facet angle", |ui| {
                            d::num_rule(ui, &mut f.facet_deg, "deg", NumRule::required().above(0.0));
                        });
                    });
                } else {
                    ui_kit::form(ui, "hp-proj", |fm| {
                        fm.row("Ray spacing", |ui| {
                            d::num_rule(ui, &mut f.ray_spacing, "m", NumRule::required().above(0.0));
                        });
                        fm.row("Facet angle", |ui| {
                            d::num_rule(ui, &mut f.facet_deg, "deg", NumRule::required().above(0.0));
                        });
                    });
                }
                if ui.button(["Create the pad", "Create the pond", "Create the ditch or berm"][f.tab.min(2)]).clicked() {
                    act = 1;
                }
                if f.tab == 1 {
                    ui_kit::section(ui, "Stage-storage", |ui| {
                        ui_kit::form(ui, "hp-stage", |fm| {
                            fm.row_note("Pond group", "The group whose surface is the basin", |ui| {
                                d::pick_from(ui, "hp-pg", &groups, &mut f.pond_group);
                            });
                            fm.row_note("Water levels", "Increasing, below the spill level, for example 1.0, 1.5, 2.0", |ui| d::text(ui, &mut f.levels, "levels"));
                        });
                        if ui.button("Calculate the stage-storage table").clicked() {
                            act = 2;
                        }
                        match &f.stage {
                            Some(Ok(rows)) => {
                                let rr: Vec<Vec<String>> = rows.iter().map(|r| vec![fmt(r.elevation, 3), fmt(r.volume, 2), fmt(r.water_area, 2), fmt(r.incremental_volume, 2)]).collect();
                                ui_kit::results_table(ui, "hp-stage-t", &["Level m", "Volume m3", "Water area m2", "Added m3"], &[Col::Num, Col::Num, Col::Num, Col::Num], &rr);
                                if ui.button("Export CSV...").clicked() {
                                    act = 3;
                                }
                            }
                            Some(Err(e)) => {
                                ui_kit::callout(ui, Level::Warning, e.clone());
                            }
                            None => {}
                        }
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::Queue);
            self.civil_ui.pick_repeat = true;
        }
        if act > 0 {
            f.message = match self.helpers_act(&mut f, act) {
                Ok(m) => m,
                Err(e) => format!("Refused: {e}"),
            };
        }
        if out.closed {
            self.civil_ui.pick = None;
            return;
        }
        self.civil_ui.grade.helpers = Some(f);
    }

    pub(crate) fn helpers_act(&mut self, f: &mut HelpForm, act: u8) -> Result<String, String> {
        let params = || -> Result<ProjectionParams, String> {
            let p = ProjectionParams { ray_spacing: num("The ray spacing", &f.ray_spacing)?, facet_angle: num("The facet angle", &f.facet_deg)?.to_radians() };
            p.validate().map_err(|e| e.to_string())?;
            Ok(p)
        };
        let name = if f.name.trim().is_empty() { ["Pad", "Pond", "Ditch"][f.tab.min(2)].to_string() } else { f.name.trim().to_string() };
        match act {
            1 => match f.tab {
                0 => {
                    let outline: Vec<[f64; 2]> = f.rows.iter().filter(|r| !r.0.trim().is_empty() || !r.1.trim().is_empty()).enumerate().map(|(i, r)| Ok([num(&format!("Easting of corner {}", i + 1), &r.0)?, num(&format!("Northing of corner {}", i + 1), &r.1)?])).collect::<Result<_, String>>()?;
                    let platform = if f.plane { PlatformSurface::Plane { x0: outline.first().map(|p| p[0]).unwrap_or(0.0), y0: outline.first().map(|p| p[1]).unwrap_or(0.0), z0: num("The level at the first corner", &f.z0)?, grade_x_percent: num("The crossfall in easting", &f.gx)?, grade_y_percent: num("The crossfall in northing", &f.gy)? } } else { PlatformSurface::Level { level: num("The platform level", &f.level)? } };
                    let pkg = building_pad(&name, &outline, &platform, f.cut.get("The cut slope")?, f.fill.get("The fill slope")?, num("The search distance", &f.max_distance)?, params()?).map_err(|e| e.to_string())?;
                    self.create_package(&pkg, f.ground, "Building pad helper");
                    Ok(format!("Created the pad '{name}': a footprint feature line and a grading group."))
                }
                1 => {
                    let rim = f.rim.and_then(|id| civil::get::<FeatureLineData>(&self.doc, id, FEATURELINE)).ok_or("Choose the rim feature line.")?;
                    let pkg = pond(&name, &rim.line, num("The floor level", &f.floor)?, f.inside.get("The inside slope")?, params()?).map_err(|e| e.to_string())?;
                    self.create_package(&pkg, f.ground, "Pond helper");
                    Ok(format!("Created the pond '{name}'. Choose its group below for the stage-storage table."))
                }
                _ => {
                    let src = f.line.and_then(|id| civil::get::<FeatureLineData>(&self.doc, id, FEATURELINE)).ok_or("Choose the line to sweep along.")?;
                    let steps: Vec<TemplateStep> = f.steps.iter().filter(|r| !r.0.trim().is_empty() || !r.1.trim().is_empty()).enumerate().map(|(i, r)| Ok(TemplateStep { offset: num(&format!("Offset of step {}", i + 1), &r.0)?, dz: num(&format!("Level change of step {}", i + 1), &r.1)? })).collect::<Result<_, String>>()?;
                    let sweep = sweep_template(&src.line, None, if f.side == 0 { Side::Left } else { Side::Right }, &steps, &params()?).map_err(|e| e.to_string())?;
                    let site = src.site.clone();
                    let mut made = 0;
                    for (k, pts) in sweep.lines.iter().enumerate().skip(1) {
                        let nm = format!("{name} line {k}");
                        let r = if sweep.ring { FeatureLine::closed_from_points(&nm, &pts[..pts.len().saturating_sub(1).max(3)]) } else { FeatureLine::from_points(&nm, pts) };
                        if let Ok(mut line) = r {
                            line.last_edited = cg::next_edit_counter(&self.doc, &site);
                            cg::add_line(&mut self.doc, &nm, line, &site, "Ditch or berm helper", false);
                            made += 1;
                        }
                    }
                    // The triangles between the lines are kept as a surface of their own.
                    let kept = tin_of_triangles(&sweep.triangles).and_then(|tin| crate::alignment_glue::add_surface_at_source(&mut self.doc, &format!("{name} surface"), &tin, Some("Ditch or berm helper")).ok());
                    self.schedule_commit_now();
                    Ok(format!("Swept the template along the line: {made} feature line(s) made, one per template point, and {} surface triangle(s) between them kept as {}.", sweep.triangles.len(), if kept.is_some() { format!("the surface '{name} surface'") } else { "nothing (the triangles could not form a surface)".to_string() }))
                }
            },
            2 => {
                let gid = f.pond_group.ok_or("Choose the pond group.")?;
                let data = civil::get::<GradingData>(&self.doc, gid, GRADING).ok_or("The group cannot be read.")?;
                let ctx = self.civil_ctx();
                let build = crate::civil_grading::run_group(&data, &ctx)?;
                let levels: Vec<f64> = f.levels.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).map(|s| num("A level", s)).collect::<Result<_, _>>()?;
                // The basin is bounded by the first grading's footprint (the rim), as the engine's helper does.
                let first = data.group.gradings.first().ok_or("The group has no grading.")?;
                let site = cg::site_from(&ctx.elements, &data.site);
                let rim = site.get(&first.footprint).ok_or("The rim line is missing.")?;
                let ring = rim.plan_ring(data.group.params.facet_angle).map_err(|e| e.to_string())?;
                f.stage = Some(build.stage_storage(Some(&ring), &levels).map_err(|e| e.to_string()));
                Ok("Stage-storage calculated.".into())
            }
            _ => {
                if let Some(Ok(rows)) = &f.stage {
                    if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("stage-storage.csv").save_file() {
                        return Ok(match std::fs::write(&p, stage_storage_csv(rows)) {
                            Ok(()) => format!("Wrote {}.", p.display()),
                            Err(e) => format!("The file could not be written: {e}"),
                        });
                    }
                }
                Ok(String::new())
            }
        }
    }

    /// The feature lines of a design package as elements and its group as a grading group.
    pub(crate) fn create_package(&mut self, pkg: &DesignPackage, ground: Option<ElementId>, source: &str) -> ElementId {
        let id = cg::add_package(&mut self.doc, pkg, ground, source, false);
        self.doc.select(&[id], SelectMode::Replace);
        self.schedule_commit_now();
        id
    }

    pub(crate) fn grading_windows(&mut self, ctx: &egui::Context) {
        self.grade_tick();
        self.fl_new_window(ctx);
        self.fl_edit_window(ctx);
        self.site_window(ctx);
        self.grade_new_window(ctx);
        self.grade_group_window(ctx);
        self.helpers_window(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a1_tests::{ctx_with_tins, demo_doc};
    use afrobim_kernel::Document;

    fn app_with(doc: Document) -> AfroBimApp {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = AfroBimApp::new(&cc, crate::Options { screenshot: Some(std::env::temp_dir().join("afrobim-a1-test.png")), ..crate::Options::default() });
        app.doc = doc;
        app
    }

    fn two_lines() -> (Document, ElementId, ElementId) {
        // Two lines crossing at (450010, 9960010): one at level 100 along easting, one at level 104 along northing.
        let mut doc = Document::default();
        let a = FeatureLine::from_points("A", &[[450_000.0, 9_960_010.0, 100.0], [450_020.0, 9_960_010.0, 100.0]]).unwrap();
        let b = FeatureLine::from_points("B", &[[450_010.0, 9_960_000.0, 104.0], [450_010.0, 9_960_020.0, 104.0]]).unwrap();
        let ia = cg::add_line(&mut doc, "A", a, "Site 1", "test", false);
        let ib = cg::add_line(&mut doc, "B", b, "Site 1", "test", false);
        (doc, ia, ib)
    }

    #[test]
    fn feature_lines_and_groups_survive_save_and_load_and_sit_at_source_coordinates() {
        let doc = demo_doc();
        let back = Document::from_json(&doc.to_json()).unwrap();
        for cat in [FEATURELINE, GRADING] {
            let a: Vec<_> = doc.elements().iter().filter(|e| e.category == cat).collect();
            let b: Vec<_> = back.elements().iter().filter(|e| e.category == cat).collect();
            assert!(!a.is_empty());
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                assert_eq!((&x.data, x.origin), (&y.data, y.origin), "{cat}");
            }
        }
        // A feature line's origin is the least easting and northing of its vertices times 1000.
        let fl = doc.elements().iter().find(|e| e.category == FEATURELINE).unwrap();
        let d: FeatureLineData = serde_json::from_value(fl.data.clone()).unwrap();
        let b = cg::line_bounds(&d.line).unwrap();
        assert!((fl.origin.x - b.min_e * 1000.0).abs() < 1e-3 && (fl.origin.y - b.min_n * 1000.0).abs() < 1e-3);
        // The group builds from its footprints and ground, with every ray finding daylight, and its mesh is at source coordinates.
        let ctx = ctx_with_tins(&doc);
        let g = doc.elements().iter().find(|e| e.category == GRADING).unwrap();
        let built = crate::civil_build::build(g, &ctx);
        assert!(built.scene.notes.is_empty(), "{:?}", built.scene.notes);
        assert!(built.scene.derived.iter().any(|d| d.0 == "Rays" && d.1.contains("(0 without")), "{:?}", built.scene.derived);
        let tin = built.tin.as_ref().expect("the group has a surface");
        let tb = tin.bounds();
        assert!(tb.min_x < b.min_e && tb.max_x > b.max_e && tb.min_y < b.min_n && tb.max_y > b.max_n, "the batter surrounds the footprint");
        let mesh = &built.scene.meshes[0];
        assert!(mesh.vertices.iter().all(|v| { let (x, y) = ((g.origin.x + v[0]) / 1000.0, (g.origin.y + v[1]) / 1000.0); x >= tb.min_x - 1e-6 && x <= tb.max_x + 1e-6 && y >= tb.min_y - 1e-6 && y <= tb.max_y + 1e-6 }), "the mesh is in element-local millimetres from the origin");
        // The fill under a pad above the ground is positive and the cut zero (the pad is 0.4 m above the ground at its centre).
        let fill = built.scene.derived.iter().find(|d| d.0.starts_with("Fill")).map(|d| d.1.parse::<f64>().unwrap()).unwrap();
        assert!(fill > 100.0, "{fill}");
    }

    #[test]
    fn a_grading_group_rebuilds_when_its_footprint_or_target_changes() {
        let mut doc = demo_doc();
        let ctx = ctx_with_tins(&doc);
        let g = doc.elements().iter().find(|e| e.category == GRADING).unwrap().clone();
        let before = crate::civil_build::build(&g, &ctx).tin.unwrap();
        // Raise the footprint by half a metre: the surface changes, and the cache key changes with the dependency hash.
        let (id, _, mut dt) = cg::lines_of(&doc, &civil::get::<GradingData>(&doc, g.id, GRADING).unwrap().site).remove(0);
        let dep0 = crate::civil_cache::dependency_hash(&doc);
        dt.line.raise_lower(0.5).unwrap();
        cg::store_line(&mut doc, id, &dt);
        assert_ne!(dep0, crate::civil_cache::dependency_hash(&doc), "an edited footprint changes what the group reads");
        let ctx2 = ctx_with_tins(&doc);
        let after = crate::civil_build::build(doc.find(g.id).unwrap(), &ctx2).tin.unwrap();
        let (za, zb) = (before.elevation_at(g.origin.x / 1000.0 + 20.0, g.origin.y / 1000.0 + 15.0), after.elevation_at(g.origin.x / 1000.0 + 20.0, g.origin.y / 1000.0 + 15.0));
        assert!(za.is_some() && zb.is_some() && (zb.unwrap() - za.unwrap() - 0.5).abs() < 1e-6, "{za:?} {zb:?}");
        // The group's own stale test sees the same.
        let d: GradingData = civil::get(&doc, g.id, GRADING).unwrap();
        let site = cg::site_from(doc.elements(), &d.site);
        let ground = d.ground.and_then(|x| ctx2.tin(x));
        let old_site = cg::site_from(ctx.elements.as_slice(), &d.site);
        let data = d.group.rebuild_data(&old_site, ground.as_deref()).unwrap();
        assert!(d.group.stale_reasons(&data, &site, ground.as_deref()).iter().any(|r| matches!(r, StaleReason::FootprintChanged(_))));
    }

    #[test]
    fn crossings_of_a_site_are_resolved_reported_and_written_back() {
        let (doc, ia, ib) = two_lines();
        let mut app = app_with(doc);
        let mut f = SiteForm { site: "Site 1".into(), facet_deg: "5".into(), priorities: vec![], loaded: String::new(), report: vec![], message: String::new() };
        // Equal priority and equal edit counters: the line made first (A) wins, and the report says it was a tie.
        let m = app.site_resolve(&mut f).unwrap();
        assert!(m.contains("1 crossing"), "{m}");
        assert_eq!(f.report.len(), 1);
        let c = &f.report[0];
        assert!((c.z_used - 100.0).abs() < 1e-9 && c.changed && c.rule == PriorityRule::Tie && c.winner == "A", "{c:?}");
        let b: FeatureLineData = civil::get(&app.doc, ib, FEATURELINE).unwrap();
        assert!((b.line.z_at(10.0).unwrap() - 100.0).abs() < 1e-9, "B now passes through A's level");
        // With a higher style priority on B the other way round: B wins.
        let (doc2, _, ib2) = two_lines();
        let mut app2 = app_with(doc2);
        let mut bd: FeatureLineData = civil::get(&app2.doc, ib2, FEATURELINE).unwrap();
        bd.line.style_priority = 5;
        cg::store_line(&mut app2.doc, ib2, &bd);
        app2.site_resolve(&mut f).unwrap();
        assert!((f.report[0].z_used - 104.0).abs() < 1e-9 && f.report[0].rule == PriorityRule::StylePriority && f.report[0].winner == "B", "{:?}", f.report[0]);
        let a2: FeatureLineData = cg::lines_of(&app2.doc, "Site 1").remove(0).2;
        assert!((a2.line.z_at(10.0).unwrap() - 104.0).abs() < 1e-9);
        let _ = ia;
        // A bad facet angle is refused.
        f.facet_deg = "x".into();
        assert!(app.site_resolve(&mut f).is_err());
    }

    #[test]
    fn feature_line_edits_apply_and_refusals_change_nothing() {
        let (doc, ia, _) = two_lines();
        let mut app = app_with(doc);
        let mut f = FlEdit::new(Some(ia));
        let data: FeatureLineData = civil::get(&app.doc, ia, FEATURELINE).unwrap();
        f.load(&data);
        // Change the level of the last vertex through the table.
        f.z[1] = "101.5".into();
        app.fl_edit_apply(&mut f, 1).unwrap();
        let d: FeatureLineData = civil::get(&app.doc, ia, FEATURELINE).unwrap();
        assert!((d.line.vertices[1].z - 101.5).abs() < 1e-9);
        // The grade to the next vertex: 20 m long, 1.5 m rise is 7.5 %.
        let rows = d.line.grade_table().unwrap();
        assert!((rows[0].grade_percent - 7.5).abs() < 1e-9);
        // Nothing changed in the table is refused.
        let mut g = FlEdit::new(Some(ia));
        g.load(&d);
        assert!(app.fl_edit_apply(&mut g, 1).is_err());
        // A fillet on a line with no interior PI is refused and the line is untouched.
        let before = app.doc.find(ia).unwrap().data.clone();
        g.pi = "0".into();
        g.radius = "5".into();
        assert!(app.fl_edit_apply(&mut g, 2).is_err());
        assert_eq!(app.doc.find(ia).unwrap().data, before);
        // Raise the whole line, then break it in two.
        g.delta = "1".into();
        app.fl_edit_apply(&mut g, 8).unwrap();
        g.station = "10".into();
        let n = app.doc.elements().iter().filter(|e| e.category == FEATURELINE).count();
        app.fl_edit_apply(&mut g, 7).unwrap();
        assert_eq!(app.doc.elements().iter().filter(|e| e.category == FEATURELINE).count(), n + 1);
    }

    #[test]
    fn auto_balance_runs_in_the_background_and_shows_every_iteration() {
        let doc = demo_doc();
        let mut app = app_with(doc);
        let gid = app.doc.elements().iter().find(|e| e.category == GRADING).unwrap().id;
        // The pad is all fill: lowering the group brings the net volume to zero somewhere below.
        let f = GroupForm { group: Some(gid), base: None, cut_factor: String::new(), fill_factor: String::new(), step: String::new(), target: "0".into(), min_shift: "-3".into(), max_shift: "3".into(), tolerance: "1".into(), max_iter: "40".into(), message: String::new() };
        app.grade_group_act(&f, 4).unwrap();
        for _ in 0..600 {
            app.grade_tick();
            if app.bg.grade.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let o = app.civil_ui.grade.group_out.as_ref().expect("the balance finished");
        let b = o.balance.as_ref().unwrap().as_ref().expect("a balance").clone();
        assert!(b.converged && b.iterations >= 2 && b.history.len() >= b.iterations, "{b:?}");
        assert!(b.net.abs() <= 1.0);
        // The shift was stored on the group.
        let d: GradingData = civil::get(&app.doc, gid, GRADING).unwrap();
        assert!((d.group.shift - b.shift).abs() < 1e-12);
        // Raise and lower move the same shift.
        let mut f2 = f;
        f2.step = "0.25".into();
        app.grade_group_act(&f2, 2).unwrap();
        let d2: GradingData = civil::get(&app.doc, gid, GRADING).unwrap();
        assert!((d2.group.shift - b.shift - 0.25).abs() < 1e-12);
        let _ = ctx_with_tins(&app.doc);
    }

    #[test]
    fn the_pad_pond_and_ditch_helpers_make_elements_and_a_stage_storage_table() {
        let doc = demo_doc();
        let mut app = app_with(doc);
        let ground = civil::surface_choices(&app.doc).first().map(|s| s.0);
        let mut f = HelpForm::new(true, ground);
        f.name = "Pad B".into();
        f.level = "1151".into();
        f.rows = vec![("450040".into(), "9960040".into()), ("450070".into(), "9960040".into()), ("450070".into(), "9960070".into()), ("450040".into(), "9960070".into())];
        let n0 = app.doc.elements().len();
        app.helpers_act(&mut f, 1).unwrap();
        assert_eq!(app.doc.elements().len(), n0 + 2, "a footprint and a group");
        // A pond needs a closed rim above its floor: a rim at level 1153 over a floor at 1150.
        let rim = FeatureLine::closed_from_points("Rim", &[[450_100.0, 9_960_100.0, 1153.0], [450_130.0, 9_960_100.0, 1153.0], [450_130.0, 9_960_130.0, 1153.0], [450_100.0, 9_960_130.0, 1153.0]]).unwrap();
        let rim_id = cg::add_line(&mut app.doc, "Rim", rim, "Pond site", "test", false);
        let mut p = HelpForm::new(true, ground);
        p.tab = 1;
        p.name = "Pond".into();
        p.rim = Some(rim_id);
        p.floor = "1150".into();
        app.helpers_act(&mut p, 1).unwrap();
        let gid = app.doc.elements().iter().find(|e| e.category == GRADING && e.name == "Pond").unwrap().id;
        p.pond_group = Some(gid);
        p.levels = "1150.5, 1151, 1152".into();
        app.civil_build_now();
        app.helpers_act(&mut p, 2).unwrap();
        let rows = p.stage.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].volume > 0.0 && rows[2].volume > rows[1].volume && rows[1].volume > rows[0].volume);
        // Refusals say why: a floor above the rim.
        p.floor = "1155".into();
        assert!(app.helpers_act(&mut p, 1).is_err());
    }

    #[test]
    fn a_ditch_from_the_helper_keeps_its_triangles_as_a_surface() {
        let doc = demo_doc();
        let mut app = app_with(doc);
        let ground = civil::surface_choices(&app.doc).first().map(|s| s.0);
        let src = FeatureLine::from_points("Ditch axis", &[[450_100.0, 9_960_100.0, 1151.0], [450_140.0, 9_960_100.0, 1151.0], [450_180.0, 9_960_110.0, 1151.5]]).unwrap();
        let id = cg::add_line(&mut app.doc, "Ditch axis", src, "Ditch site", "test", false);
        let mut f = HelpForm::new(true, ground);
        f.tab = 2;
        f.name = "Ditch".into();
        f.line = Some(id);
        f.steps = vec![("1.5".into(), "-0.6".into()), ("1.5".into(), "0.6".into())];
        let msg = app.helpers_act(&mut f, 1).unwrap();
        assert!(msg.contains("kept as the surface"), "{msg}");
        let e = app.doc.elements().iter().find(|e| e.name == "Ditch surface").expect("the ditch has a surface");
        let tin = serde_json::from_value::<afrobim_corridor::SurfaceDefinition>(e.data.clone()).unwrap().tin;
        assert!(tin.triangles().len() >= 4, "{} triangles", tin.triangles().len());
    }
}

/// A TIN from loose triangles (corners as easting, northing, level): equal corners become one point and a
/// triangle with no area in plan is left out. None when nothing is left.
fn tin_of_triangles(triangles: &[[[f64; 3]; 3]]) -> Option<afrobim_corridor::Tin> {
    let mut index: std::collections::HashMap<[u64; 3], u32> = std::collections::HashMap::new();
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for t in triangles {
        let area2 = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1]);
        if !(area2.abs() > 1e-9) {
            continue;
        }
        let ids = t.map(|p| *index.entry([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).or_insert_with(|| {
            points.push(p);
            points.len() as u32 - 1
        }));
        tris.push(ids);
    }
    afrobim_corridor::Tin::new(points, tris).ok()
}
