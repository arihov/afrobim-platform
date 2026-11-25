// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The project coordinate system in the application: the "Coordinate system" tab of the project
//! settings (command `CRS`, alias `COORDSYS`), the status bar read-outs, and the `WHERE` command.
//!
//! The coordinate system is a statement about what the document's numbers mean. Setting or
//! changing it never moves an object: model space keeps the coordinates of the sources, in
//! millimetres, and the system says that 450 000 000 mm east is 450 000 m of UTM easting. Nothing
//! here reprojects anything (the kernel has no datum shift, and none is invented).

use afrobim_kernel::{clear_project_crs, crs_from_model_point, set_project_crs, CrsCheck, CrsFinding, CrsKind, CrsSource, Document, FindingKind, IncomingDataset, PlanExtent, ProjectCrs, Pt};
use egui::RichText;

use crate::app::AfroBimApp;
use crate::ui_kit::{self, Level};

/// What the editor in the tab is building.
#[derive(Clone, PartialEq)]
pub struct CrsEdit {
    /// 0 a system the kernel knows, 1 named only (a name, a code or pasted WKT), 2 local site grid,
    /// 3 a geographic system (latitude and longitude).
    mode: usize,
    query: String,
    /// EPSG code picked from the list.
    selected: Option<u32>,
    zone: u32,
    south: usize,
    name: String,
    epsg: String,
    wkt: String,
    h_datum: String,
    v_datum: String,
    /// The project system the fields were last filled from.
    seeded_from: Option<Option<ProjectCrs>>,
}

impl Default for CrsEdit {
    fn default() -> Self {
        CrsEdit { mode: 0, query: String::new(), selected: None, zone: 36, south: 1, name: String::new(), epsg: String::new(), wkt: String::new(), h_datum: String::new(), v_datum: String::new(), seeded_from: None }
    }
}

impl CrsEdit {
    fn seed(&mut self, current: &Option<ProjectCrs>) {
        if self.seeded_from.as_ref() == Some(current) {
            return;
        }
        *self = CrsEdit { seeded_from: Some(current.clone()), ..CrsEdit::default() };
        if let Some(c) = current {
            self.h_datum = c.horizontal_datum.clone();
            self.v_datum = c.vertical_datum.clone();
            match c.kind {
                CrsKind::Projected => {
                    self.selected = c.epsg;
                    if let Some((zone, south)) = c.utm_zone() {
                        self.zone = u32::from(zone);
                        self.south = usize::from(south);
                    }
                }
                CrsKind::NamedOnly => {
                    self.mode = 1;
                    self.name = c.name.clone();
                    self.epsg = c.epsg.map(|e| e.to_string()).unwrap_or_default();
                    self.wkt = c.wkt.clone().unwrap_or_default();
                }
                CrsKind::LocalGrid => {
                    self.mode = 2;
                    self.name = c.name.clone();
                }
                CrsKind::Geographic => {
                    self.mode = 3;
                    self.selected = c.epsg;
                }
            }
        }
    }

    /// The system the fields describe, or why they do not describe one yet.
    fn pending(&self) -> Result<ProjectCrs, String> {
        let mut crs = match self.mode {
            0 => {
                let code = self.selected.ok_or("Pick a system from the list.")?;
                ProjectCrs::from_epsg(code).ok_or_else(|| format!("EPSG:{code} is not a system the application can compute with."))?
            }
            1 => {
                let wkt = self.wkt.trim();
                if !wkt.is_empty() {
                    ProjectCrs::from_wkt(wkt).ok_or("The WKT text cannot be read: it must start with a node such as PROJCRS[\"name\", ...] or PROJCS[\"name\", ...].")?
                } else {
                    let name = self.name.trim();
                    let epsg = match self.epsg.trim() {
                        "" => None,
                        t => Some(t.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().map_err(|_| "The EPSG code must be a number, for example 2039.")?),
                    };
                    if name.is_empty() && epsg.is_none() {
                        return Err("Type a name or an EPSG code, or paste WKT.".into());
                    }
                    let shown = if name.is_empty() { format!("EPSG:{}", epsg.unwrap_or_default()) } else { name.to_string() };
                    // A code the kernel can compute with is a projected system, whatever it was typed as.
                    match epsg.and_then(ProjectCrs::from_epsg) {
                        Some(known) if name.is_empty() => known,
                        _ => ProjectCrs::named_only(epsg, &shown),
                    }
                }
            }
            3 => {
                let code = self.selected.ok_or("Pick a geographic system from the list.")?;
                ProjectCrs::geographic(code).ok_or_else(|| format!("EPSG:{code} is not a geographic system the application knows."))?
            }
            _ => {
                if self.name.trim().is_empty() {
                    return Err("Name the site grid, for example \"Site grid, plant datum\".".into());
                }
                ProjectCrs::local_grid(self.name.trim())
            }
        };
        if !self.h_datum.trim().is_empty() {
            crs.horizontal_datum = self.h_datum.trim().to_string();
        }
        crs.vertical_datum = self.v_datum.trim().to_string();
        crs.source = CrsSource::Typed;
        Ok(crs)
    }
}

