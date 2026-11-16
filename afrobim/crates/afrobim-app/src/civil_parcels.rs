//! Parcels (the site): create from corner points or from the courses of a deed, properties,
//! topology check, split by a road reserve, and the land acquisition schedule with CSV.

use afrobim_cogo::{analyse_site, area_table_csv, schedule_csv, split_parcel, AcquisitionRow, Boundary, DeedCall, Parcel, ParcelStatus, Side, SplitOptions, TopologyOptions, Xy};
use afrobim_kernel::{ElementId, SelectMode};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::civil::{self, Bounds, PARCEL};
use crate::civil_dlg::{self as d, fmt};
use crate::civil_scene::SLine;
use crate::ui_kit::{self, Col, Dialog, Footer, Level, NumRule};

#[derive(Default)]
pub struct ParcelUi {
    pub pick: Option<PickForm>,
    pub deed: Option<DeedForm>,
    pub props: Option<PropsForm>,
    pub topo: Option<TopoForm>,
    pub split: Option<SplitForm>,
    pub sched: Option<SchedForm>,
}

const STATUSES: [&str; 4] = ["Existing", "To be acquired", "Acquired", "Other"];

fn status_index(s: &ParcelStatus) -> usize {
    match s {
        ParcelStatus::Existing => 0,
        ParcelStatus::ToBeAcquired => 1,
        ParcelStatus::Acquired => 2,
        ParcelStatus::Other(_) => 3,
    }
}

fn status_of(i: usize, other: &str) -> ParcelStatus {
    match i {
        0 => ParcelStatus::Existing,
        1 => ParcelStatus::ToBeAcquired,
        2 => ParcelStatus::Acquired,
        _ => ParcelStatus::Other(other.to_string()),
    }
}

impl AfroBimApp {
    pub(crate) fn parcels_all(&self) -> Vec<(ElementId, Parcel)> {
        self.doc.elements().iter().filter(|e| e.category == PARCEL).filter_map(|e| serde_json::from_value::<Parcel>(e.data.clone()).ok().map(|p| (e.id, p))).collect()
    }

    pub(crate) fn add_parcel_element(&mut self, p: &Parcel, source: &str) -> Option<ElementId> {
        let id = crate::civil_landxml::add_parcel(&mut self.doc, p, source)?;
        self.schedule_commit_now();
        Some(id)
    }

    fn next_parcel_number(&self) -> u32 {
        self.parcels_all().iter().map(|p| p.1.number).max().unwrap_or(0) + 1
    }

    pub(crate) fn civil_parcel_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        use crate::civil_cmds::Cv::*;
        let parcels = self.parcels_all();
        let sel = self.doc.primary().filter(|id| parcels.iter().any(|p| p.0 == *id)).or_else(|| parcels.first().map(|p| p.0));
        match cv {
            ParcelPick => self.civil_ui.parcels.pick = Some(PickForm { name: String::new(), rows: vec![(String::new(), String::new()); 3], set: 0, numbers: String::new(), message: String::new() }),
            ParcelDeed => self.civil_ui.parcels.deed = Some(DeedForm { name: String::new(), e: String::new(), n: String::new(), calls: vec![DeedRow::default(); 3], tol: String::new(), message: String::new() }),
            _ if parcels.is_empty() => self.status = "There are no parcels yet. Create one (PARCELPICK or PARCELDEED) or import them from LandXML.".into(),
            ParcelProps => {
                if let Some((id, p)) = sel.and_then(|id| parcels.iter().find(|x| x.0 == id).cloned()) {
                    self.civil_ui.parcels.props = Some(PropsForm { id, name: p.name.clone(), number: p.number.to_string(), owner: p.owner.clone(), land_ref: p.land_reference.clone(), land_use: p.land_use.clone(), status: status_index(&p.status), other: if let ParcelStatus::Other(s) = &p.status { s.clone() } else { String::new() }, attrs: p.attributes.iter().map(|(k, v)| (k.clone(), v.clone())).collect() });
                }
            }
            ParcelTopology => self.civil_ui.parcels.topo = Some(TopoForm { snap: String::new(), arc: String::new(), min_area: String::new(), report: None }),
            ParcelSplit => self.civil_ui.parcels.split = Some(SplitForm { alignment: crate::alignment_glue::alignments(&self.doc).first().map(|a| a.0), left: String::new(), right: String::new(), snap: String::new(), arc: String::new(), message: String::new() }),
            _ => self.civil_ui.parcels.sched = Some(SchedForm { m2: "2".into(), ha: "4".into(), acres: false, only_affected: false, message: String::new() }),
        }
    }

    pub(crate) fn civil_parcel_windows(&mut self, ctx: &egui::Context) {
        self.pick_parcel_window(ctx);
        self.deed_window(ctx);
        self.props_window(ctx);
        self.topo_window(ctx);
        self.split_window(ctx);
        self.sched_window(ctx);
    }
}

