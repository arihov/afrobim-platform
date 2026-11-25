// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The corridor from the new model (baseline, regions with an assembly each, a frequency and a
//! ground surface), sample line groups with a per-line swath edit, and the section view array.
//! The older CORRIDOR command, with its single typical section, stays for old documents.

use std::collections::BTreeMap;

use afrobim_corridor::{Assembly, AssemblyRegion, Baseline, CorridorModel, Frequency, SampleLineGroup, SampleStations, VolumeOptions};
use afrobim_kernel::{ElementId, SelectMode};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::civil::{self, Bounds, CorridorData, SampleData, ASSEMBLY, CORRIDOR, SAMPLES};
use crate::civil_dlg::{self as d};
use crate::civil_targets::{self as tg, DocObjs, TargetRef};
use crate::ui_kit::{self, Dialog, Footer, Level, NumRule};

#[derive(Default)]
pub struct CorridorUi {
    pub cor: Option<CorForm>,
    pub samples: Option<SampForm>,
    pub views: Option<ViewsForm>,
}

pub struct RgForm {
    name: String,
    start: String,
    end: String,
    assembly: Option<ElementId>,
    /// Spacing on tangents; the spacing on curves and spirals when those are blank.
    spacing: String,
    curve: String,
    spiral: String,
    curve_none: bool,
    spiral_none: bool,
    at_geometry: bool,
    at_critical: bool,
    /// Extra stations, separated by commas.
    extra: String,
    /// The region's own copy of the assembly, whose declared targets are bound here, and the
    /// assembly of the document it was copied from.
    asm: Option<Assembly>,
    src: Option<ElementId>,
    refs: BTreeMap<String, TargetRef>,
}

impl RgForm {
    fn new(name: String, start: String, end: String, assembly: Option<ElementId>, asm: Option<Assembly>) -> RgForm {
        RgForm { name, start, end, assembly, spacing: String::new(), curve: String::new(), spiral: String::new(), curve_none: false, spiral_none: false, at_geometry: true, at_critical: true, extra: String::new(), asm, src: assembly, refs: BTreeMap::new() }
    }
}

pub struct BlForm {
    name: String,
    alignment: Option<ElementId>,
    regions: Vec<RgForm>,
}

pub struct CorForm {
    edit: Option<ElementId>,
    name: String,
    ground: Option<ElementId>,
    baselines: Vec<BlForm>,
    message: String,
    /// 0 rural, 1 urban: the area the superelevation limit of the defaults basis is read for.
    area: usize,
    emax: String,
}

impl AfroBimApp {
    fn assembly_of(&self, id: ElementId) -> Option<Assembly> {
        civil::get::<Assembly>(&self.doc, id, ASSEMBLY)
    }