/// What the tab asks the application to do.
#[derive(Default)]
pub struct CrsTabOut {
    pub set: Option<ProjectCrs>,
    pub clear: bool,
}

#[derive(Default)]
pub struct CrsUi {
    edit: CrsEdit,
}

/// One line each: what a kind of system lets the application do.
pub fn kind_text(kind: CrsKind) -> &'static str {
    match kind {
        CrsKind::Projected => "Projected, with maths: the application can place a coordinate on the Earth (latitude, longitude, grid convergence, point scale factor) and check an import against the area of use.",
        CrsKind::NamedOnly => "Named only: the system is recorded by name or code, but the application has no maths for it. Coordinates are shown as eastings and northings; no latitude, longitude, convergence or scale, and imports are checked only for the same code or name.",
        CrsKind::LocalGrid => "Local site grid: the numbers have no geodetic meaning. Nothing can be placed on the Earth and imports cannot be checked against it.",
        CrsKind::Geographic => "Geographic: latitude and longitude in degrees on a datum. The application takes it as the source or target of a datum transformation (TRANSFORM). A document whose model is drawn in metres should use a projected system as its project system.",
    }
}

/// `south, west, north, east` as sentences.
pub fn area_text(crs: &ProjectCrs) -> Option<String> {
    let a = crs.area_of_use()?;
    Some(format!("latitude {:.2} to {:.2}, longitude {:.2} to {:.2} degrees (EPSG area of use)", a.south, a.north, a.west, a.east))
}

/// The cursor's easting and northing in metres of the project CRS, as the status bar shows them.
pub fn en_text(p: Pt, crs: Option<&ProjectCrs>) -> String {
    let (e, n) = crs_from_model_point(p);
    let local = crs.is_some_and(|c| c.kind == CrsKind::LocalGrid);
    format!("{} {}  {} {}", if local { "X" } else { "E" }, thousands(e), if local { "Y" } else { "N" }, thousands(n))
}

/// A number of metres with three decimals and thin grouping, `9 960 123.457`.
pub fn thousands(v: f64) -> String {
    let s = format!("{:.3}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut grouped = String::new();
    for (i, ch) in int.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push('\u{2009}');
        }
        grouped.push(ch);
    }
    let int: String = grouped.chars().rev().collect();
    format!("{}{int}.{frac}", if v < 0.0 { "-" } else { "" })
}

/// Latitude and longitude of a model point, with the datum they are on. None when the project
/// system has no maths. The datum is that of the system (Arc 1960 for an Arc 1960 system): no
/// datum shift to WGS 84 exists here.
pub fn lat_lon_text(p: Pt, crs: &ProjectCrs) -> Option<String> {
    let (e, n) = crs_from_model_point(p);
    let (lat, lon) = crs.to_geographic(e, n)?;
    let datum = if crs.horizontal_datum.is_empty() { "datum of the system" } else { crs.horizontal_datum.as_str() };
    Some(format!("lat {:.6}  lon {:.6}  ({datum})", lat, lon))
}