// ─── by points ─────────────────────────────────────────────────────────────

pub struct PickForm {
    name: String,
    rows: Vec<(String, String)>,
    set: usize,
    numbers: String,
    message: String,
}

impl AfroBimApp {
    /// A click in the model adds a corner to the open parcel dialog.
    pub(crate) fn parcel_pick_vertex(&mut self, e: f64, n: f64) {
        if let Some(f) = &mut self.civil_ui.parcels.pick {
            let blank = f.rows.iter().position(|r| r.0.trim().is_empty() && r.1.trim().is_empty());
            let row = (format!("{e:.3}"), format!("{n:.3}"));
            match blank {
                Some(i) => f.rows[i] = row,
                None => f.rows.push(row),
            }
        }
    }

    fn pick_parcel_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.parcels.pick.take() else { return };
        let sets = self.point_sets();
        let corners: Vec<Xy> = f.rows.iter().filter_map(|(e, n)| Some(Xy::new(e.trim().parse().ok()?, n.trim().parse().ok()?))).collect();
        let ok = corners.len() >= 3 || !f.numbers.trim().is_empty();
        let mut pick = false;
        let out = Dialog::new("Create parcel", crate::glyphs::by_name("PARCELPICK")).width(520.0).help("Type the corners (easting, northing) in order round the parcel, or press Pick and click them in the model, or give point numbers from a point set. The area is computed from the corners.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pp", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Plot 12"));
                });
                egui::Grid::new("pp-rows").num_columns(3).show(ui, |ui| {
                    ui.label("Easting");
                    ui.label("Northing");
                    ui.label("");
                    ui.end_row();
                    let mut remove = None;
                    for (i, (e, n)) in f.rows.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(e).desired_width(120.0));
                        ui.add(egui::TextEdit::singleline(n).desired_width(120.0));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                    if let Some(i) = remove {
                        f.rows.remove(i);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Add corner").clicked() {
                        f.rows.push((String::new(), String::new()));
                    }
                    if ui.button(format!("{}  Pick the next corner", egui_phosphor::regular::CROSSHAIR)).clicked() {
                        pick = true;
                    }
                });
                if !sets.is_empty() {
                    ui_kit::section(ui, "Or from point numbers", |ui| {
                        ui.horizontal(|ui| {
                            let names: Vec<String> = sets.iter().map(|s| s.1.clone()).collect();
                            ui_kit::combo_owned(ui, "pp-set", &mut f.set, &names);
                            d::text(ui, &mut f.numbers, "e.g. 5 6 22 21");
                        });
                    });
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| Footer::new("Create parcel").icon(crate::glyphs::by_name("PARCELPICK")).enabled(ok, "Give at least three corners").show(ui),
        );
        if pick {
            self.civil_ui.pick = Some(crate::civil_ui::PickFor::ParcelCorner);
            self.status = "Click the corners of the parcel in the model.".into();
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.pick = None;
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            let mut pts = corners.clone();
            if !f.numbers.trim().is_empty() {
                pts.clear();
                if let Some(data) = sets.get(f.set).and_then(|s| self.points_data(s.0)) {
                    for t in f.numbers.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()) {
                        match t.parse::<u32>().ok().and_then(|n| data.store.get(n)) {
                            Some(p) => pts.push(p.xy()),
                            None => {
                                f.message = format!("Point '{t}' is not in the set.");
                                pts.clear();
                                break;
                            }
                        }
                    }
                }
            }
            if pts.len() >= 3 {
                match Boundary::polygon(&pts) {
                    Ok(bd) => {
                        let number = self.next_parcel_number();
                        let name = if f.name.trim().is_empty() { format!("Parcel {number}") } else { f.name.trim().to_string() };
                        let p = Parcel::new(&name, number, bd);
                        if let Some(id) = self.add_parcel_element(&p, "Corner points") {
                            self.doc.select(&[id], SelectMode::Replace);
                            self.status = format!("Created {name}: area {:.2} m2.", p.area().unwrap_or(0.0));
                        }
                        self.civil_ui.pick = None;
                        return;
                    }
                    Err(e) => f.message = e.to_string(),
                }
            }
        }
        self.civil_ui.parcels.pick = Some(f);
    }
}

// ─── deed ──────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct DeedRow {
    curve: bool,
    bearing: String,
    distance: String,
    radius: String,
    left: bool,
}

pub struct DeedForm {
    name: String,
    e: String,
    n: String,
    calls: Vec<DeedRow>,
    tol: String,
    message: String,
}