    pub(crate) fn civil_corridor_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        let aligns = crate::alignment_glue::alignments(&self.doc);
        if aligns.is_empty() {
            self.status = "There is no alignment yet. Import one (LANDXML) or draw one (ALIGNDESIGN) first.".into();
            return;
        }
        let sel = self.doc.primary();
        match cv {
            CorridorModel => {
                if civil::of_category(&self.doc, ASSEMBLY).is_empty() {
                    self.status = "There is no assembly yet: build one with ASSEMBLY first.".into();
                    return;
                }
                let editing = sel.filter(|id| self.doc.find(*id).is_some_and(|e| e.category == CORRIDOR));
                let f = match editing.and_then(|id| civil::get::<CorridorData>(&self.doc, id, CORRIDOR).map(|d| (id, d))) {
                    Some((id, cd)) => CorForm {
                        edit: Some(id),
                        name: self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default(),
                        ground: Some(ElementId(cd.ground)),
                        baselines: cd
                            .model
                            .baselines
                            .iter()
                            .map(|b| BlForm {
                                name: b.name.clone(),
                                alignment: aligns.iter().find(|a| crate::alignment_glue::alignment_of(&self.doc, a.0).is_some_and(|x| x.name == b.alignment)).map(|a| a.0),
                                regions: b
                                    .regions
                                    .iter()
                                    .map(|r| {
                                        let src = civil::of_category(&self.doc, ASSEMBLY).into_iter().find(|(aid, nm)| self.assembly_of(*aid).is_some_and(|a| a == r.assembly || (a.name == r.assembly.name && *nm == r.assembly.name))).map(|x| x.0);
                                        let fq = &r.frequency;
                                        let t = fq.tangent_m;
                                        let same = |v: Option<f64>| if v == t { String::new() } else { v.map(|x| x.to_string()).unwrap_or_default() };
                                        let mut rg = RgForm::new(r.name.clone(), r.start_station_m.to_string(), r.end_station_m.to_string(), src, Some(r.assembly.clone()));
                                        rg.spacing = t.map(|v| v.to_string()).unwrap_or_default();
                                        rg.curve = same(fq.curve_m);
                                        rg.spiral = same(fq.spiral_m);
                                        rg.curve_none = fq.curve_m.is_none();
                                        rg.spiral_none = fq.spiral_m.is_none();
                                        rg.at_geometry = fq.at_geometry_points;
                                        rg.at_critical = fq.at_superelevation_critical;
                                        rg.extra = fq.extra_stations.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                                        rg
                                    })
                                    .collect(),
                            })
                            .collect(),
                        message: String::new(),
                        area: 0,
                        emax: String::new(),
                    },
                    None => {
                        let al = aligns.first().map(|a| a.0);
                        let (s0, s1) = al.and_then(|i| crate::alignment_glue::alignment_of(&self.doc, i)).map(|a| (a.start_station_m, a.end_station_m())).unwrap_or((0.0, 0.0));
                        CorForm {
                            edit: None,
                            name: String::new(),
                            ground: civil::surface_choices(&self.doc).first().map(|s| s.0),
                            baselines: vec![BlForm { name: "Baseline 1".into(), alignment: al, regions: vec![{
                                let a0 = civil::of_category(&self.doc, ASSEMBLY).first().map(|a| a.0);
                                RgForm::new("Region 1".into(), s0.to_string(), s1.to_string(), a0, a0.and_then(|i| self.assembly_of(i)))
                            }] }],
                            message: String::new(),
                            area: 0,
                            emax: String::new(),
                        }
                    }
                };
                self.civil_ui.cor.cor = Some(f);
            }
            SampleLines => {
                let editing = sel.filter(|id| self.doc.find(*id).is_some_and(|e| e.category == SAMPLES));
                let f = match editing.and_then(|id| civil::get::<SampleData>(&self.doc, id, SAMPLES).map(|d| (id, d))) {
                    Some((id, sd)) => SampForm {
                        edit: Some(id),
                        name: self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default(),
                        alignment: aligns.iter().find(|a| crate::alignment_glue::alignment_of(&self.doc, a.0).is_some_and(|x| x.name == sd.group.alignment)).map(|a| a.0),
                        group: Some(sd.group.clone()),
                        corridor: sd.corridor.map(ElementId),
                        surfaces: sd.surfaces.iter().map(|s| ElementId(*s)).collect(),
                        spec: 0,
                        interval: String::new(),
                        listed: String::new(),
                        left: String::new(),
                        right: String::new(),
                        prefix: "SL".into(),
                        columns: sd.columns.to_string(),
                        exaggeration: sd.vertical_exaggeration.to_string(),
                        e: format!("{:.3}", sd.insertion[0]),
                        n: format!("{:.3}", sd.insertion[1]),
                        message: String::new(),
                    },
                    None => SampForm { edit: None, name: String::new(), alignment: aligns.first().map(|a| a.0), group: None, corridor: None, surfaces: civil::surface_choices(&self.doc).first().map(|s| vec![s.0]).unwrap_or_default(), spec: 0, interval: String::new(), listed: String::new(), left: String::new(), right: String::new(), prefix: "SL".into(), columns: String::new(), exaggeration: String::new(), e: String::new(), n: String::new(), message: String::new() },
                };
                self.civil_ui.cor.samples = Some(f);
            }
            _ => {
                let editing = sel.filter(|id| self.doc.find(*id).is_some_and(|e| e.category == SAMPLES)).or_else(|| civil::of_category(&self.doc, SAMPLES).first().map(|s| s.0));
                match editing {
                    Some(id) => {
                        if let Some(sd) = civil::get::<SampleData>(&self.doc, id, SAMPLES) {
                            self.civil_ui.cor.views = Some(ViewsForm { id, columns: sd.columns.to_string(), exaggeration: sd.vertical_exaggeration.to_string(), e: format!("{:.3}", sd.insertion[0]), n: format!("{:.3}", sd.insertion[1]) });
                        }
                    }
                    None => self.status = "There is no sample line group yet: make one with SAMPLELINES first.".into(),
                }
            }
        }
    }

    pub(crate) fn civil_corridor_windows(&mut self, ctx: &egui::Context) {
        self.corridor_window(ctx);
        self.samples_window(ctx);
        self.views_window(ctx);
    }

    fn corridor_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cor.cor.take() else { return };
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let asms = civil::of_category(&self.doc, ASSEMBLY);
        let surfaces = civil::surface_choices(&self.doc);
        let objs = DocObjs::gather(&self.doc);
        let aligns_data: Vec<(ElementId, afrobim_alignment::Alignment)> = aligns.iter().filter_map(|(id, _)| crate::alignment_glue::alignment_of(&self.doc, *id).map(|a| (*id, a))).collect();
        let mut open_se: Option<ElementId> = None;
        let out = Dialog::new(if f.edit.is_some() { "Edit corridor" } else { "Corridor" }, crate::glyphs::by_name("CORRIDORMODEL")).width(820.0).resizable(true).max_body_height(620.0).help("A corridor has baselines (an alignment with a design profile), each with regions that follow one another along the stations. A region has an assembly and a frequency of sections. The targets the assembly declares (a width or offset target, a level target, the ground surface of a side slope) are bound per region below; one that is not yet bound blocks the corridor and says why. Lanes that follow the alignment's cross-fall take it from the superelevation stored on the alignment. Everything is an input.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    d::text(ui, &mut f.name, "e.g. Main road corridor");
                    ui.add_space(8.0);
                    ui.label("Ground surface (target)");
                    d::pick_from(ui, "cor-g", &surfaces, &mut f.ground);
                });
                ui.horizontal(|ui| {
                    ui.label("Superelevation limit").on_hover_text("Compared with the superelevation stored on each baseline's alignment");
                    ui_kit::segmented(ui, &mut f.area, &["Rural", "Urban"]);
                    let key = if f.area == 0 { "link.superelevation.max_rural" } else { "link.superelevation.max_urban" };
                    ui_kit::default_num_field(ui, "cor-emax", &mut f.emax, "%", key, &afrobim_defaults::Conditions::none(), 1.0, &NumRule::default().above(0.0));
                });
                if let Some(limit) = d::num_rule_peek(&f.emax, &NumRule::default().above(0.0)) {
                    for b in &f.baselines {
                        let Some(al) = b.alignment.and_then(|id| crate::alignment_glue::alignment_of(&self.doc, id)) else { continue };
                        let worst = al.superelevation.as_ref().map(|se| se.sections.iter().flat_map(|s| s.points.iter()).flat_map(|p| [p.left_pct, p.right_pct]).flatten().fold(0.0_f64, |m, v| m.max(v.abs())));
                        if let Some(w) = worst.filter(|w| *w > limit + 1e-9) {
                            ui_kit::callout(ui, Level::Warning, format!("Baseline {}: the alignment's stored cross-fall reaches {:.2} %, above the limit of {:.2} %.", b.name, w, limit));
                        }
                    }
                }
                let mut remove_bl = None;
                for (bi, b) in f.baselines.iter_mut().enumerate() {
                    ui_kit::section(ui, &format!("Baseline {}", bi + 1), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            ui_kit::text_field(ui, &mut b.name, "");
                            ui.label("Alignment");
                            d::pick_from(ui, &format!("bl-al{bi}"), &aligns, &mut b.alignment);
                            if ui.small_button("Remove baseline").clicked() {
                                remove_bl = Some(bi);
                            }
                        });
                        let mut remove_rg = None;
                        for (ri, r) in b.regions.iter_mut().enumerate() {
                            if r.assembly != r.src {
                                r.asm = r.assembly.and_then(|id| civil::get::<Assembly>(&self.doc, id, ASSEMBLY));
                                r.src = r.assembly;
                                r.refs.clear();
                            }
                            let al = b.alignment.and_then(|id| aligns_data.iter().find(|a| a.0 == id)).map(|a| &a.1);
                            if region_ui(ui, bi, ri, r, &asms, &surfaces, f.ground, &objs, al, &mut open_se, b.alignment) {
                                remove_rg = Some(ri);
                            }
                        }
                        if let Some(i) = remove_rg {
                            b.regions.remove(i);
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Add region").clicked() {
                                let start = b.regions.last().map(|r| r.end.clone()).unwrap_or_default();
                                let a0 = asms.first().map(|a| a.0);
                                b.regions.push(RgForm::new(format!("Region {}", b.regions.len() + 1), start, String::new(), a0, a0.and_then(|i| civil::get::<Assembly>(&self.doc, i, ASSEMBLY))));
                            }
                            if ui.button("Example spacing").on_hover_text("20 m: a demonstration number, not from any standard").clicked() {
                                for r in &mut b.regions {
                                    r.spacing = "20".into();
                                }
                            }
                        });
                    });
                }
                if let Some(i) = remove_bl {
                    f.baselines.remove(i);
                }
                if ui.button("Add baseline").clicked() {
                    f.baselines.push(BlForm { name: format!("Baseline {}", f.baselines.len() + 1), alignment: aligns.first().map(|a| a.0), regions: vec![] });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| Footer::new(if f.edit.is_some() { "Apply" } else { "Create corridor" }).icon(crate::glyphs::by_name("CORRIDORMODEL")).show(ui),
        );
        if let Some(id) = open_se {
            // The Superelevation tab of the alignment's Definition area in the Properties panel.
            self.doc.select(&[id], SelectMode::Replace);
            self.align_tab = 1;
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            match self.corridor_from_form(&f) {
                Ok(()) => return,
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.cor.cor = Some(f);
    }

    fn corridor_from_form(&mut self, f: &CorForm) -> Result<(), String> {
        let ground = f.ground.ok_or("Choose the ground surface.")?;
        let objs = DocObjs::gather(&self.doc);
        let mut baselines = Vec::new();
        let mut bounds: Option<Bounds> = None;
        for b in &f.baselines {
            let al = b.alignment.and_then(|id| crate::alignment_glue::alignment_of(&self.doc, id)).ok_or_else(|| format!("{}: choose an alignment.", b.name))?;
            if al.profile.is_none() {
                return Err(format!("{}: the alignment '{}' has no design profile. Design one with PROFILEDESIGN or import one.", b.name, al.name));
            }
            let mut regions = Vec::new();
            for r in &b.regions {
                let num = |t: &str, what: &str| t.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{} / {}: enter the {what}.", b.name, r.name));
                let spacing = num(&r.spacing, "spacing on tangents")?;
                if spacing <= 0.0 {
                    return Err(format!("{} / {}: the spacing must be greater than 0.", b.name, r.name));
                }
                let mut assembly = r.asm.clone().or_else(|| r.assembly.and_then(|id| self.assembly_of(id))).ok_or_else(|| format!("{} / {}: choose an assembly.", b.name, r.name))?;
                for note in tg::rebind_all(&mut assembly, &r.refs, &objs) {
                    return Err(format!("{} / {}: {note}", b.name, r.name));
                }
                for d in tg::declared(&assembly) {
                    if let Some(why) = tg::declared_status(&mut assembly, &d) {
                        return Err(format!("{} / {}: the target of '{}' ({}) is not yet bound: {why}.", b.name, r.name, d.part, d.label));
                    }
                }
                let opt = |t: &str, none: bool, what: &str| -> Result<Option<f64>, String> {
                    if none {
                        Ok(None)
                    } else if t.trim().is_empty() {
                        Ok(Some(spacing))
                    } else {
                        let v = num(t, what)?;
                        if v <= 0.0 {
                            return Err(format!("{} / {}: the {what} must be greater than 0.", b.name, r.name));
                        }
                        Ok(Some(v))
                    }
                };
                let mut extra = Vec::new();
                for t in r.extra.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()) {
                    extra.push(num(t, "extra station")?);
                }
                let freq = Frequency { tangent_m: Some(spacing), curve_m: opt(&r.curve, r.curve_none, "spacing on curves")?, spiral_m: opt(&r.spiral, r.spiral_none, "spacing on spirals")?, at_geometry_points: r.at_geometry, at_superelevation_critical: r.at_critical, extra_stations: extra };
                regions.push(AssemblyRegion { name: r.name.clone(), start_station_m: num(&r.start, "start station")?, end_station_m: num(&r.end, "end station")?, assembly, frequency: freq });
            }
            let baseline = Baseline { name: b.name.clone(), alignment: al.name.clone(), regions, crossfall_table: vec![] };
            baseline.check()?;
            for p in al.polyline(5.0) {
                match &mut bounds {
                    Some(bb) => bb.grow(p.0, p.1),
                    None => bounds = Some(Bounds::point(p.0, p.1)),
                }
            }
            baselines.push(baseline);
        }
        if baselines.is_empty() {
            return Err("Add a baseline.".into());
        }
        let name = if f.name.trim().is_empty() { "Corridor".to_string() } else { f.name.trim().to_string() };
        let data = CorridorData { model: CorridorModel { name: name.clone(), baselines, volumes: VolumeOptions::default() }, ground: ground.0, notes: vec![] };
        let v = serde_json::to_value(&data).unwrap_or_default();
        let id = match f.edit {
            Some(id) => {
                civil::set(&mut self.doc, id, v, bounds);
                if let Some(e) = self.doc.find_mut(id) {
                    e.name = name.clone();
                }
                id
            }
            None => civil::add(&mut self.doc, CORRIDOR, &name, v, bounds, "Built from the corridor model"),
        };
        self.doc.select(&[id], SelectMode::Replace);
        self.status = format!("Corridor '{name}' saved. It builds in the background; its top and datum surfaces appear under Surfaces.");
        self.schedule_commit_now();
        Ok(())
    }
}