thread_local! {
    /// The plan to WGS 84 for a project system near a place: (system code, 50 km cell) to plan.
    static WGS84_PLAN: std::cell::RefCell<Option<((Option<u32>, i64, i64), Option<afrobim_kernel::crs_plan::TransformPlan>)>> = const { std::cell::RefCell::new(None) };
}

/// The WGS 84 latitude and longitude of a model point as a second reading, labelled with the EPSG
/// transformation code and its stated accuracy. None when the project system is on WGS 84 already
/// (the first reading is that), has no maths, or the registry has no transformation to WGS 84.
/// The datum's own latitude and longitude stay the primary reading.
pub fn wgs84_text(p: Pt, crs: &ProjectCrs) -> Option<String> {
    if crs.kind != CrsKind::Projected {
        return None;
    }
    let (e, n) = crs_from_model_point(p);
    let key = (crs.epsg, (e / 50_000.0).floor() as i64, (n / 50_000.0).floor() as i64);
    let plan = WGS84_PLAN.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().map(|x| x.0) != Some(key) {
            let wgs = ProjectCrs::geographic(4326)?;
            let plan = afrobim_kernel::crs_plan::plan_transform(crs, &wgs, Some(afrobim_kernel::crs_plan::DataExtent::new((e, n), (e, n)))).ok().and_then(|v| v.into_iter().next());
            *c = Some((key, plan));
        }
        c.as_ref().and_then(|x| x.1.clone())
    })?;
    if plan.kind != afrobim_kernel::crs_plan::PlanKind::DatumTransformation {
        return None;
    }
    let r = plan.apply(e, n, 0.0).ok()?;
    let codes: Vec<String> = plan.transformation_codes().iter().map(|c| format!("EPSG:{c}")).collect();
    Some(format!("WGS 84  lat {:.6}  lon {:.6}  ({}, stated accuracy {} m)", r.first, r.second, codes.join(" + "), plan.accuracy_m.map(|a| a.to_string()).unwrap_or_else(|| "not stated".into())))
}

/// Grid convergence and point scale factor, as two lines, with the units stated.
pub fn convergence_text(p: Pt, crs: &ProjectCrs) -> Option<String> {
    let (e, n) = crs_from_model_point(p);
    let (k, gamma) = crs.scale_and_convergence_at(e, n)?;
    Some(format!("Point scale factor {k:.7}\nGrid convergence {gamma:.5} degrees (grid north is {} of true north)", if gamma >= 0.0 { "east" } else { "west" }))
}

/// The full account of a picked point, for `WHERE` and `ID`: model millimetres, CRS metres, and
/// latitude, longitude, convergence and scale where the system has maths.
pub fn where_lines(p: Pt, crs: Option<&ProjectCrs>) -> Vec<String> {
    let (e, n) = crs_from_model_point(p);
    let mut out = vec![format!("Model position {:.3} , {:.3} mm", p.x, p.y)];
    match crs {
        None => out.push("No coordinate system is set: the numbers above are the model's own. Set one with CRS.".into()),
        Some(c) => {
            let local = c.kind == CrsKind::LocalGrid;
            out.push(format!("{} {:.3} m   {} {:.3} m   in {}", if local { "X" } else { "Easting" }, e, if local { "Y" } else { "Northing" }, n, c.label()));
            if let Some(ll) = lat_lon_text(p, c) {
                out.push(ll);
            }
            if let Some(cv) = convergence_text(p, c) {
                out.extend(cv.lines().map(str::to_string));
            }
            if c.kind == CrsKind::Projected {
                match wgs84_text(p, c) {
                    Some(w) => {
                        out.push(w);
                        out.push("The first latitude and longitude are on the datum of the system. The WGS 84 line is computed through the registry transformation named, to its stated accuracy, which is not survey accuracy.".into());
                    }
                    None => out.push("Latitude and longitude are on the datum of the system. No transformation to WGS 84 is recorded for it, so no WGS 84 reading is offered.".into()),
                }
            }
        }
    }
    out
}