impl DeedForm {
    fn calls(&self) -> Result<Vec<DeedCall>, String> {
        let mut out = Vec::new();
        for (i, r) in self.calls.iter().enumerate() {
            let b = afrobim_cogo::parse_bearing(&r.bearing).map_err(|_| format!("Course {}: the bearing '{}' is not understood.", i + 1, r.bearing))?;
            let dist = r.distance.trim().parse::<f64>().ok().filter(|v| *v > 0.0).ok_or_else(|| format!("Course {}: enter a distance greater than 0.", i + 1))?;
            if r.curve {
                let radius = r.radius.trim().parse::<f64>().ok().filter(|v| *v > 0.0).ok_or_else(|| format!("Course {}: enter the radius.", i + 1))?;
                out.push(DeedCall::Curve { radius, turn: if r.left { afrobim_cogo::Turn::Left } else { afrobim_cogo::Turn::Right }, chord_bearing_deg: b, chord_length: dist, large: false });
            } else {
                out.push(DeedCall::Line { bearing_deg: b, distance: dist });
            }
        }
        Ok(out)
    }
}

impl AfroBimApp {
    fn deed_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.parcels.deed.take() else { return };
        let start = f.e.trim().parse::<f64>().ok().zip(f.n.trim().parse::<f64>().ok()).map(|(e, n)| Xy::new(e, n));
        let calls = f.calls();
        let tol = d::num_rule_peek(&f.tol, &NumRule::required().above(0.0));
        let closure = match (&start, &calls) {
            (Some(s), Ok(c)) if !c.is_empty() => Some(Boundary::deed_closure(*s, c)),
            _ => None,
        };
        let ok = start.is_some() && calls.as_ref().is_ok_and(|c| c.len() >= 2) && tol.is_some();
        let out = Dialog::new("Parcel from a deed", crate::glyphs::by_name("PARCELDEED")).width(640.0).resizable(true).max_body_height(560.0).help("The courses of the deed in order from the point of beginning: a bearing and a distance for a line, or for a curve the chord bearing, chord length and radius. The closure report shows how well the courses close. The closure tolerance is your requirement (the deed or the regulations): the parcel is created only when the misclosure is within it, and the courses are not adjusted.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "dd", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Plot 12"));
                    fm.row("Point of beginning, easting", |ui| {
                        d::num(ui, &mut f.e, "m");
                    });
                    fm.row("Point of beginning, northing", |ui| {
                        d::num(ui, &mut f.n, "m");
                    });
                    fm.row_note("Closure tolerance", "Largest misclosure accepted, in metres", |ui| {
                        d::num_rule(ui, &mut f.tol, "m", NumRule::required().above(0.0));
                    });
                });
                egui::Grid::new("dd-rows").num_columns(6).spacing([6.0, 4.0]).show(ui, |ui| {
                    for h in ["Kind", "Bearing", "Distance / chord (m)", "Radius (m)", "Curves left", ""] {
                        ui.label(RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    let mut remove = None;
                    for (i, r) in f.calls.iter_mut().enumerate() {
                        let mut k = r.curve as usize;
                        ui_kit::combo(ui, &format!("dd-k{i}"), &mut k, &["Line", "Curve"]);
                        r.curve = k == 1;
                        ui.add(egui::TextEdit::singleline(&mut r.bearing).desired_width(130.0).hint_text("N45-30-00E or 45.5"));
                        ui.add(egui::TextEdit::singleline(&mut r.distance).desired_width(90.0));
                        ui.add_enabled(r.curve, egui::TextEdit::singleline(&mut r.radius).desired_width(70.0));
                        ui.add_enabled(r.curve, egui::Checkbox::new(&mut r.left, ""));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                    if let Some(i) = remove {
                        f.calls.remove(i);
                    }
                });
                if ui.button("Add course").clicked() {
                    f.calls.push(DeedRow::default());
                }
                match (&calls, &closure) {
                    (Err(e), _) => {
                        ui_kit::callout(ui, Level::Warning, e.clone());
                    }
                    (_, Some(Ok((_, c)))) => {
                        ui_kit::results_table(ui, "dd-c", &["Closure", "Value"], &[Col::Text, Col::Num], &[vec!["Error in easting (m)".into(), fmt(c.error_e, 4)], vec!["Error in northing (m)".into(), fmt(c.error_n, 4)], vec!["Linear misclosure (m)".into(), fmt(c.linear, 4)], vec!["Perimeter (m)".into(), fmt(c.perimeter, 3)], vec!["Precision".into(), c.precision_denominator.map(|p| format!("1 in {p:.0}")).unwrap_or_else(|| "not defined".into())]]);
                        if let Some(t) = tol {
                            ui_kit::callout(ui, if c.linear <= t { Level::Info } else { Level::Warning }, if c.linear <= t { format!("Within the tolerance of {t} m.") } else { format!("The misclosure of {:.4} m is more than the tolerance of {t} m: the parcel will not be created.", c.linear) });
                        }
                    }
                    (_, Some(Err(e))) => {
                        ui_kit::callout(ui, Level::Warning, e.to_string());
                    }
                    _ => {}
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Warning, f.message.clone());
                }
            },
            |ui| Footer::new("Create parcel").icon(crate::glyphs::by_name("PARCELDEED")).enabled(ok, "Enter the start, at least two courses and the tolerance").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let (Some(s), Ok(c), Some(t)) = (start, &calls, tol) {
                match Boundary::from_deed(s, c, t) {
                    Ok((bd, cl)) => {
                        let number = self.next_parcel_number();
                        let name = if f.name.trim().is_empty() { format!("Parcel {number}") } else { f.name.trim().to_string() };
                        let mut p = Parcel::new(&name, number, bd);
                        p.attributes.insert("Deed closure (m)".into(), format!("{:.4}", cl.linear));
                        if let Some(id) = self.add_parcel_element(&p, "Deed courses") {
                            self.doc.select(&[id], SelectMode::Replace);
                            self.status = format!("Created {name} from the deed: area {:.2} m2, misclosure {:.4} m.", p.area().unwrap_or(0.0), cl.linear);
                        }
                        return;
                    }
                    Err(e) => f.message = e.to_string(),
                }
            }
        }
        self.civil_ui.parcels.deed = Some(f);
    }
}

