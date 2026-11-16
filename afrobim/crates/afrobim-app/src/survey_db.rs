//! The open survey database of the document: where it lives (a file beside the project, its path
//! stored in the project settings), loading and saving, the import with strict or lenient reading,
//! the COGO point set with its link back to the database, update when the database changes, figures
//! drawn in plan, and the synthetic database of the civil demonstration.
//!
//! The engine is `afrobim-survey`. Nothing here supplies an instrument standard error, a
//! tolerance, a refraction coefficient or a confidence level: those are inputs of the dialogs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use afrobim_kernel::{project_setting, set_project_setting, Document, ElementId};
use afrobim_survey::*;
use std::result::Result;
use serde::{Deserialize, Serialize};

use crate::civil::{self, Bounds, PointsData};

pub const SETTING_KEY: &str = "survey_database";
pub const LINK_PROP: &str = "survey.link";
pub const SOURCE: &str = "Survey database";
pub const DEMO_LABEL: &str = "SYNTHETIC DEMONSTRATION DATA: example geometry and example equipment values, not a survey and not an instrument's specification";

/// What the document stores about its survey database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbRef {
    /// Relative to the project file's folder when inside it, else absolute; forward slashes.
    pub path: String,
    pub name: String,
}

pub fn db_ref(doc: &Document) -> Option<DbRef> {
    project_setting(doc, SETTING_KEY).and_then(|v| serde_json::from_value(v).ok())
}

pub fn set_db_ref(doc: &mut Document, r: &DbRef) {
    set_project_setting(doc, SETTING_KEY, serde_json::to_value(r).unwrap_or_default());
}

/// The path to store: relative to the project's folder when the file is inside it.
pub fn stored_path(project_file: Option<&Path>, db_file: &Path) -> String {
    let rel = project_file.and_then(Path::parent).and_then(|dir| db_file.strip_prefix(dir).ok());
    rel.unwrap_or(db_file).to_string_lossy().replace('\\', "/")
}

/// The file a stored path names.
pub fn resolve_path(project_file: Option<&Path>, stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    project_file.and_then(Path::parent).map(|d| d.join(p)).unwrap_or_else(|| p.to_path_buf())
}

pub fn load_file(path: &Path) -> Result<SurveyDatabase, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    SurveyDatabase::from_json(&text).map_err(|e| e.to_string())
}

pub fn save_file(db: &SurveyDatabase, path: &Path) -> Result<(), String> {
    let text = db.to_json().map_err(|e| e.to_string())?;
    // Through a temporary file and a rename, so a failure leaves the old file whole.
    let tmp = path.with_extension("tmp-save");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

// ─── import ─────────────────────────────────────────────────────────────

/// A reading of a text with the errors it had. `strict`: the first error stops it. Lenient: the
/// line named by each error is left out and the reading goes on, so every bad line is listed.
pub struct Reading {
    pub data: Option<ParsedData>,
    /// Errors, each with its line number in the message.
    pub errors: Vec<String>,
    /// The lines left out (lenient).
    pub dropped: Vec<usize>,
}

pub fn read_text(format: &ImportFormat, text: &str, strict: bool) -> Reading {
    let mut work = text.to_string();
    let mut errors = Vec::new();
    let mut dropped: Vec<usize> = Vec::new();
    for _ in 0..if strict { 1 } else { 400 } {
        match afrobim_survey::import::parse_with(format, &work) {
            Ok(d) => return Reading { data: Some(d), errors, dropped },
            Err(e) => {
                errors.push(e.to_string());
                match e {
                    SurveyError::Parse { line, .. } if !strict && line >= 1 && !dropped.contains(&line) => {
                        dropped.push(line);
                        work = work.lines().enumerate().map(|(i, l)| if i + 1 == line { "" } else { l }).collect::<Vec<_>>().join("\n");
                    }
                    _ => break,
                }
            }
        }
    }
    Reading { data: None, errors, dropped }
}

/// What an import of `data` would do, in words, for the preview.
pub fn preview_lines(d: &ParsedData) -> Vec<String> {
    let obs: usize = d.setups.iter().map(|s| s.observations.len()).sum();
    let mut v = vec![format!("{} point(s) ({} control), {} setup(s) with {} observation(s), {} level run(s), {} baseline(s), {} known direction(s), {} figure(s).", d.points.len(), d.points.iter().filter(|p| p.control).count(), d.setups.len(), obs, d.level_runs.len(), d.baselines.len(), d.directions.len(), d.figures.len())];
    v.extend(d.warnings.iter().map(|w| format!("Warning: {w}")));
    v
}

// ─── linked point sets ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub db_path: String,
    pub db_name: String,
    pub network: String,
    /// A hash of the network's point coordinates when the set was made or last updated.
    pub hash: String,
}