/// What the IFC file states for the project's system, or None for no system and for a local site
/// grid (a grid with no geodetic meaning is not a coordinate reference system to write).
pub fn georeference_of(crs: Option<&ProjectCrs>) -> Option<afrobim_ifc::IfcGeoreference> {
    let c = crs.filter(|c| c.kind != CrsKind::LocalGrid)?;
    let (map_projection, map_zone) = match (c.kind, c.utm_zone()) {
        (CrsKind::Projected, Some((zone, south))) => (Some("UTM".to_string()), Some(format!("{zone}{}", if south { 'S' } else { 'N' }))),
        _ => (None, None),
    };
    let opt = |t: &str| (!t.trim().is_empty()).then(|| t.trim().to_string());
    Some(afrobim_ifc::IfcGeoreference {
        name: c.code().unwrap_or_else(|| c.name.clone()),
        description: Some(match c.code() {
            Some(code) => format!("{code} - {}", c.name),
            None => c.name.clone(),
        }),
        geodetic_datum: opt(&c.horizontal_datum),
        vertical_datum: opt(&c.vertical_datum),
        map_projection,
        map_zone,
        origin: None,
    })
}

/// The sentence the export dialog shows about the coordinate reference.
pub fn export_statement(crs: Option<&ProjectCrs>) -> String {
    match crs {
        Some(c) if c.kind != CrsKind::LocalGrid => format!(
            "The project coordinate system, {}, is written as IfcProjectedCRS with an IfcMapConversion. Geometry is written near a local origin (the least easting and northing of what has a site position, rounded down to a whole kilometre, height 0), and the map conversion carries that origin, so viewers place the model correctly without huge coordinates.",
            c.label()
        ),
        Some(c) => format!("The project's system, {}, is a local site grid with no geodetic meaning: it is not written. Coordinates are written as they are, with no IfcMapConversion.", c.name),
        None => "The project has no coordinate system, so no IfcProjectedCRS or IfcMapConversion is written and coordinates are written as they are in the model. Set one with CRS to georeference the file.".to_string(),
    }
}

/// The coordinate system a LandXML file declares, as the application's type. `epsgCode` wins when
/// it is a code the kernel can compute with; else the WKT the file carries, else the name. A
/// `CoordinateSystem` element with nothing in it gives None.
pub fn from_landxml(cs: &afrobim_alignment::LandXmlCoordinateSystem) -> Option<ProjectCrs> {
    let name = [&cs.name, &cs.projected_coordinate_system_name, &cs.horizontal_coordinate_system_name, &cs.desc].into_iter().flatten().map(|s| s.trim()).find(|s| !s.is_empty()).map(str::to_string);
    let epsg = cs.epsg_code.as_deref().map(|t| t.trim().trim_start_matches(|c: char| !c.is_ascii_digit())).and_then(|t| t.parse::<u32>().ok());
    let wkt = cs.ogc_wkt_code.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let mut crs = if let Some(known) = epsg.and_then(ProjectCrs::from_epsg) {
        known
    } else if let Some(w) = wkt.and_then(ProjectCrs::from_wkt) {
        // The code the file states is the system's, even when the WKT's root has none.
        ProjectCrs { epsg: w.epsg.or(epsg), ..w }
    } else if let Some(code) = epsg {
        ProjectCrs::named_only(Some(code), name.as_deref().unwrap_or(&format!("EPSG:{code}")))
    } else if let Some(n) = &name {
        ProjectCrs::parse(n).unwrap_or_else(|| ProjectCrs::named_only(None, n))
    } else {
        return None;
    };
    if crs.horizontal_datum.is_empty() {
        crs.horizontal_datum = cs.horizontal_datum.clone().or_else(|| cs.datum.clone()).unwrap_or_default();
    }
    if let Some(v) = cs.vertical_datum.as_deref().filter(|v| !v.trim().is_empty()) {
        crs.vertical_datum = v.trim().to_string();
    }
    crs.source = CrsSource::LandXml;
    Some(crs)
}