// ─── properties ────────────────────────────────────────────────────────────

pub struct PropsForm {
    id: ElementId,
    name: String,
    number: String,
    owner: String,
    land_ref: String,
    land_use: String,
    status: usize,
    other: String,
    attrs: Vec<(String, String)>,
}

impl AfroBimApp {
    fn props_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.parcels.props.take() else { return };
        let parcels = self.parcels_all();
        let area = parcels.iter().find(|p| p.0 == f.id).and_then(|p| p.1.area().ok());
        let ok = !f.name.trim().is_empty() && f.number.trim().parse::<u32>().is_ok();
        let out = Dialog::new("Parcel properties", crate::glyphs::by_name("PARCELPROPS")).id_title(format!("Parcel properties: {}", f.name)).subtitle(f.name.clone()).width(520.0).show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pr", |fm| {
                    fm.row("Parcel", |ui| {
                        let mut sel = Some(f.id);
                        let items: Vec<(ElementId, String)> = parcels.iter().map(|p| (p.0, p.1.name.clone())).collect();
                        if d::pick_from(ui, "pr-sel", &items, &mut sel) {
                            if let Some((id, p)) = sel.and_then(|s| parcels.iter().find(|x| x.0 == s)) {
                                *&mut f = PropsForm { id: *id, name: p.name.clone(), number: p.number.to_string(), owner: p.owner.clone(), land_ref: p.land_reference.clone(), land_use: p.land_use.clone(), status: status_index(&p.status), other: String::new(), attrs: p.attributes.iter().map(|(k, v)| (k.clone(), v.clone())).collect() };
                            }
                        }
                    });
                    fm.row("Name", |ui| d::text(ui, &mut f.name, ""));
                    fm.row("Parcel number", |ui| d::text(ui, &mut f.number, ""));
                    fm.row("Owner", |ui| d::text(ui, &mut f.owner, ""));
                    fm.row("Land reference", |ui| d::text(ui, &mut f.land_ref, "title or plot reference"));
                    fm.row("Land use", |ui| d::text(ui, &mut f.land_use, ""));
                    fm.row("Status", |ui| {
                        ui_kit::combo(ui, "pr-status", &mut f.status, &STATUSES);
                    });
                    if f.status == 3 {
                        fm.row("Status, in words", |ui| d::text(ui, &mut f.other, ""));
                    }
                    fm.row("Area", |ui| {
                        ui.label(area.map(|a| format!("{a:.2} m2  ({:.4} ha)", a / afrobim_cogo::HECTARE_M2)).unwrap_or_else(|| "not computed".into()));
                    });
                });
                ui.label(RichText::new("Other fields").strong());
                let mut remove = None;
                for (i, (k, v)) in f.attrs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(k).desired_width(140.0));
                        ui.add(egui::TextEdit::singleline(v).desired_width(220.0));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    f.attrs.remove(i);
                }
                if ui.small_button("Add field").clicked() {
                    f.attrs.push((String::new(), String::new()));
                }
            },
            |ui| Footer::new("Apply").icon(crate::glyphs::by_name("PARCELPROPS")).enabled(ok, "Name the parcel and give a whole number").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let Some((_, mut p)) = parcels.into_iter().find(|p| p.0 == f.id) {
                p.name = f.name.trim().to_string();
                p.number = f.number.trim().parse().unwrap_or(p.number);
                p.owner = f.owner.clone();
                p.land_reference = f.land_ref.clone();
                p.land_use = f.land_use.clone();
                p.status = status_of(f.status, f.other.trim());
                p.attributes = f.attrs.iter().filter(|(k, _)| !k.trim().is_empty()).map(|(k, v)| (k.trim().to_string(), v.clone())).collect();
                civil::set(&mut self.doc, f.id, serde_json::to_value(&p).unwrap_or_default(), None);
                if let Some(e) = self.doc.find_mut(f.id) {
                    e.name = p.name.clone();
                }
                self.schedule_commit_now();
                self.status = format!("Updated {}.", p.name);
            }
            return;
        }
        self.civil_ui.parcels.props = Some(f);
    }
}