pub fn link_of(doc: &Document, id: ElementId) -> Option<Link> {
    doc.find(id)?.properties.get(LINK_PROP).and_then(|t| serde_json::from_str(t).ok())
}

/// A hash of the coordinates of a network's points (name, E, N, Z), to tell that the database changed.
pub fn network_hash(net: &Network) -> String {
    let mut s = String::new();
    for p in net.points.iter().filter(|p| p.e.is_some() && p.n.is_some()) {
        s.push_str(&format!("{}|{:?}|{:?}|{:?}\n", p.name, p.e, p.n, p.z));
    }
    afrobim_survey::hash::sha256_hex(s.as_bytes())
}

/// Adds the points of a network to the document as a COGO point set with a link back to the database.
pub fn insert_points(doc: &mut Document, db: &SurveyDatabase, db_path: &str, network: &str, first_free: u32) -> Result<ElementId, String> {
    let net = db.network(network).map_err(|e| e.to_string())?;
    let mut data = PointsData::default();
    into_store(net, &mut data.store, first_free).map_err(|e| e.to_string())?;
    if data.store.is_empty() {
        return Err(format!("Network {network} has no point with coordinates."));
    }
    let b = Bounds::of(data.store.iter().map(|p| (p.easting, p.northing)));
    let name = format!("Survey: {network}");
    let id = civil::add(doc, civil::POINTS, &name, serde_json::to_value(&data).unwrap_or_default(), b, SOURCE);
    let link = Link { db_path: db_path.to_string(), db_name: db.name.clone(), network: network.to_string(), hash: network_hash(net) };
    if let Some(e) = doc.find_mut(id) {
        e.properties.insert(LINK_PROP.into(), serde_json::to_string(&link).unwrap_or_default());
    }
    Ok(id)
}

/// What an update found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdateSummary {
    pub moved: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub unchanged: usize,
}

impl UpdateSummary {
    pub fn changed(&self) -> bool {
        !(self.moved.is_empty() && self.added.is_empty() && self.removed.is_empty())
    }
}

/// Compares (and with `apply` brings up to date) a linked point set with its network. Points are
/// matched by survey name. New points take the next free number; points gone from the database
/// are removed from the set.
pub fn update_points(doc: &mut Document, id: ElementId, db: &SurveyDatabase, apply: bool) -> Result<UpdateSummary, String> {
    let link = link_of(doc, id).ok_or("This point set has no link to a survey database.")?;
    let net = db.network(&link.network).map_err(|e| e.to_string())?;
    let mut data: PointsData = civil::get(doc, id, civil::POINTS).ok_or("The point set cannot be read.")?;
    let mut sum = UpdateSummary::default();
    let by_name: BTreeMap<String, u32> = data.store.iter().filter_map(|p| p.name.clone().map(|n| (n, p.number))).collect();
    for p in net.points.iter().filter(|p| p.e.is_some() && p.n.is_some()) {
        let (e, n) = (p.e.unwrap_or_default(), p.n.unwrap_or_default());
        match by_name.get(&p.name) {
            Some(&num) => {
                let cur = data.store.get(num).cloned();
                match cur {
                    Some(c) if c.easting.to_bits() == e.to_bits() && c.northing.to_bits() == n.to_bits() && c.elevation.map(f64::to_bits) == p.z.map(f64::to_bits) => sum.unchanged += 1,
                    Some(_) => {
                        sum.moved.push(p.name.clone());
                        if apply {
                            data.store.move_point(num, e, n, p.z, "survey database update").map_err(|x| x.to_string())?;
                        }
                    }
                    None => {}
                }
            }
            None => {
                sum.added.push(p.name.clone());
                if apply {
                    let mut c = afrobim_cogo::CogoPoint::new(0, e, n, p.z, &p.description);
                    c.name = Some(p.name.clone());
                    c.source.file = Some("afrobim-survey".into());
                    data.store.add_next_free(c).map_err(|x| x.to_string())?;
                }
            }
        }
    }
    for (name, num) in &by_name {
        if net.point(name).is_none_or(|p| p.e.is_none() || p.n.is_none()) {
            sum.removed.push(name.clone());
            if apply {
                let _ = data.store.remove(*num, "no longer in the survey database");
            }
        }
    }
    if apply && sum.changed() {
        let b = Bounds::of(data.store.iter().map(|p| (p.easting, p.northing)));
        civil::set(doc, id, serde_json::to_value(&data).unwrap_or_default(), b);
    }
    if apply {
        let mut l = link;
        l.hash = network_hash(net);
        if let Some(e) = doc.find_mut(id) {
            e.properties.insert(LINK_PROP.into(), serde_json::to_string(&l).unwrap_or_default());
        }
    }
    Ok(sum)
}