/// What an incoming dataset says against the project system. With no project system the dataset's
/// own declared system is tested against its own extent (swapped columns, millimetres, feet, the
/// area of use), and the first line says the project has none. With neither, one line says that
/// nothing can be checked. Nothing here moves or converts anything.
pub fn dataset_check(project: Option<&ProjectCrs>, label: &str, declared: Option<&ProjectCrs>, extent: Option<PlanExtent>) -> Vec<CrsFinding> {
    let incoming = IncomingDataset { label: label.to_string(), crs: declared.cloned(), extent };
    match (project, declared) {
        (Some(p), _) => CrsCheck::run(p, &incoming).findings,
        (None, Some(d)) => {
            let mut v = vec![CrsFinding { kind: FindingKind::Undeclared, warning: false, message: format!("The project has no coordinate system yet. {label} declares {}.", d.label()) }];
            v.extend(CrsCheck::run(d, &incoming).findings.into_iter().filter(|f| f.kind != FindingKind::SameSystem));
            v
        }
        (None, None) => vec![CrsFinding {
            kind: FindingKind::Undeclared,
            warning: false,
            message: format!("Neither the project nor {label} has a coordinate system, so the position cannot be checked. The coordinates are taken as they are."),
        }],
    }
}

/// The findings as callouts: a warning for what needs a look, a note for the rest.
pub fn show_findings(ui: &mut egui::Ui, findings: &[CrsFinding]) {
    for f in findings {
        let level = if f.warning {
            Level::Warning
        } else if f.kind == FindingKind::SameSystem {
            Level::Success
        } else {
            Level::Info
        };
        ui_kit::callout(ui, level, f.message.clone());
        ui.add_space(2.0);
    }
}

/// The tab. `objects` is how many elements the document holds. `out.set` is the system to store.
pub fn tab(ui: &mut egui::Ui, state: &mut CrsUi, current: &Option<ProjectCrs>, objects: usize) -> CrsTabOut {
    let mut out = CrsTabOut::default();
    let t = crate::theme::tokens_ui(ui);
    state.edit.seed(current);
    let edit = &mut state.edit;

    ui_kit::section(ui, "Project coordinate system", |ui| match current {
        None => {
            ui_kit::callout(ui, Level::Info, "No coordinate system. The document's numbers are model coordinates only, and an import cannot be checked against anything. Set one below; this asks nothing else of you.");
        }
        Some(c) => {
            ui.label(RichText::new(c.label()).strong().size(crate::theme::FONT_BODY + 1.0));
            ui_kit::form(ui, "crs-current", |f| {
                f.row("Kind", |ui| {
                    ui.label(match c.kind {
                        CrsKind::Projected => "Projected, with maths",
                        CrsKind::NamedOnly => "Named only",
                        CrsKind::LocalGrid => "Local site grid",
                        CrsKind::Geographic => "Geographic (latitude, longitude)",
                    });
                });
                f.row("Horizontal datum", |ui| {
                    ui.label(if c.horizontal_datum.is_empty() { "not stated" } else { c.horizontal_datum.as_str() });
                });
                f.row("Vertical datum", |ui| {
                    ui.label(if c.vertical_datum.is_empty() { "not stated" } else { c.vertical_datum.as_str() });
                });
                f.row("Area of use", |ui| {
                    ui.label(area_text(c).unwrap_or_else(|| "not known to the application".into()));
                });
                f.row("Taken from", |ui| {
                    ui.label(c.source.label());
                });
                f.row("Unit", |ui| {
                    ui.label("metres");
                });
            });
            ui.add_space(4.0);
            ui.label(RichText::new(kind_text(c.kind)).size(crate::theme::FONT_SMALL).color(t.text_muted));
        }
    });

    ui_kit::section(ui, if current.is_some() { "Change it" } else { "Set it" }, |ui| {
        if objects > 0 {
            ui_kit::callout(
                ui,
                Level::Warning,
                format!("This document has {objects} element(s). Setting or changing the coordinate system never moves any of them. It only changes what their coordinates are taken to mean: nothing is reprojected, and nothing is shifted."),
            );
            ui.add_space(4.0);
        }
        let mut mode = edit.mode;
        ui_kit::segmented(ui, &mut mode, &["Known system", "Named only", "Local site grid", "Geographic"]);
        edit.mode = mode;
        ui.add_space(4.0);
        match edit.mode {
            0 => known_systems(ui, edit),
            1 => named_only(ui, edit),
            3 => {
                for c in ProjectCrs::geographic_systems() {
                    if ui.selectable_label(edit.selected == c.epsg, c.label()).clicked() {
                        edit.selected = c.epsg;
                    }
                }
                if let Some(c) = edit.selected.and_then(ProjectCrs::geographic) {
                    if let Some(g) = afrobim_kernel::crs_transform::geographic_crs(c.epsg.unwrap_or_default()) {
                        crate::ui_kit::note(ui, format!("Area of use: {} (latitude {:.2} to {:.2}, longitude {:.2} to {:.2} degrees)", g.area_text, g.area.south, g.area.north, g.area.west, g.area.east));
                    }
                }
            }
            _ => {
                ui_kit::form(ui, "crs-local", |f| {
                    f.row_note("Name", "a site grid has no geodetic meaning", |ui| {
                        ui_kit::text_field(ui, &mut edit.name, "Site grid, plant datum");
                    });
                });
            }
        }
        ui.add_space(4.0);
        ui_kit::form(ui, "crs-datums", |f| {
            f.row_note("Horizontal datum", "free text; blank takes the datum of a known system", |ui| {
                ui_kit::text_field(ui, &mut edit.h_datum, "for example WGS 84, Arc 1960");
            });
            f.row_note("Vertical datum", "free text, never interpreted: the levels are taken as they are given", |ui| {
                ui_kit::text_field(ui, &mut edit.v_datum, "for example the name of a levelling datum");
            });
        });
        ui.add_space(6.0);
        match edit.pending() {
            Err(why) => {
                ui.label(RichText::new(why).size(crate::theme::FONT_SMALL).color(t.text_muted));
            }
            Ok(crs) => {
                ui.label(RichText::new(format!("Will be: {}  ({})", crs.label(), match crs.kind {
                    CrsKind::Projected => "projected, with maths",
                    CrsKind::NamedOnly => "named only",
                    CrsKind::LocalGrid => "local site grid",
                    CrsKind::Geographic => "geographic",
                })).size(crate::theme::FONT_SMALL));
                let same = current.as_ref().is_some_and(|c| c.epsg == crs.epsg && c.name == crs.name && c.kind == crs.kind && c.horizontal_datum == crs.horizontal_datum && c.vertical_datum == crs.vertical_datum && c.wkt == crs.wkt);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!same, egui::Button::new(if current.is_some() { "Change coordinate system" } else { "Set coordinate system" })).clicked() {
                        out.set = Some(crs);
                    }
                    if current.is_some() && ui.button("Remove it").on_hover_text("The document goes back to having no coordinate system. Nothing moves.").clicked() {
                        out.clear = true;
                    }
                });
            }
        }
    });
    out
}