// ─── topology ──────────────────────────────────────────────────────────────

pub struct TopoForm {
    snap: String,
    arc: String,
    min_area: String,
    report: Option<Result<afrobim_cogo::TopologyReport, String>>,
}

impl AfroBimApp {
    fn topo_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.parcels.topo.take() else { return };
        let pos = NumRule::required().above(0.0);
        let vals = (d::num_rule_peek(&f.snap, &pos), d::num_rule_peek(&f.arc, &pos), d::num_rule_peek(&f.min_area, &NumRule::required().min(0.0)));
        let ok = vals.0.is_some() && vals.1.is_some() && vals.2.is_some();
        let mut draw = false;
        let out = Dialog::new("Topology check", crate::glyphs::by_name("PARCELTOPOLOGY")).width(560.0).help("Finds the boundaries parcels share, and the gaps and overlaps between them. The three tolerances are yours: how close two boundary points must be to count as the same, how closely curves are flattened, and the smallest gap or overlap area worth reporting.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "tp", |fm| {
                    fm.row("Snap tolerance", |ui| {
                        d::num_rule(ui, &mut f.snap, "m", pos);
                    });
                    fm.row("Curve flattening", |ui| {
                        d::num_rule(ui, &mut f.arc, "m", pos);
                    });
                    fm.row("Smallest area reported", |ui| {
                        d::num_rule(ui, &mut f.min_area, "m2", NumRule::required().min(0.0));
                    });
                });
                if ui.button("Example values").on_hover_text("0.01 m, 0.005 m, 0.1 m2: demonstration numbers, not from any standard").clicked() {
                    f.snap = "0.01".into();
                    f.arc = "0.005".into();
                    f.min_area = "0.1".into();
                }
                match &f.report {
                    Some(Ok(r)) => {
                        ui_kit::results_table(ui, "tp-r", &["Finding", "Count", "Area (m2)"], &[Col::Text, Col::Num, Col::Num], &[vec!["Shared boundaries".into(), r.shared.len().to_string(), String::new()], vec!["Gaps".into(), r.gaps.len().to_string(), fmt(r.total_gap_area_m2, 3)], vec!["Overlaps".into(), r.overlaps.len().to_string(), fmt(r.total_overlap_area_m2, 3)]]);
                        for l in r.summary_lines().iter().take(12) {
                            ui.label(RichText::new(l).small());
                        }
                        if ui.button("Draw the gaps and overlaps in the model").clicked() {
                            draw = true;
                        }
                    }
                    Some(Err(e)) => {
                        ui_kit::callout(ui, Level::Error, e.clone());
                    }
                    None => {}
                }
            },
            |ui| Footer::new("Check").icon(crate::glyphs::by_name("PARCELTOPOLOGY")).enabled(ok, "Enter the three tolerances").cancel(Some("Close")).show(ui),
        );
        if draw {
            if let Some(Ok(r)) = &f.report {
                self.civil_ui.overlay.lines.retain(|l| l.width != 2.2);
                for g in r.gaps.iter().chain(r.overlaps.iter()) {
                    let mut ring: Vec<[f64; 2]> = g.ring.iter().map(|p| [p.e * 1000.0, p.n * 1000.0]).collect();
                    if let Some(a) = ring.first().copied() {
                        ring.push(a);
                    }
                    self.civil_ui.overlay.lines.push(SLine::new(ring, [0xE0, 0x3C, 0xC8], 2.2));
                }
                self.status = "Gaps and overlaps are drawn in the model (a temporary overlay).".into();
            }
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let (Some(snap), Some(arc), Some(min)) = vals {
                let ps: Vec<Parcel> = self.parcels_all().into_iter().map(|p| p.1).collect();
                f.report = Some(analyse_site(&ps, &TopologyOptions { snap_tol_m: snap, arc_tol_m: arc, min_area_m2: min }).map_err(|e| e.to_string()));
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.parcels.topo = Some(f);
    }
}