/// One region of a baseline: its stations and assembly, the targets the assembly declares (each
/// bound here, with a message for one that is not yet), the surface the side slopes meet, the
/// imported superelevation it follows, and the frequency of sections. True when it is to be removed.
#[allow(clippy::too_many_arguments)]
fn region_ui(ui: &mut egui::Ui, bi: usize, ri: usize, r: &mut RgForm, asms: &[(ElementId, String)], surfaces: &[(ElementId, String)], ground: Option<ElementId>, objs: &DocObjs, al: Option<&afrobim_alignment::Alignment>, open_se: &mut Option<ElementId>, al_id: Option<ElementId>) -> bool {
    use afrobim_corridor::{Side, SubKind};
    let mut remove = false;
    let pos = NumRule::required().above(0.0);
    let title = format!("Region {}: {}", ri + 1, if r.name.trim().is_empty() { "unnamed" } else { r.name.trim() });
    ui_kit::section(ui, &title, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(130.0));
            ui.label("From");
            d::num(ui, &mut r.start, "m");
            ui.label("To");
            d::num(ui, &mut r.end, "m");
            ui.label("Assembly");
            d::pick_from(ui, &format!("rg-as{bi}-{ri}"), asms, &mut r.assembly);
        });
        ui_kit::heading(ui, "Targets");
        let (asm, refs) = (&mut r.asm, &mut r.refs);
        match asm {
            None => {
                ui_kit::callout(ui, Level::Warning, "Choose an assembly to see the targets it declares.");
            }
            Some(a) => {
                let decl = tg::declared(a);
                let day: Vec<(Side, usize, String)> = [Side::Left, Side::Right].into_iter().flat_map(|sd| a.side(sd).iter().enumerate().filter(|(_, p)| matches!(p.kind, SubKind::Daylight(_))).map(move |(i, p)| (sd, i, p.name.clone())).collect::<Vec<_>>()).collect();
                if decl.is_empty() && day.is_empty() {
                    ui.label(RichText::new("This assembly declares no targets: every width and link in it is entered in the assembly editor.").size(crate::theme::FONT_SMALL).weak());
                }
                for dcl in &decl {
                    let bound = tg::declared_status(a, dcl).is_none();
                    let side = if dcl.side == Side::Left { "Left" } else { "Right" };
                    let head = format!("{side} side, part {} ({}): {}{}", dcl.index + 1, dcl.part, dcl.label, if bound { "" } else { "  (not yet bound)" });
                    egui::CollapsingHeader::new(RichText::new(head).strong()).id_salt(("cor-tg", bi, ri, dcl.key())).default_open(!bound).show(ui, |ui| {
                        let rf = refs.entry(dcl.key()).or_default();
                        tg::declared_ui(ui, &format!("cor-tg-{bi}-{ri}-{}", dcl.key()), a, dcl, rf, objs);
                    });
                }
                for (sd, i, name) in &day {
                    let side = if *sd == Side::Left { "Left" } else { "Right" };
                    let shown = ground.and_then(|g| surfaces.iter().find(|s| s.0 == g)).map(|s| s.1.clone());
                    match shown {
                        Some(n) => ui.label(format!("{side} side, part {} ({name}): the side slope meets the ground surface '{n}' (chosen above).", i + 1)),
                        None => ui.label(RichText::new(format!("{side} side, part {} ({name}): the side slope needs a ground surface. Not yet bound: choose the ground surface above.", i + 1)).color(ui.visuals().warn_fg_color)),
                    };
                }
                let follows: Vec<String> = [Side::Left, Side::Right]
                    .into_iter()
                    .flat_map(|sd| a.side(sd).iter().filter(|p| matches!(&p.kind, SubKind::Lane(s) | SubKind::Shoulder(s) | SubKind::Footway(s) | SubKind::Verge(s) if s.follows_superelevation)).map(|p| format!("{} ({})", p.name, if sd == Side::Left { "left" } else { "right" })).collect::<Vec<_>>())
                    .collect();
                if !follows.is_empty() {
                    let stored = al.and_then(|a| a.superelevation.as_ref()).map(|se| se.sections.iter().map(|s| s.points.len()).sum::<usize>()).unwrap_or(0);
                    let msg = if stored > 0 { format!("Lanes that follow the alignment's cross-fall: {}. The cross-fall is read from the superelevation stored on the alignment ({stored} stations), so it is shown here and not edited here.", follows.join(", ")) } else { format!("Lanes that follow the alignment's cross-fall: {}. The alignment has no superelevation stored, so they keep the cross-slope of their own template.", follows.join(", ")) };
                    ui_kit::callout(ui, Level::Info, msg);
                    if let Some(id) = al_id {
                        if ui.link("Show the alignment's Superelevation tab").clicked() {
                            *open_se = Some(id);
                        }
                    }
                }
            }
        }
        ui_kit::heading(ui, "Frequency of sections");
        ui_kit::form(ui, &format!("cor-fq-{bi}-{ri}"), |fm| {
            fm.row_note("Spacing on tangents", "Sections stand on whole multiples of the spacing", |ui| {
                d::num_rule(ui, &mut r.spacing, "m", pos);
            });
            fm.row("Sections at geometry points", |ui| {
                ui_kit::toggle(ui, &mut r.at_geometry, "Every element, PVI and vertical curve end");
            });
            fm.row("Sections at critical stations", |ui| {
                ui_kit::toggle(ui, &mut r.at_critical, "Where cross-fall is stored (superelevation, crossfall table)");
            });
        });
        ui_kit::advanced(ui, &format!("cor-adv-{bi}-{ri}"), |ui| {
            ui_kit::form(ui, &format!("cor-fq2-{bi}-{ri}"), |fm| {
                fm.row_note("Spacing on curves", "Blank: the spacing on tangents", |ui| {
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!r.curve_none, |ui| {
                            let _ = d::num_opt_rule(ui, &mut r.curve, "m", &pos);
                        });
                        ui_kit::toggle(ui, &mut r.curve_none, "No regular sections");
                    });
                });
                fm.row_note("Spacing on spirals", "Blank: the spacing on tangents", |ui| {
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!r.spiral_none, |ui| {
                            let _ = d::num_opt_rule(ui, &mut r.spiral, "m", &pos);
                        });
                        ui_kit::toggle(ui, &mut r.spiral_none, "No regular sections");
                    });
                });
                fm.row_note("Extra stations", "Comma separated", |ui| {
                    ui_kit::text_field(ui, &mut r.extra, "e.g. 120, 340.5");
                });
            });
        });
        if ui.small_button("Remove region").clicked() {
            remove = true;
        }
    });
    remove
}