/// One-click systems for work in Uganda. The country straddles UTM zones 35 and 36 and the
/// equator: WGS 84 / UTM for new survey, Arc 1960 / UTM for legacy cadastral and road records.
pub const UGANDA_PRESETS: &[(u32, &str)] = &[
    (32636, "WGS 84 / UTM 36N"),
    (32736, "WGS 84 / UTM 36S"),
    (32635, "WGS 84 / UTM 35N"),
    (32735, "WGS 84 / UTM 35S"),
    (21096, "Arc 1960 / UTM 36N"),
    (21036, "Arc 1960 / UTM 36S"),
    (21095, "Arc 1960 / UTM 35N"),
    (21035, "Arc 1960 / UTM 35S"),
];

fn known_systems(ui: &mut egui::Ui, edit: &mut CrsEdit) {
    crate::ui_kit::note(ui, "Uganda");
    ui.horizontal_wrapped(|ui| {
        for (code, label) in UGANDA_PRESETS {
            if ProjectCrs::from_epsg(*code).is_some() && ui.selectable_label(edit.selected == Some(*code), *label).on_hover_text(format!("EPSG:{code}")).clicked() {
                edit.selected = Some(*code);
            }
        }
    });
    ui.add_space(4.0);
    ui_kit::form(ui, "crs-search", |f| {
        f.row_note("Search", "an EPSG code (21036) or part of a name (arc 1960, utm 36 s)", |ui| {
            ui_kit::text_field(ui, &mut edit.query, "EPSG code or name");
        });
    });
    ui.add_space(4.0);
    // WGS 84 / UTM by zone and hemisphere.
    ui.horizontal(|ui| {
        ui.label("WGS 84 / UTM zone");
        ui.add(egui::DragValue::new(&mut edit.zone).range(1..=60));
        let mut i = edit.south;
        ui_kit::segmented(ui, &mut i, &["North", "South"]);
        edit.south = i;
        let code = ProjectCrs::utm_wgs84(edit.zone as u8, edit.south == 1).and_then(|c| c.epsg);
        if ui.selectable_label(edit.selected == code && code.is_some(), "Use").clicked() {
            edit.selected = code;
        }
    });
    ui.add_space(2.0);
    crate::ui_kit::note(ui, "Arc 1960 / UTM");
    let arc: Vec<ProjectCrs> = ProjectCrs::search("arc 1960", 20);
    ui.horizontal_wrapped(|ui| {
        for c in &arc {
            if ui.selectable_label(edit.selected == c.epsg, c.name.trim_start_matches("Arc 1960 / UTM ")).clicked() {
                edit.selected = c.epsg;
            }
        }
    });
    if !edit.query.trim().is_empty() {
        ui.add_space(4.0);
        let found = ProjectCrs::search(&edit.query, 12);
        if found.is_empty() {
            ui_kit::callout(ui, Level::Info, "No system the application can compute with matches. For any other EPSG system, use Named only.");
        }
        for c in &found {
            if ui.selectable_label(edit.selected == c.epsg, c.label()).clicked() {
                edit.selected = c.epsg;
            }
        }
    }
    if let Some(c) = edit.selected.and_then(ProjectCrs::from_epsg) {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Selected: {}", c.label())).strong());
        if let Some(a) = area_text(&c) {
            crate::ui_kit::note(ui, format!("Area of use: {a}"));
        }
    }
}