// ─── split by a road reserve ───────────────────────────────────────────────

pub struct SplitForm {
    alignment: Option<ElementId>,
    left: String,
    right: String,
    snap: String,
    arc: String,
    message: String,
}

fn piece_parcel(src: &Parcel, ring: &[Xy], name: String, number: u32, status: ParcelStatus, note: &str) -> Option<Parcel> {
    let bd = Boundary::polygon(ring).ok()?;
    let mut p = Parcel::new(&name, number, bd);
    p.owner = src.owner.clone();
    p.land_reference = src.land_reference.clone();
    p.land_use = src.land_use.clone();
    p.status = status;
    p.attributes = src.attributes.clone();
    p.attributes.insert("Split from".into(), src.name.clone());
    p.attributes.insert("Piece".into(), note.to_string());
    Some(p)
}

impl AfroBimApp {
    fn split_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.parcels.split.take() else { return };
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let pos = NumRule::required().above(0.0);
        let vals = (d::num_rule_peek(&f.left, &pos), d::num_rule_peek(&f.right, &pos), d::num_rule_peek(&f.snap, &pos), d::num_rule_peek(&f.arc, &pos));
        let ok = f.alignment.is_some() && vals.0.is_some() && vals.1.is_some() && vals.2.is_some() && vals.3.is_some();
        let out = Dialog::new("Split by road reserve", crate::glyphs::by_name("PARCELSPLIT")).width(560.0).help("The road reserve is the band from the offset of the alignment to its left to the offset to its right. Every parcel the band crosses is split into the part inside it (the take, status To be acquired) and the rest (the remainder). The reserve widths and the tolerances are yours. The original parcels are hidden and kept; the new ones say which parcel they came from.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "ps", |fm| {
                    fm.row("Alignment", |ui| {
                        d::pick_from(ui, "ps-al", &aligns, &mut f.alignment);
                    });
                    fm.row_note("Reserve to the left", "From the centreline, in metres", |ui| {
                        d::num_rule(ui, &mut f.left, "m", pos);
                    });
                    fm.row_note("Reserve to the right", "From the centreline, in metres", |ui| {
                        d::num_rule(ui, &mut f.right, "m", pos);
                    });
                    fm.row("Snap tolerance", |ui| {
                        d::num_rule(ui, &mut f.snap, "m", pos);
                    });
                    fm.row("Curve flattening", |ui| {
                        d::num_rule(ui, &mut f.arc, "m", pos);
                    });
                });
                if ui.button("Example values").on_hover_text("15 m each side, 0.01 m, 0.005 m: demonstration numbers, not from any standard").clicked() {
                    f.left = "15".into();
                    f.right = "15".into();
                    f.snap = "0.01".into();
                    f.arc = "0.005".into();
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| Footer::new("Split the parcels").icon(crate::glyphs::by_name("PARCELSPLIT")).enabled(ok, "Complete every field").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let (Some(al), (Some(l), Some(r), Some(snap), Some(arc))) = (f.alignment.and_then(|id| crate::alignment_glue::alignment_of(&self.doc, id)), vals) {
                match split_by_reserve(&mut self.doc, &al, l, r, snap, arc) {
                    Ok((hidden, made)) => {
                        f.message = format!("Split {hidden} parcel(s) into {made} new parcel(s). The originals are hidden, not deleted.");
                        self.status = f.message.clone();
                        self.schedule_commit_now();
                    }
                    Err(e) => f.message = e,
                }
            }
        }
        self.civil_ui.parcels.split = Some(f);
    }
}

// ─── schedule ──────────────────────────────────────────────────────────────

pub struct SchedForm {
    m2: String,
    ha: String,
    acres: bool,
    only_affected: bool,
    message: String,
}

