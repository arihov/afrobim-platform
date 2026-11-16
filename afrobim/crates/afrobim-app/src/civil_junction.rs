//! Intersections and roundabouts as civil objects.
//!
//! An intersection element holds the inputs of the `afrobim-junction` engine (the two alignments are
//! named, never held) and what its last build made: the edge, kerb return, kerb line, island and
//! locked-secondary alignments, each a real alignment element with its profile, and a corridor
//! element for the regions the engine describes. The element draws the crossing, the kerb return low
//! points and the findings; the alignments draw themselves. When a source alignment (or an assembly of
//! the corridor library) changes, the engine's dependency record says so and the objects are rebuilt.
//! A roundabout is the same for `build_roundabout`. Everything is at source coordinates.

use afrobim_alignment::Alignment;
use afrobim_junction::{build_intersection, build_roundabout, DependencyRecord, IntersectionInputs, IntersectionResult, RoundaboutInputs, RoundaboutResult};
use afrobim_kernel::{Document, Element, ElementId};
use serde::{Deserialize, Serialize};

use crate::civil::{self, Bounds, CorridorData, CORRIDOR, INTERSECTION, ROUNDABOUT};
use crate::civil_build::{mm, Built, Ctx};
use crate::civil_scene::{SLine, SText, Scene};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntersectionData {
    pub inputs: IntersectionInputs,
    /// The alignment elements the last build made (ids), and the corridor element if any.
    #[serde(default)]
    pub created: Vec<u64>,
    #[serde(default)]
    pub corridor: Option<u64>,
    /// The ground surface (element id) the corridor is cut against.
    #[serde(default)]
    pub ground: Option<u64>,
    #[serde(default)]
    pub dependencies: Option<DependencyRecord>,
    /// The crossing point and the kerb return low points as of the last build: x, y, level (NaN when none).
    #[serde(default)]
    pub crossing: [f64; 2],
    #[serde(default)]
    pub low_points: Vec<[f64; 3]>,
    /// The findings of the last build: code, severity, message.
    #[serde(default)]
    pub findings: Vec<(String, String, String)>,
    #[serde(default)]
    pub example: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmReport {
    pub name: String,
    pub entry_angle_deg: f64,
    pub entry_path_radius_m: Option<f64>,
    pub entry_width_m: f64,
    pub exit_width_m: f64,
    pub entry_radius_m: f64,
    pub exit_radius_m: f64,
    pub flare_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundaboutData {
    pub inputs: RoundaboutInputs,
    #[serde(default)]
    pub created: Vec<u64>,
    #[serde(default)]
    pub dependencies: Option<DependencyRecord>,
    #[serde(default)]
    pub arms: Vec<ArmReport>,
    #[serde(default)]
    pub findings: Vec<(String, String, String)>,
    #[serde(default)]
    pub example: bool,
}

/// Every alignment of the document.
pub fn all_alignments(doc: &Document) -> Vec<Alignment> {
    doc.elements().iter().filter(|e| e.category == afrobim_alignment::CATEGORY).filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok()).collect()
}

pub fn assemblies_of(doc: &Document) -> Vec<afrobim_corridor::Assembly> {
    doc.elements().iter().filter(|e| e.category == civil::ASSEMBLY).filter_map(|e| civil::get::<afrobim_corridor::Assembly>(doc, e.id, civil::ASSEMBLY).map(|mut a| {
        a.name = e.name.clone();
        a
    })).collect()
}

fn finding_row(f: &afrobim_junction::Finding) -> (String, String, String) {
    (f.code.clone(), format!("{:?}", f.severity).to_lowercase(), format!("{}: {}", f.subject, f.message))
}

/// Adds the alignments as elements (visible), returning their ids and the name each was given.
fn add_alignments(doc: &mut Document, list: &[Alignment]) -> (Vec<u64>, Vec<(String, String)>) {
    let mut ids = Vec::new();
    let mut renames = Vec::new();
    for a in list {
        if let Some(id) = crate::alignment_glue::add(doc, a) {
            let given = doc.find(id).map(|e| e.name.clone()).unwrap_or_default();
            if given != a.name {
                renames.push((a.name.clone(), given));
            }
            ids.push(id.0);
        }
    }
    if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == crate::alignment_glue::LAYER) {
        layer.visible = true;
    }
    (ids, renames)
}

fn remove_all(doc: &mut Document, ids: &[u64]) {
    for id in ids {
        doc.remove(ElementId(*id));
    }
}

fn low_points_of(r: &IntersectionResult) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for e in &r.edges {
        out.extend(e.low_points.iter().map(|p| [p.x, p.y, p.level_m]));
    }
    for ret in &r.returns {
        if let Some(v) = &ret.vertical {
            out.extend(v.low_points.iter().map(|p| [p.x, p.y, p.level_m]));
        }
    }
    out
}