/// Linked point sets of the document and whether their network changed: element, name, link, changed.
pub fn linked_sets(doc: &Document, db: Option<&SurveyDatabase>) -> Vec<(ElementId, String, Link, Option<bool>)> {
    doc.elements()
        .iter()
        .filter(|e| e.category == civil::POINTS)
        .filter_map(|e| {
            let l = link_of(doc, e.id)?;
            let changed = db.and_then(|d| d.network(&l.network).ok()).map(|n| network_hash(n) != l.hash);
            Some((e.id, e.name.clone(), l, changed))
        })
        .collect()
}

// ─── figures in plan ────────────────────────────────────────────────────

fn de_vertices<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<[f64; 3]>, D::Error> {
    let v: Vec<[Option<f64>; 3]> = Vec::deserialize(d)?;
    Ok(v.into_iter().map(|p| [p[0].unwrap_or(f64::NAN), p[1].unwrap_or(f64::NAN), p[2].unwrap_or(f64::NAN)]).collect())
}

/// A figure as the document keeps it: vertices as coordinates, so it draws without the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigureRec {
    pub name: String,
    pub layer: String,
    pub style: String,
    pub closed: bool,
    pub breakline: bool,
    /// Easting, northing, level (NaN is stored as null when the point has none).
    #[serde(deserialize_with = "de_vertices")]
    pub vertices: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FigureData {
    pub network: String,
    pub db_name: String,
    pub figures: Vec<FigureRec>,
    pub notes: Vec<String>,
}

/// The figures of a network flattened to coordinates (arcs to chords within `chord_tolerance`, an input).
pub fn figure_records(db: &SurveyDatabase, network: &str, chord_tolerance: f64) -> Result<FigureData, String> {
    let net = db.network(network).map_err(|e| e.to_string())?;
    let lookup = |name: &str| net.point(name).and_then(|p| Some((p.e?, p.n?, p.z)));
    let mut out = FigureData { network: network.into(), db_name: db.name.clone(), ..Default::default() };
    for f in &net.figures {
        match flatten_figure(f, &lookup, chord_tolerance, false) {
            Ok(b) => out.figures.push(FigureRec { name: f.name.clone(), layer: b.layer, style: b.style, closed: b.closed, breakline: f.breakline, vertices: b.points }),
            Err(e) => out.notes.push(format!("Figure {} not drawn: {e}", f.name)),
        }
    }
    Ok(out)
}

pub fn add_figures(doc: &mut Document, data: &FigureData) -> ElementId {
    let b = Bounds::of(data.figures.iter().flat_map(|f| f.vertices.iter().map(|v| (v[0], v[1]))));
    // An element of the same network is replaced rather than added again.
    let existing = doc.elements().iter().find(|e| e.category == civil::FIGURES && e.data.get("network").and_then(|n| n.as_str()) == Some(&data.network)).map(|e| e.id);
    let value = serde_json::to_value(data).unwrap_or_default();
    match existing {
        Some(id) => {
            civil::set(doc, id, value, b);
            id
        }
        None => civil::add(doc, civil::FIGURES, &format!("Figures: {}", data.network), value, b, SOURCE),
    }
}

/// The breaklines of figures flagged as breaklines, as the surface definition takes them.
pub fn breaklines_of(data: &FigureData) -> Vec<afrobim_terrain::Breakline> {
    data.figures
        .iter()
        .filter(|f| f.breakline && f.vertices.len() >= 2 && f.vertices.iter().all(|v| v[2].is_finite()))
        .map(|f| afrobim_terrain::Breakline { name: f.name.clone(), kind: afrobim_terrain::BreaklineKind::Standard, vertices: f.vertices.clone() })
        .collect()
}

// ─── adjustment, run as a job ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AdjMode {
    Plan2d,
    Levelling,
}