/// The acquisition rows of the parcels in the document: a parcel made by a split is part of its
/// source's row (take and remainder); any other parcel has a row with no take.
pub fn schedule_rows(parcels: &[(ElementId, Parcel, bool)]) -> Vec<AcquisitionRow> {
    let mut rows: Vec<AcquisitionRow> = Vec::new();
    for (_, p, hidden) in parcels {
        let parent = p.attributes.get("Split from").cloned();
        let a = p.area().unwrap_or(0.0);
        match parent {
            Some(name) => {
                let row = match rows.iter().position(|r| r.parcel_name == name) {
                    Some(i) => i,
                    None => {
                        let src = parcels.iter().find(|x| x.1.name == name).map(|x| &x.1);
                        rows.push(AcquisitionRow { parcel_number: src.map(|s| s.number).unwrap_or(0), parcel_name: name.clone(), land_reference: p.land_reference.clone(), owner: p.owner.clone(), status: src.map(|s| s.status.label()).unwrap_or_default(), total_area_m2: 0.0, take_area_m2: 0.0, remainder_area_m2: 0.0, total_ha: 0.0, take_ha: 0.0, remainder_ha: 0.0, affected: false });
                        rows.len() - 1
                    }
                };
                let r = &mut rows[row];
                if p.attributes.get("Piece").is_some_and(|k| k == "take") {
                    r.take_area_m2 += a;
                    r.affected = true;
                } else {
                    r.remainder_area_m2 += a;
                }
                r.total_area_m2 = r.take_area_m2 + r.remainder_area_m2;
            }
            None if !*hidden => rows.push(AcquisitionRow { parcel_number: p.number, parcel_name: p.name.clone(), land_reference: p.land_reference.clone(), owner: p.owner.clone(), status: p.status.label(), total_area_m2: a, take_area_m2: 0.0, remainder_area_m2: a, total_ha: 0.0, take_ha: 0.0, remainder_ha: 0.0, affected: false }),
            None => {}
        }
    }
    for r in &mut rows {
        r.total_ha = r.total_area_m2 / afrobim_cogo::HECTARE_M2;
        r.take_ha = r.take_area_m2 / afrobim_cogo::HECTARE_M2;
        r.remainder_ha = r.remainder_area_m2 / afrobim_cogo::HECTARE_M2;
    }
    rows
}

impl AfroBimApp {
    fn sched_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.parcels.sched.take() else { return };
        let all: Vec<(ElementId, Parcel, bool)> = self.parcels_all().into_iter().map(|(id, p)| (id, p, self.doc.find(id).is_some_and(civil::hidden))).collect();
        let rows = schedule_rows(&all);
        let (m2, ha) = (f.m2.trim().parse::<usize>().ok(), f.ha.trim().parse::<usize>().ok());
        let mut csv = false;
        let out = Dialog::new("Land acquisition schedule", crate::glyphs::by_name("ACQUISITION")).width(820.0).resizable(true).max_body_height(520.0).help("One row per parcel: its total area, the part to be taken for the road reserve and the remainder. Parcels made by PARCELSPLIT are grouped under the parcel they came from. Acres, when shown, are a derived column (the international acre).").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Decimals: area m2");
                    d::num_opt(ui, &mut f.m2, "").ok();
                    ui.label("hectares");
                    d::num_opt(ui, &mut f.ha, "").ok();
                    ui.checkbox(&mut f.acres, "Acres column");
                    ui.checkbox(&mut f.only_affected, "Only parcels with a take");
                });
                let shown: Vec<&AcquisitionRow> = rows.iter().filter(|r| !f.only_affected || r.affected).collect();
                let table: Vec<Vec<String>> = shown.iter().map(|r| vec![r.parcel_number.to_string(), r.parcel_name.clone(), r.land_reference.clone(), r.owner.clone(), r.status.clone(), fmt(r.total_area_m2, 2), fmt(r.take_area_m2, 2), fmt(r.remainder_area_m2, 2), fmt(r.take_ha, 4)]).collect();
                egui::ScrollArea::vertical().id_salt("sched-rows").max_height(380.0).show(ui, |ui| {
                    ui_kit::results_table(ui, "sched", &["No.", "Parcel", "Land reference", "Owner", "Status", "Total m2", "Take m2", "Remainder m2", "Take ha"], &[Col::Num, Col::Text, Col::Text, Col::Text, Col::Text, Col::Num, Col::Num, Col::Num, Col::Num], &table);
                });
                let (tt, tr): (f64, f64) = rows.iter().fold((0.0, 0.0), |a, r| (a.0 + r.take_area_m2, a.1 + r.remainder_area_m2));
                ui.label(RichText::new(format!("{} parcel row(s). Total take {tt:.2} m2 ({:.4} ha); total remainder {tr:.2} m2.", rows.len(), tt / afrobim_cogo::HECTARE_M2)).small());
                if !f.message.is_empty() {
                    ui.label(RichText::new(&f.message).small());
                }
            },
            |ui| Footer::new("Export CSV...").icon(crate::glyphs::by_name("ACQUISITION")).enabled(m2.is_some() && ha.is_some(), "Decimals must be whole numbers").cancel(Some("Close")).show(ui),
        );
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            csv = true;
        }
        if csv {
            if let (Some(a), Some(b)) = (m2, ha) {
                let text = schedule_csv(&rows, a, b, f.acres, f.only_affected);
                let parcels: Vec<Parcel> = all.iter().filter(|x| !x.2).map(|x| x.1.clone()).collect();
                let table = area_table_csv(&parcels, a, b, f.acres).unwrap_or_default();
                if let Some(path) = rfd::FileDialog::new().set_file_name("acquisition-schedule.csv").save_file() {
                    let body = format!("{text}\n{table}");
                    f.message = match std::fs::write(&path, body) {
                        Ok(()) => format!("Wrote {}.", path.display()),
                        Err(e) => format!("The file could not be written: {e}"),
                    };
                    self.status = f.message.clone();
                }
            }
        }
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        self.civil_ui.parcels.sched = Some(f);
    }
}