/// Builds an intersection from its inputs and the alignments of the document, and (re)makes its
/// objects. `existing` is the intersection element being refreshed, if any. Returns the element and
/// notes for the status line.
pub fn make_intersection(doc: &mut Document, existing: Option<ElementId>, name: &str, inputs: IntersectionInputs, ground: Option<u64>, example: bool, source: &str) -> Result<(ElementId, Vec<String>), String> {
    let aligns = all_alignments(doc);
    let result = build_intersection(&inputs, &aligns).map_err(|e| e.to_string())?;
    let mut notes = Vec::new();
    // Take away what an earlier build made.
    if let Some(id) = existing {
        if let Some(old) = civil::get::<IntersectionData>(doc, id, INTERSECTION) {
            remove_all(doc, &old.created);
            if let Some(c) = old.corridor {
                doc.remove(ElementId(c));
            }
        }
    }
    let mut list: Vec<Alignment> = result.alignments.clone();
    list.extend(result.islands.iter().map(|i| i.alignment.clone()));
    if let Some(c) = &result.corridor {
        for a in &c.alignments {
            if !list.iter().any(|x| x.name == a.name) && !aligns.iter().any(|x| x.name == a.name) {
                list.push(a.clone());
            }
        }
        notes.extend(c.missing.iter().map(|m| format!("Corridor: {m}")));
    }
    let (created, renames) = add_alignments(doc, &list);
    let mut corridor_id = None;
    if let (Some(c), Some(g)) = (&result.corridor, ground) {
        let mut model = c.model.clone();
        for b in &mut model.baselines {
            if let Some((_, to)) = renames.iter().find(|(from, _)| *from == b.alignment) {
                b.alignment = to.clone();
            }
        }
        let cd = CorridorData { model, ground: g, notes: c.notes.clone() };
        let b = Bounds { min_e: result.crossing.x - 40.0, min_n: result.crossing.y - 40.0, max_e: result.crossing.x + 40.0, max_n: result.crossing.y + 40.0 };
        let cname = format!("{name} corridor");
        corridor_id = Some(civil::add(doc, CORRIDOR, &cname, serde_json::to_value(&cd).unwrap_or_default(), Some(b), source).0);
    } else if result.corridor.is_some() {
        notes.push("The corridor regions were not made: choose a ground surface.".into());
    }
    let data = IntersectionData { inputs, created, corridor: corridor_id, ground, dependencies: Some(result.dependencies.clone()), crossing: [result.crossing.x, result.crossing.y], low_points: low_points_of(&result), findings: result.findings.iter().map(finding_row).collect(), example };
    let b = Bounds::point(result.crossing.x, result.crossing.y);
    let id = match existing {
        Some(id) => {
            civil::set(doc, id, serde_json::to_value(&data).unwrap_or_default(), Some(b));
            id
        }
        None => civil::add(doc, INTERSECTION, name, serde_json::to_value(&data).unwrap_or_default(), Some(b), source),
    };
    Ok((id, notes))
}

/// The intersections whose source alignments or assemblies changed since they were built, with what changed.
pub fn stale_intersections(doc: &Document) -> Vec<(ElementId, Vec<String>)> {
    let aligns = all_alignments(doc);
    let asms = assemblies_of(doc);
    doc.elements().iter().filter(|e| e.category == INTERSECTION).filter_map(|e| {
        let d = civil::get::<IntersectionData>(doc, e.id, INTERSECTION)?;
        let stale = d.dependencies?.stale(&aligns, &asms);
        (!stale.is_empty()).then_some((e.id, stale))
    }).collect()
}

pub fn make_roundabout(doc: &mut Document, existing: Option<ElementId>, name: &str, inputs: RoundaboutInputs, example: bool, source: &str) -> Result<(ElementId, RoundaboutResult), String> {
    let aligns = all_alignments(doc);
    let result = build_roundabout(&inputs, &aligns).map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        if let Some(old) = civil::get::<RoundaboutData>(doc, id, ROUNDABOUT) {
            remove_all(doc, &old.created);
        }
    }
    let mut list: Vec<Alignment> = vec![result.inscribed_circle.clone(), result.carriageway_inner_edge.clone(), result.central_island_edge.clone(), result.circulatory_centreline.clone()];
    for a in &result.arms {
        list.push(a.entry_kerb.clone());
        list.push(a.exit_kerb.clone());
        list.push(a.splitter.alignment.clone());
    }
    let (created, _) = add_alignments(doc, &list);
    let arms = result.arms.iter().map(|a| ArmReport { name: a.name.clone(), entry_angle_deg: a.measures.entry_angle_rad.to_degrees(), entry_path_radius_m: a.measures.entry_path_radius_m, entry_width_m: a.measures.entry_width_m, exit_width_m: a.measures.exit_width_m, entry_radius_m: a.measures.entry_radius_m, exit_radius_m: a.measures.exit_radius_m, flare_ratio: a.measures.flare_ratio }).collect();
    let data = RoundaboutData { inputs, created, dependencies: Some(result.dependencies.clone()), arms, findings: result.findings.iter().map(finding_row).collect(), example };
    let b = Bounds::point(result.centre.0, result.centre.1);
    let id = match existing {
        Some(id) => {
            civil::set(doc, id, serde_json::to_value(&data).unwrap_or_default(), Some(b));
            id
        }
        None => civil::add(doc, ROUNDABOUT, name, serde_json::to_value(&data).unwrap_or_default(), Some(b), source),
    };
    Ok((id, result))
}