// ─── sample lines ──────────────────────────────────────────────────────────

pub struct SampForm {
    edit: Option<ElementId>,
    name: String,
    alignment: Option<ElementId>,
    group: Option<SampleLineGroup>,
    corridor: Option<ElementId>,
    surfaces: Vec<ElementId>,
    spec: usize,
    interval: String,
    listed: String,
    left: String,
    right: String,
    prefix: String,
    columns: String,
    exaggeration: String,
    e: String,
    n: String,
    message: String,
}

impl AfroBimApp {
    fn samples_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cor.samples.take() else { return };
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let corridors = civil::of_category(&self.doc, CORRIDOR);
        let surfaces = civil::surface_choices(&self.doc);
        let al = f.alignment.and_then(|id| crate::alignment_glue::alignment_of(&self.doc, id));
        let pos = NumRule::required().above(0.0);
        let mut add_now = false;
        let cols_ok = d::num_rule_peek(&f.columns, &NumRule::required().min(1.0)).is_some() && d::num_rule_peek(&f.exaggeration, &pos).is_some() && f.e.trim().parse::<f64>().is_ok() && f.n.trim().parse::<f64>().is_ok();
        let has_lines = f.group.as_ref().is_some_and(|g| !g.lines.is_empty());
        let out = Dialog::new("Sample line group", crate::glyphs::by_name("SAMPLELINES")).width(740.0).resizable(true).max_body_height(580.0).help("A sample line is a line across the alignment at a station with a swath to the left and to the right. The sections cut at each line show the surfaces you tick and, when a corridor is chosen, its design. Swath widths are inputs for the whole group when you add stations, and can then be edited line by line below.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "sg", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Sample lines 1"));
                    fm.row("Alignment", |ui| {
                        let before = f.alignment;
                        d::pick_from(ui, "sg-al", &aligns, &mut f.alignment);
                        if before != f.alignment {
                            f.group = None;
                        }
                    });
                    fm.row_note("Corridor design", "Optional: the design is drawn in the sections", |ui| {
                        let mut sel = f.corridor;
                        ui.horizontal(|ui| {
                            d::pick_from(ui, "sg-cor", &corridors, &mut sel);
                            if ui.small_button("None").clicked() {
                                sel = None;
                            }
                        });
                        f.corridor = sel;
                    });
                    fm.row("Surfaces cut", |ui| {
                        ui.vertical(|ui| {
                            for (id, name) in &surfaces {
                                let mut on = f.surfaces.contains(id);
                                if ui.checkbox(&mut on, name).changed() {
                                    if on {
                                        f.surfaces.push(*id);
                                    } else {
                                        f.surfaces.retain(|s| s != id);
                                    }
                                }
                            }
                        });
                    });
                });
                ui_kit::section(ui, "Add sample lines", |ui| {
                    ui_kit::segmented(ui, &mut f.spec, &["At an interval", "At listed stations", "At geometry points", "At superelevation stations"]);
                    ui_kit::form(ui, "sg-add", |fm| {
                        if f.spec == 0 {
                            fm.row("Interval", |ui| {
                                d::num_rule(ui, &mut f.interval, "m", pos);
                            });
                        }
                        if f.spec == 1 {
                            fm.row_note("Stations", "Separated by commas or spaces", |ui| d::text(ui, &mut f.listed, "e.g. 0, 50, 120.5"));
                        }
                        fm.row("Swath to the left", |ui| {
                            d::num_rule(ui, &mut f.left, "m", pos);
                        });
                        fm.row("Swath to the right", |ui| {
                            d::num_rule(ui, &mut f.right, "m", pos);
                        });
                        fm.row("Name prefix", |ui| d::text(ui, &mut f.prefix, ""));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Add the lines").clicked() {
                            add_now = true;
                        }
                        if ui.button("Example values").on_hover_text("30 m interval, 18 m swaths: demonstration numbers, not from any standard").clicked() {
                            f.interval = "30".into();
                            f.left = "18".into();
                            f.right = "18".into();
                            f.columns = "4".into();
                            f.exaggeration = "2".into();
                        }
                    });
                });
                if let Some(g) = &mut f.group {
                    ui_kit::section(ui, &format!("Lines ({})", g.lines.len()), |ui| {
                        let mut remove = None;
                        egui::ScrollArea::vertical().max_height(180.0).id_salt("sg-lines").show(ui, |ui| {
                            egui::Grid::new("sg-grid").num_columns(5).spacing([8.0, 3.0]).striped(true).show(ui, |ui| {
                                for h in ["Name", "Station (m)", "Left swath (m)", "Right swath (m)", ""] {
                                    ui.label(RichText::new(h).small().strong());
                                }
                                ui.end_row();
                                for i in 0..g.lines.len() {
                                    let mut name = g.lines[i].name.clone();
                                    let (mut l, mut r) = (g.lines[i].left_swath_m, g.lines[i].right_swath_m);
                                    if ui.add(egui::TextEdit::singleline(&mut name).desired_width(70.0)).changed() {
                                        let _ = g.rename_line(i, &name);
                                    }
                                    ui.label(format!("{:.3}", g.lines[i].station_m));
                                    let a = ui.add(egui::DragValue::new(&mut l).speed(0.5).range(0.1..=1000.0)).changed();
                                    let b = ui.add(egui::DragValue::new(&mut r).speed(0.5).range(0.1..=1000.0)).changed();
                                    if a || b {
                                        let _ = g.set_swath(i, l, r);
                                    }
                                    if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                                        remove = Some(i);
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                        if let Some(i) = remove {
                            g.remove_line(i);
                        }
                    });
                }
                ui_kit::section(ui, "Section views in the model", |ui| {
                    ui_kit::form(ui, "sg-views", |fm| {
                        fm.row("Columns", |ui| {
                            d::num_rule(ui, &mut f.columns, "", NumRule::required().min(1.0));
                        });
                        fm.row("Vertical exaggeration", |ui| {
                            d::num_rule(ui, &mut f.exaggeration, "", pos);
                        });
                        fm.row("Insertion easting", |ui| {
                            d::num(ui, &mut f.e, "m");
                        });
                        fm.row("Insertion northing", |ui| {
                            d::num(ui, &mut f.n, "m");
                        });
                    });
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| Footer::new("Save sample line group").icon(crate::glyphs::by_name("SAMPLELINES")).enabled(has_lines && cols_ok, "Add lines and complete the section view fields").show(ui),
        );
        if add_now {
            if let Some(a) = &al {
                let (l, r) = (f.left.trim().parse::<f64>(), f.right.trim().parse::<f64>());
                let spec = match f.spec {
                    0 => f.interval.trim().parse::<f64>().ok().map(|i| SampleStations::Interval { interval_m: i, start_station_m: None, end_station_m: None }),
                    1 => {
                        let v: Vec<f64> = f.listed.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
                        (!v.is_empty()).then_some(SampleStations::Listed(v))
                    }
                    2 => Some(SampleStations::GeometryPoints),
                    _ => Some(SampleStations::SuperelevationStations),
                };
                match (spec, l, r) {
                    (Some(spec), Ok(l), Ok(r)) => {
                        let g = f.group.get_or_insert_with(|| SampleLineGroup::new(if f.name.trim().is_empty() { "Sample lines" } else { f.name.trim() }, a));
                        f.message = match g.add_stations(a, &spec, l, r, &f.prefix) {
                            Ok(n) => format!("Added {n} line(s)."),
                            Err(e) => e,
                        };
                    }
                    _ => f.message = "Enter the interval or stations and both swaths first.".into(),
                }
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let Some(mut group) = f.group.clone() {
                if !f.name.trim().is_empty() {
                    group.name = f.name.trim().to_string();
                }
                let data = SampleData { group, corridor: f.corridor.map(|c| c.0), surfaces: f.surfaces.iter().map(|s| s.0).collect(), insertion: [f.e.trim().parse().unwrap_or(0.0), f.n.trim().parse().unwrap_or(0.0)], columns: f.columns.trim().parse().unwrap_or(1), vertical_exaggeration: f.exaggeration.trim().parse().unwrap_or(1.0), slices: vec![], imported: vec![], compare: false };
                let v = serde_json::to_value(&data).unwrap_or_default();
                let b = al.as_ref().and_then(|a| Bounds::of(a.polyline(5.0).into_iter()));
                let name = if f.name.trim().is_empty() { "Sample lines".to_string() } else { f.name.trim().to_string() };
                match f.edit {
                    Some(id) => {
                        civil::set(&mut self.doc, id, v, b);
                        if let Some(e) = self.doc.find_mut(id) {
                            e.name = name.clone();
                        }
                    }
                    None => {
                        let id = civil::add(&mut self.doc, SAMPLES, &name, v, b, "Sample line group");
                        self.doc.select(&[id], SelectMode::Replace);
                    }
                }
                self.status = format!("Sample line group '{name}' saved: its lines are in the plan and its sections are drawn at the insertion point.");
                self.schedule_commit_now();
            }
            return;
        }
        self.civil_ui.cor.samples = Some(f);
    }
}

// ─── section views ─────────────────────────────────────────────────────────

pub struct ViewsForm {
    id: ElementId,
    columns: String,
    exaggeration: String,
    e: String,
    n: String,
}

impl AfroBimApp {
    fn views_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.cor.views.take() else { return };
        let name = self.doc.find(f.id).map(|e| e.name.clone()).unwrap_or_default();
        let pos = NumRule::required().above(0.0);
        let vals = (d::num_rule_peek(&f.columns, &NumRule::required().min(1.0)), d::num_rule_peek(&f.exaggeration, &pos), f.e.trim().parse::<f64>().ok(), f.n.trim().parse::<f64>().ok());
        let ok = vals.0.is_some() && vals.1.is_some() && vals.2.is_some() && vals.3.is_some();
        let out = Dialog::new("Section views", crate::glyphs::by_name("SECTIONVIEWS")).id_title(format!("Section views: {name}")).subtitle(name.clone()).width(460.0).help("The sections of the sample line group are drawn in the model as an array of views, left to right and then downward from the insertion point (its upper left corner).").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "sv", |fm| {
                    fm.row("Columns", |ui| {
                        d::num_rule(ui, &mut f.columns, "", NumRule::required().min(1.0));
                    });
                    fm.row("Vertical exaggeration", |ui| {
                        d::num_rule(ui, &mut f.exaggeration, "", pos);
                    });
                    fm.row("Insertion easting", |ui| {
                        d::num(ui, &mut f.e, "m");
                    });
                    fm.row("Insertion northing", |ui| {
                        d::num(ui, &mut f.n, "m");
                    });
                });
            },
            |ui| Footer::new("Draw the views").icon(crate::glyphs::by_name("SECTIONVIEWS")).enabled(ok, "Complete the fields").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let (Some(mut sd), (Some(c), Some(ex), Some(e), Some(n))) = (civil::get::<SampleData>(&self.doc, f.id, SAMPLES), vals) {
                sd.columns = c as usize;
                sd.vertical_exaggeration = ex;
                sd.insertion = [e, n];
                civil::set(&mut self.doc, f.id, serde_json::to_value(&sd).unwrap_or_default(), None);
                self.doc.select(&[f.id], SelectMode::Replace);
                self.status = "The section views are drawn in the model.".into();
                self.schedule_commit_now();
            }
            return;
        }
        self.civil_ui.cor.views = Some(f);
    }
}
