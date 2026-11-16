//! The corridor from the new model (baseline, regions with an assembly each, a frequency and a
//! ground surface), sample line groups with a per-line swath edit, and the section view array.
//! The older CORRIDOR command, with its single typical section, stays for old documents.

use afrobim_corridor::{Assembly, AssemblyRegion, Baseline, CorridorModel, Frequency, SampleLineGroup, SampleStations, VolumeOptions};
use afrobim_kernel::{ElementId, SelectMode};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::civil::{self, Bounds, CorridorData, SampleData, ASSEMBLY, CORRIDOR, SAMPLES};
use crate::civil_dlg::{self as d};
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
    spacing: String,
    at_geometry: bool,
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
                                regions: b.regions.iter().map(|r| RgForm { name: r.name.clone(), start: r.start_station_m.to_string(), end: r.end_station_m.to_string(), assembly: civil::of_category(&self.doc, ASSEMBLY).into_iter().find(|(aid, _)| self.assembly_of(*aid).is_some_and(|a| a == r.assembly)).map(|x| x.0), spacing: r.frequency.tangent_m.map(|v| v.to_string()).unwrap_or_default(), at_geometry: r.frequency.at_geometry_points }).collect(),
                            })
                            .collect(),
                        message: String::new(),
                    },
                    None => {
                        let al = aligns.first().map(|a| a.0);
                        let (s0, s1) = al.and_then(|i| crate::alignment_glue::alignment_of(&self.doc, i)).map(|a| (a.start_station_m, a.end_station_m())).unwrap_or((0.0, 0.0));
                        CorForm {
                            edit: None,
                            name: String::new(),
                            ground: civil::surface_choices(&self.doc).first().map(|s| s.0),
                            baselines: vec![BlForm { name: "Baseline 1".into(), alignment: al, regions: vec![RgForm { name: "Region 1".into(), start: s0.to_string(), end: s1.to_string(), assembly: civil::of_category(&self.doc, ASSEMBLY).first().map(|a| a.0), spacing: String::new(), at_geometry: true }] }],
                            message: String::new(),
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
        let pos = NumRule::required().above(0.0);
        let out = Dialog::new(if f.edit.is_some() { "Edit corridor" } else { "Corridor" }, crate::glyphs::by_name("CORRIDORMODEL")).width(760.0).resizable(true).max_body_height(560.0).help("A corridor has baselines (an alignment with a design profile), each with regions that follow one another along the stations. A region has an assembly and a frequency: the spacing of sections in tangents, curves and spirals, with sections also at geometry points when ticked. The side slopes of the assembly's daylight part meet the ground surface chosen here. Everything is an input; Example values fills demonstration numbers.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "cor", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Main road corridor"));
                    fm.row("Ground surface (target)", |ui| {
                        d::pick_from(ui, "cor-g", &surfaces, &mut f.ground);
                    });
                });
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
                        egui::Grid::new(("cor-regions", bi)).num_columns(7).spacing([6.0, 4.0]).show(ui, |ui| {
                            for h in ["Region", "From (m)", "To (m)", "Assembly", "Spacing (m)", "At geometry points", ""] {
                                ui.label(RichText::new(h).small().strong());
                            }
                            ui.end_row();
                            for (ri, r) in b.regions.iter_mut().enumerate() {
                                ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(80.0));
                                d::num(ui, &mut r.start, "");
                                d::num(ui, &mut r.end, "");
                                d::pick_from(ui, &format!("rg-as{bi}-{ri}"), &asms, &mut r.assembly);
                                d::num_rule(ui, &mut r.spacing, "", pos);
                                ui.checkbox(&mut r.at_geometry, "");
                                if ui.small_button("x").clicked() {
                                    remove_rg = Some(ri);
                                }
                                ui.end_row();
                            }
                        });
                        if let Some(i) = remove_rg {
                            b.regions.remove(i);
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Add region").clicked() {
                                let start = b.regions.last().map(|r| r.end.clone()).unwrap_or_default();
                                b.regions.push(RgForm { name: format!("Region {}", b.regions.len() + 1), start, end: String::new(), assembly: asms.first().map(|a| a.0), spacing: String::new(), at_geometry: true });
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
                let spacing = num(&r.spacing, "spacing")?;
                let assembly = r.assembly.and_then(|id| self.assembly_of(id)).ok_or_else(|| format!("{} / {}: choose an assembly.", b.name, r.name))?;
                let mut freq = Frequency::uniform(spacing);
                freq.at_geometry_points = r.at_geometry;
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
                                    if ui.small_button("x").clicked() {
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
                let data = SampleData { group, corridor: f.corridor.map(|c| c.0), surfaces: f.surfaces.iter().map(|s| s.0).collect(), insertion: [f.e.trim().parse().unwrap_or(0.0), f.n.trim().parse().unwrap_or(0.0)], columns: f.columns.trim().parse().unwrap_or(1), vertical_exaggeration: f.exaggeration.trim().parse().unwrap_or(1.0) };
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