pub fn stale_roundabouts(doc: &Document) -> Vec<(ElementId, Vec<String>)> {
    let aligns = all_alignments(doc);
    doc.elements().iter().filter(|e| e.category == ROUNDABOUT).filter_map(|e| {
        let d = civil::get::<RoundaboutData>(doc, e.id, ROUNDABOUT)?;
        let stale = d.dependencies?.stale(&aligns, &[]);
        (!stale.is_empty()).then_some((e.id, stale))
    }).collect()
}

// ─── scenes ─────────────────────────────────────────────────────────────────

const C_MARK: [u8; 3] = [0xE0, 0x6C, 0x9F];
const C_TEXT: [u8; 3] = [0xA8, 0xB0, 0xBA];

fn ring(c: (f64, f64), r: f64) -> Vec<[f64; 2]> {
    (0..=16).map(|i| { let a = std::f64::consts::TAU * i as f64 / 16.0; mm(c.0 + r * a.cos(), c.1 + r * a.sin()) }).collect()
}

pub fn build_intersection_scene(e: &Element, _ctx: &Ctx) -> Result<Built, String> {
    let d: IntersectionData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The intersection data cannot be read: {x}"))?;
    let mut s = Scene::default();
    let c = (d.crossing[0], d.crossing[1]);
    s.lines.push(SLine::new(ring(c, 0.8), C_MARK, 1.8));
    s.lines.push(SLine::new(vec![mm(c.0 - 1.2, c.1), mm(c.0 + 1.2, c.1)], C_MARK, 1.4));
    s.lines.push(SLine::new(vec![mm(c.0, c.1 - 1.2), mm(c.0, c.1 + 1.2)], C_MARK, 1.4));
    s.texts.push(SText { at: mm(c.0, c.1), text: e.name.clone(), rgb: C_MARK, height_mm: 0.0, px: 11.0, anchor: 3, prio: 6, halo: true });
    for p in &d.low_points {
        s.lines.push(SLine::new(ring((p[0], p[1]), 0.4), C_MARK, 1.6));
        s.texts.push(SText { at: mm(p[0], p[1]), text: format!("low {:.3}", p[2]), rgb: C_MARK, height_mm: 0.0, px: 9.5, anchor: 4, prio: 4, halo: true });
    }
    for (code, sev, msg) in &d.findings {
        if sev != "info" {
            s.notes.push(format!("{code}: {msg}"));
        }
    }
    s.derived.push(("Primary".into(), d.inputs.primary.clone()));
    s.derived.push(("Secondary".into(), d.inputs.secondary.clone()));
    s.derived.push(("Alignments made".into(), d.created.len().to_string()));
    s.derived.push(("Kerb return low points".into(), d.low_points.len().to_string()));
    s.derived.push(("Findings".into(), d.findings.len().to_string()));
    if d.example {
        s.derived.push(("Data".into(), "example values, not a design".into()));
    }
    Ok(Built { scene: s, ..Default::default() })
}

pub fn build_roundabout_scene(e: &Element) -> Result<Built, String> {
    let d: RoundaboutData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The roundabout data cannot be read: {x}"))?;
    let mut s = Scene::default();
    let c = d.inputs.centre;
    s.lines.push(SLine::new(vec![mm(c.0 - 1.0, c.1), mm(c.0 + 1.0, c.1)], C_MARK, 1.4));
    s.lines.push(SLine::new(vec![mm(c.0, c.1 - 1.0), mm(c.0, c.1 + 1.0)], C_MARK, 1.4));
    s.texts.push(SText { at: mm(c.0, c.1), text: e.name.clone(), rgb: C_MARK, height_mm: 0.0, px: 11.0, anchor: 3, prio: 6, halo: true });
    for (code, sev, msg) in &d.findings {
        if sev != "info" {
            s.notes.push(format!("{code}: {msg}"));
        }
    }
    s.derived.push(("Inscribed diameter (m)".into(), format!("{:.2}", d.inputs.inscribed_diameter_m)));
    s.derived.push(("Arms".into(), d.arms.len().to_string()));
    for a in &d.arms {
        s.derived.push((format!("Entry angle, {}", a.name), format!("{:.1} deg (AfroBIM's own construction)", a.entry_angle_deg)));
        if let Some(r) = a.entry_path_radius_m {
            s.derived.push((format!("Entry path radius, {}", a.name), format!("{r:.1} m (AfroBIM's own construction)")));
        }
    }
    if d.example {
        s.derived.push(("Data".into(), "example values, not a design".into()));
    }
    let _ = C_TEXT;
    Ok(Built { scene: s, ..Default::default() })
}

/// Whether an element's roundabout data reads (for the tests of references).
#[cfg(test)]
pub fn roundabout_data_ok(e: &afrobim_kernel::Element) -> bool {
    serde_json::from_value::<RoundaboutData>(e.data.clone()).is_ok()
}