#[derive(Debug, Clone)]
pub struct AdjRequest {
    pub db: SurveyDatabase,
    pub network: String,
    pub mode: AdjMode,
    pub options: AdjustOptions,
    /// Baarda data snooping: the most observations removed, or None for a single run.
    pub snoop_max: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum AdjOut {
    Plan { adjustment: Adjustment, removed: Vec<(String, f64)>, snoop_note: String, report: String },
    Level { adjustment: LevelAdjustment, report: String },
}

pub fn run_adjustment(req: &AdjRequest) -> Result<AdjOut, String> {
    match req.mode {
        AdjMode::Plan2d => match req.snoop_max {
            Some(n) => {
                let s = adjust_with_snooping(&req.db, &req.network, &req.options, n).map_err(|e| e.to_string())?;
                let report = adjustment_report(&s.adjustment);
                Ok(AdjOut::Plan { adjustment: s.adjustment, removed: s.removed, snoop_note: s.stopped_reason, report })
            }
            None => {
                let a = adjust_network_2d(&req.db, &req.network, &req.options).map_err(|e| e.to_string())?;
                let report = adjustment_report(&a);
                Ok(AdjOut::Plan { adjustment: a, removed: Vec::new(), snoop_note: String::new(), report })
            }
        },
        AdjMode::Levelling => {
            let a = adjust_levelling(&req.db, &req.network, &req.options).map_err(|e| e.to_string())?;
            let report = level_report(&a);
            Ok(AdjOut::Level { adjustment: a, report })
        }
    }
}

/// Writes adjusted coordinates into the network (points that are not fixed). Returns how many.
pub fn apply_adjusted(db: &mut SurveyDatabase, network: &str, out: &AdjOut, time: &str) -> Result<usize, String> {
    let mut n = 0;
    match out {
        AdjOut::Plan { adjustment, .. } => {
            let net = db.network_mut(network).map_err(|e| e.to_string())?;
            for p in &adjustment.points {
                if p.fixed {
                    continue;
                }
                if let Some(sp) = net.point_mut(&p.name) {
                    sp.e = Some(p.e);
                    sp.n = Some(p.n);
                    sp.computed = true;
                    n += 1;
                }
            }
        }
        AdjOut::Level { adjustment, .. } => {
            let net = db.network_mut(network).map_err(|e| e.to_string())?;
            for h in &adjustment.heights {
                if h.fixed {
                    continue;
                }
                if let Some(sp) = net.point_mut(&h.name) {
                    sp.z = Some(h.z);
                    sp.computed = true;
                    n += 1;
                }
            }
        }
    }
    db.log(time, "apply adjusted coordinates", &format!("{network}: {n} point(s)"));
    Ok(n)
}

// ─── the demonstration database ─────────────────────────────────────────

/// A small synthetic network at the demonstration's coordinates: two control points, four
/// traverse stations, readings made from the true geometry with a deterministic wobble of a few
/// millimetres and arc seconds, and a figure shot by radiation. Labelled synthetic.
pub fn demo_database(e0: f64, n0: f64) -> SurveyDatabase {
    let mut db = SurveyDatabase::new("Demo survey database (synthetic)");
    let mut eq = Equipment::exact("EXAMPLE-TS");
    // Example values: not an instrument's specification (the adjustment dialog says so too).
    eq.sigma_distance_const_m = 0.003;
    eq.sigma_distance_ppm = 2.0;
    eq.sigma_pointing_arcsec = 2.0;
    eq.sigma_reading_arcsec = 2.0;
    eq.sigma_zenith_arcsec = 3.0;
    db.equipment.push(eq);
    db.figure_prefixes.push(FigurePrefix { prefix: "EP".into(), style: "Edge".into(), layer: "V-EDGE".into(), breakline: true });
    let _ = db.add_network("Main", "synthetic");
    let pts = [("A", 0.0, 0.0), ("B", 200.0, 0.0), ("P1", 40.0, 80.0), ("P2", 100.0, 120.0), ("P3", 160.0, 80.0), ("P4", 100.0, 40.0)];
    let at = |n: &str| pts.iter().find(|p| p.0 == n).map(|p| (e0 + p.1, n0 + p.2)).unwrap_or((e0, n0));
    let az = |a: (f64, f64), b: (f64, f64)| (b.0 - a.0).atan2(b.1 - a.1).to_degrees().rem_euclid(360.0);
    let wob = |k: f64| (k * 12.9898).sin() * 0.5 + (k * 4.1414).cos() * 0.5;
    let mut t = String::from("AFROFB 1\nUNITS DIST=m ANGLE=deg VERT=zenith\n");
    t.push_str(&format!("CONTROL A {} {} 1150.000 FIX=ENZ DESC=control (synthetic)\nCONTROL B {} {} 1152.500 FIX=ENZ DESC=control (synthetic)\n", at("A").0, at("A").1, at("B").0, at("B").1));
    // station, backsight, instrument orientation (degrees, arbitrary), targets
    let setups: [(&str, &str, f64, &[&str]); 6] = [("A", "B", 17.0, &["P1", "P4"]), ("P1", "A", 143.0, &["P2", "P4"]), ("P2", "P1", 61.0, &["P3", "P4"]), ("P3", "P2", 252.0, &["P4", "B"]), ("P4", "P3", 99.0, &["A", "P1"]), ("B", "A", 305.0, &["P3"])];
    let mut k = 0.0;
    for (st, bs, orient, targets) in setups {
        let (s, b) = (at(st), at(bs));
        let reading = |a: f64| (a - orient).rem_euclid(360.0);
        t.push_str(&format!("SETUP {st} 1.5 BS={bs} BSREAD={} EQUIP=EXAMPLE-TS\n", reading(az(s, b))));
        for tg in targets {
            k += 1.0;
            let p = at(tg);
            let d = (p.0 - s.0).hypot(p.1 - s.1);
            let hz = reading(az(s, p)) + wob(k) * 4.0 / 3600.0;
            let sd = d + wob(k + 0.5) * 0.004;
            t.push_str(&format!("OBS {tg} HZ={hz:.6} HZKIND=dir ZEN=90 SD={sd:.4} TH=1.5\n"));
        }
    }
    // Detail shots from P2, with linework codes: figure EP begins, continues, ends.
    let (s, b) = (at("P2"), at("P1"));
    let orient = 61.0;
    t.push_str(&format!("SETUP P2 1.5 BS=P1 BSREAD={} EQUIP=EXAMPLE-TS\n", (az(s, b) - orient).rem_euclid(360.0)));
    let shots = [("101", -40.0, -20.0, "EP B"), ("102", -10.0, -45.0, "EP C"), ("103", 35.0, -48.0, "EP C"), ("104", 70.0, -25.0, "EP E")];
    for (name, de, dn, code) in shots {
        let p = (s.0 + de, s.1 + dn);
        let d = (p.0 - s.0).hypot(p.1 - s.1);
        let hz = (az(s, p) - orient).rem_euclid(360.0);
        t.push_str(&format!("OBS {name} HZ={hz:.6} HZKIND=dir ZEN=90 SD={d:.4} TH=1.5 DESC={code}\n"));
    }
    let r = db.import_text("Main", ImportFormat::Fieldbook, &t, "demo.afrofb", "synthetic", DEMO_LABEL, true, DuplicatePolicy::Error);
    debug_assert!(r.is_ok(), "{r:?}");
    let _ = compute_points(&mut db, "Main");
    let _ = db.generate_figures("Main", "synthetic");
    db
}

/// The scene of a figures element: each figure as a polyline (closed ones back to the start) with
/// its name at the first vertex. Layer and style are shown in the Properties panel.
pub fn build_figures_scene(e: &afrobim_kernel::Element) -> Result<crate::civil_build::Built, String> {
    use crate::civil_scene::{SLine, SText};
    let d: FigureData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The figure data cannot be read: {x}"))?;
    let mut scene = crate::civil_scene::Scene::default();
    for f in &d.figures {
        let mut pts: Vec<[f64; 2]> = f.vertices.iter().map(|v| crate::civil_build::mm(v[0], v[1])).collect();
        if f.closed {
            if let Some(first) = pts.first().copied() {
                pts.push(first);
            }
        }
        if pts.len() >= 2 {
            let rgb = if f.breakline { [0xE0, 0x9A, 0x2E] } else { [0x4F, 0xA0, 0xD0] };
            scene.lines.push(SLine::new(pts.clone(), rgb, 1.8));
        }
        if let Some(p) = pts.first() {
            scene.texts.push(SText { at: *p, text: f.name.clone(), rgb: crate::civil_build::C_TEXT, height_mm: 0.0, px: 11.0, anchor: 3, prio: 2, halo: true });
        }
    }
    scene.derived.push(("Figures".into(), d.figures.len().to_string()));
    scene.derived.push(("Breaklines".into(), d.figures.iter().filter(|f| f.breakline).count().to_string()));
    scene.derived.push(("Network".into(), d.network.clone()));
    scene.notes.extend(d.notes.iter().cloned());
    Ok(crate::civil_build::Built { scene, ..Default::default() })
}