#[allow(dead_code)]
fn _unused(_: Bounds) {}

/// Splits every parcel the road reserve crosses: the band from `left` m to the left of the
/// alignment to `right` m to its right. Returns the number of parcels split and of new parcels
/// made. The new parcels say which parcel they came from; the originals are hidden, not deleted.
pub fn split_by_reserve(doc: &mut afrobim_kernel::Document, al: &afrobim_alignment::Alignment, left: f64, right: f64, snap: f64, arc: f64) -> Result<(usize, usize), String> {
    // Curves become chords of 1 m: the reserve edge is then within 1 mm of the true offset on a radius of 125 m or more (the engine's offset takes straight segments).
    let mut centre: Vec<Xy> = al.polyline(1.0).iter().map(|p| Xy::new(p.0, p.1)).collect();
    // The joins of elements repeat a point: a zero-length segment has no direction to offset.
    centre.dedup_by(|a, b| a.dist(*b) < 1e-6);
    // 4 is the mitre limit: a numerical guard on how far a sharp corner of the offset line may run out.
    let left_edge = afrobim_cogo::offset_polyline(&centre, left, 4.0).map_err(|e| format!("The reserve edges cannot be offset: {e}"))?;
    let right_edge = afrobim_cogo::offset_polyline(&centre, -right, 4.0).map_err(|e| format!("The reserve edges cannot be offset: {e}"))?;
    let o = SplitOptions { snap_tol_m: snap, arc_tol_m: arc };
    let parcels: Vec<(ElementId, Parcel)> = doc.elements().iter().filter(|e| e.category == PARCEL && !civil::hidden(e)).filter_map(|e| serde_json::from_value::<Parcel>(e.data.clone()).ok().map(|p| (e.id, p))).collect();
    let mut number = doc.elements().iter().filter_map(|e| serde_json::from_value::<Parcel>(e.data.clone()).ok().filter(|_| e.category == PARCEL)).map(|p| p.number).max().unwrap_or(0) + 1;
    let (mut made, mut hidden) = (0, 0);
    for (id, p) in parcels {
        // The band is right of the left edge and left of the right edge.
        let Ok(first) = split_parcel(&p, &left_edge, Side::Right, &o) else { continue };
        if first.take_pieces.is_empty() {
            continue;
        }
        let mut take: Vec<Vec<Xy>> = Vec::new();
        let mut rest: Vec<Vec<Xy>> = first.remainder_pieces.iter().map(|x| x.ring.clone()).collect();
        for piece in &first.take_pieces {
            let tmp = piece_parcel(&p, &piece.ring, p.name.clone(), 0, ParcelStatus::Existing, "");
            match tmp.and_then(|t| split_parcel(&t, &right_edge, Side::Left, &o).ok()) {
                Some(second) => {
                    take.extend(second.take_pieces.iter().map(|x| x.ring.clone()));
                    rest.extend(second.remainder_pieces.iter().map(|x| x.ring.clone()));
                }
                None => take.push(piece.ring.clone()),
            }
        }
        if take.is_empty() {
            continue;
        }
        let mut add = |doc: &mut afrobim_kernel::Document, ring: &Vec<Xy>, name: String, status: ParcelStatus, note: &str| {
            if let Some(np) = piece_parcel(&p, ring, name, number, status, note) {
                number += 1;
                if crate::civil_landxml::add_parcel(doc, &np, "Split by road reserve").is_some() {
                    made += 1;
                }
            }
        };
        for (i, ring) in take.iter().enumerate() {
            add(doc, ring, format!("{} take {}", p.name, i + 1), ParcelStatus::ToBeAcquired, "take");
        }
        for (i, ring) in rest.iter().enumerate() {
            add(doc, ring, format!("{} remainder {}", p.name, i + 1), p.status.clone(), "remainder");
        }
        if let Some(e) = doc.find_mut(id) {
            e.properties.insert(civil::PROP_HIDDEN.into(), "1".into());
        }
        hidden += 1;
    }
    Ok((hidden, made))
}