fn named_only(ui: &mut egui::Ui, edit: &mut CrsEdit) {
    ui_kit::form(ui, "crs-named", |f| {
        f.row("Name", |ui| {
            ui_kit::text_field(ui, &mut edit.name, "for example Israeli Grid");
        });
        f.row_note("EPSG code", "optional", |ui| {
            ui_kit::text_field(ui, &mut edit.epsg, "for example 2039");
        });
        f.row_note("Or paste WKT", "the text is kept as it is; its name and code are read from it", |ui| {
            ui.add(egui::TextEdit::multiline(&mut edit.wkt).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
        });
    });
}

impl AfroBimApp {
    /// CRS / COORDSYS: opens the project settings on the coordinate system tab.
    pub(crate) fn open_crs_tab(&mut self) {
        self.projset.open = true;
        self.projset.tab = crate::projset::CRS_TAB;
    }

    /// Stores a system chosen in the tab. Nothing in the document moves.
    pub(crate) fn apply_project_crs(&mut self, crs: ProjectCrs) {
        let objects = object_count(&self.doc);
        let label = crs.label();
        set_project_crs(&mut self.doc, &crs);
        self.status = if objects > 0 { format!("Coordinate system set to {label}. {objects} element(s) were not moved: their coordinates are now taken to be in it.") } else { format!("Coordinate system set to {label}.") };
        self.log(format!("CRS: {}", self.status));
        self.schedule_commit_now();
    }

    pub(crate) fn remove_project_crs(&mut self) {
        clear_project_crs(&mut self.doc);
        self.status = "The coordinate system was removed. Nothing was moved.".into();
        self.schedule_commit_now();
    }

}

/// The objects of a document that carry coordinates (everything but the settings element).
pub fn object_count(doc: &Document) -> usize {
    doc.elements().iter().filter(|e| e.category != afrobim_annotate::SETTINGS_CATEGORY && e.category != afrobim_kernel::PROJECT_SETTINGS_CATEGORY).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_uganda_preset_is_a_system_the_application_computes_with_and_reaches_wgs84() {
        use afrobim_kernel::crs_plan::plan_transform;
        let wgs = ProjectCrs::geographic(4326).unwrap();
        for (code, label) in UGANDA_PRESETS {
            let c = ProjectCrs::from_epsg(*code).unwrap_or_else(|| panic!("EPSG:{code} ({label}) is not known to the kernel"));
            assert_eq!(c.kind, CrsKind::Projected, "{label}");
            let (zone, south) = c.utm_zone().unwrap();
            assert_eq!(format!("{zone}{}", if south { "S" } else { "N" }), label.rsplit(' ').next().unwrap(), "{label}");
            assert_eq!(c.horizontal_datum.contains("Arc 1960"), label.starts_with("Arc"), "{label}: {}", c.horizontal_datum);
            assert!(plan_transform(&c, &wgs, None).is_ok_and(|p| !p.is_empty()), "{label} has no plan to WGS 84");
        }
    }

    #[test]
    fn the_ifc_georeference_follows_the_project_system() {
        assert!(georeference_of(None).is_none());
        assert!(georeference_of(Some(&ProjectCrs::local_grid("Site"))).is_none());
        let g = georeference_of(ProjectCrs::from_epsg(21036).as_ref()).unwrap();
        assert_eq!((g.name.as_str(), g.map_zone.as_deref(), g.geodetic_datum.as_deref()), ("EPSG:21036", Some("36S"), Some("Arc 1960")));
        let n = georeference_of(Some(&ProjectCrs::named_only(Some(2039), "Israeli Grid"))).unwrap();
        assert!(n.map_projection.is_none() && n.name == "EPSG:2039");
        assert!(export_statement(None).contains("no coordinate system"));
    }

    #[test]
    fn eastings_and_northings_read_in_metres_at_true_magnitude() {
        let p = afrobim_kernel::model_point_from_crs(450_123.456, 9_960_123.457);
        assert_eq!(p.x, 450_123_456.0);
        assert_eq!(en_text(p, None), "E 450\u{2009}123.456  N 9\u{2009}960\u{2009}123.457");
        assert!(en_text(p, Some(&ProjectCrs::local_grid("Site"))).starts_with("X "));
        assert_eq!(thousands(-1234.5), "-1\u{2009}234.500");
    }

    #[test]
    fn latitude_and_longitude_carry_the_datum_of_the_system() {
        let arc = ProjectCrs::from_epsg(21036).unwrap();
        let p = afrobim_kernel::model_point_from_crs(500_000.0, 10_000_000.0);
        let ll = lat_lon_text(p, &arc).unwrap();
        assert!(ll.contains("Arc 1960") && ll.contains("lon 33.000000"), "{ll}");
        assert!(lat_lon_text(p, &ProjectCrs::named_only(Some(2039), "Israeli Grid")).is_none());
        let lines = where_lines(p, Some(&arc));
        assert!(lines.iter().any(|l| l.contains("WGS 84") && l.contains("stated accuracy")), "{lines:?}");
        assert!(where_lines(p, None).iter().any(|l| l.contains("No coordinate system")));
    }

    #[test]
    fn the_editor_builds_each_kind_and_says_what_is_missing() {
        let mut e = CrsEdit::default();
        assert!(e.pending().is_err());
        e.selected = Some(32736);
        assert_eq!(e.pending().unwrap().kind, CrsKind::Projected);
        e.v_datum = "Mean sea level".into();
        assert_eq!(e.pending().unwrap().vertical_datum, "Mean sea level");
        e.mode = 1;
        assert!(e.pending().is_err());
        e.name = "Israeli Grid".into();
        e.epsg = "2039".into();
        let n = e.pending().unwrap();
        assert_eq!((n.kind, n.epsg), (CrsKind::NamedOnly, Some(2039)));
        e.wkt = "LOCAL_CS[\"Plant grid\",LOCAL_DATUM[\"Plant\",0]]".into();
        assert_eq!(e.pending().unwrap().kind, CrsKind::LocalGrid);
        e.wkt.clear();
        e.mode = 2;
        e.name.clear();
        assert!(e.pending().is_err());
        e.name = "Site grid".into();
        assert_eq!(e.pending().unwrap().kind, CrsKind::LocalGrid);
    }
}
