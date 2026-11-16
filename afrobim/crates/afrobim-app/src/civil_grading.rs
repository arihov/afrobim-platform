//! Feature lines, sites and grading groups as civil objects, with the plan, slope pattern and 3D
//! scenes built from the `afrobim-grading` engine.
//!
//! A feature line is one element (`civil-featureline`) holding the engine's `FeatureLine` and the name
//! of the site it belongs to; a site is the set of feature lines that carry its name. A grading group
//! is one element (`civil-grading`) holding the engine's `GradingGroup`; it builds one surface (the
//! welded triangles of its gradings and infills), which is listed under Surfaces and rebuilt when a
//! footprint, the target surface or the group settings change. Everything is at source coordinates.

use std::sync::Arc;

use afrobim_grading::{slope_ticks, FeatureLine, GradingGroup, GroupBuild, Site};
use afrobim_kernel::{Document, Element, ElementId};
use serde::{Deserialize, Serialize};

use crate::civil::{self, Bounds, FEATURELINE, GRADING};
use crate::civil_build::{mm, Built, Ctx, MESH_CAP};
use crate::civil_scene::{SLine, SText, Scene, L3};

/// The facet angle the feature lines are drawn with (a drawing tolerance only: no calculation uses it).
pub const DRAW_FACET_RAD: f64 = 0.05;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureLineData {
    pub line: FeatureLine,
    /// The site the line belongs to: lines with the same site name share elevations where they cross.
    pub site: String,
    /// Demonstration data.
    #[serde(default)]
    pub example: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingData {
    pub group: GradingGroup,
    /// The site whose feature lines are the footprints.
    pub site: String,
    /// The target (ground) surface, an element id.
    pub ground: Option<u64>,
    /// Spacing of the slope pattern ticks in plan (blank: no ticks): a drawing setting.
    #[serde(default)]
    pub tick_spacing_m: Option<f64>,
    /// Volumes are measured against this surface (an element id), usually the ground.
    #[serde(default)]
    pub base: Option<u64>,
    #[serde(default)]
    pub example: bool,
}

pub fn line_bounds(l: &FeatureLine) -> Option<Bounds> {
    Bounds::of(l.vertices.iter().map(|v| (v.x, v.y)))
}

pub fn add_line(doc: &mut Document, name: &str, line: FeatureLine, site: &str, source: &str, example: bool) -> ElementId {
    let b = line_bounds(&line);
    let mut line = line;
    line.name = name.to_string();
    let data = FeatureLineData { line, site: site.to_string(), example };
    civil::add(doc, FEATURELINE, name, serde_json::to_value(&data).unwrap_or_default(), b, source)
}

pub fn store_line(doc: &mut Document, id: ElementId, data: &FeatureLineData) {
    let b = line_bounds(&data.line);
    civil::set(doc, id, serde_json::to_value(data).unwrap_or_default(), b);
}

/// The feature line elements of a site: (element id, name, data).
pub fn lines_of(doc: &Document, site: &str) -> Vec<(ElementId, String, FeatureLineData)> {
    doc.elements().iter().filter(|e| e.category == FEATURELINE).filter_map(|e| civil::get::<FeatureLineData>(doc, e.id, FEATURELINE).map(|d| (e.id, e.name.clone(), d))).filter(|(_, _, d)| d.site == site).collect()
}

/// The names of the sites of the document, in the order their first line appears.
pub fn sites(doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in doc.elements().iter().filter(|e| e.category == FEATURELINE) {
        if let Some(d) = civil::get::<FeatureLineData>(doc, e.id, FEATURELINE) {
            if !out.contains(&d.site) {
                out.push(d.site);
            }
        }
    }
    out
}

/// The engine's site of a name from the elements (the element's name is the line's name; the
/// last-edited counters are kept as stored so that the priority rules mean what they said).
pub fn site_from(elements: &[Element], name: &str) -> Site {
    let mut site = Site::new(name);
    for e in elements.iter().filter(|e| e.category == FEATURELINE) {
        if let Ok(d) = serde_json::from_value::<FeatureLineData>(e.data.clone()) {
            if d.site == name {
                let mut line = d.line;
                line.name = e.name.clone();
                site.clock = site.clock.max(line.last_edited);
                site.feature_lines.push(line);
            }
        }
    }
    site
}

/// The counter a line edited now takes: one more than any line of its site has.
pub fn next_edit_counter(doc: &Document, site: &str) -> u64 {
    lines_of(doc, site).iter().map(|l| l.2.line.last_edited).max().unwrap_or(0) + 1
}

/// The footprints' bounds, until the build gives the surface's own.
pub fn footprint_bounds(doc: &Document, d: &GradingData) -> Option<Bounds> {
    Bounds::of(lines_of(doc, &d.site).iter().flat_map(|l| l.2.line.vertices.iter().map(|v| (v.x, v.y))))
}

pub fn add_grading_element(doc: &mut Document, name: &str, data: &GradingData, source: &str) -> ElementId {
    let b = footprint_bounds(doc, data);
    civil::add(doc, GRADING, name, serde_json::to_value(data).unwrap_or_default(), b, source)
}

pub fn store_grading_element(doc: &mut Document, id: ElementId, data: &GradingData) {
    let b = footprint_bounds(doc, data);
    civil::set(doc, id, serde_json::to_value(data).unwrap_or_default(), b);
}

/// The feature lines of a design package as elements, and its group as a grading group.
pub fn add_package(doc: &mut Document, pkg: &afrobim_grading::DesignPackage, ground: Option<ElementId>, source: &str, example: bool) -> ElementId {
    for l in &pkg.site.feature_lines {
        let mut line = l.clone();
        line.last_edited = next_edit_counter(doc, &pkg.site.name);
        add_line(doc, &l.name.clone(), line, &pkg.site.name, source, example);
    }
    let data = GradingData { group: pkg.group.clone(), site: pkg.site.name.clone(), ground: ground.map(|g| g.0), tick_spacing_m: Some(2.0), base: ground.map(|g| g.0), example };
    add_grading_element(doc, &pkg.group.name.clone(), &data, source)
}

// ─── scenes ─────────────────────────────────────────────────────────────────

const C_FL: [u8; 3] = [0x3F, 0xC1, 0xA5];
const C_DAYLIGHT: [u8; 3] = [0xD0, 0x7A, 0x3E];
const C_PROJ: [u8; 3] = [0x9A, 0x8E, 0x70];
const C_TICK: [u8; 3] = [0xC8, 0x8A, 0x5A];
const C_TEXT: [u8; 3] = [0xA8, 0xB0, 0xBA];

pub fn build_featureline(e: &Element) -> Result<Built, String> {
    let d: FeatureLineData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The feature line data cannot be read: {x}"))?;
    let flat = d.line.flatten(DRAW_FACET_RAD).map_err(|x| x.to_string())?;
    let mut s = Scene::default();
    let mut pts: Vec<[f64; 2]> = flat.iter().map(|f| mm(f.p[0], f.p[1])).collect();
    if d.line.closed {
        if let Some(f) = pts.first().copied() {
            pts.push(f);
        }
    }
    s.lines.push(SLine::new(pts, C_FL, 1.8));
    for v in d.line.vertices.iter().filter(|v| v.kind == afrobim_grading::VertexKind::Pi) {
        let r = 0.15;
        let c = (v.x, v.y);
        let ring: Vec<[f64; 2]> = (0..=12).map(|i| { let a = std::f64::consts::TAU * i as f64 / 12.0; mm(c.0 + r * a.cos(), c.1 + r * a.sin()) }).collect();
        s.lines.push(SLine::new(ring, C_FL, 1.2));
    }
    if let Some(v) = d.line.vertices.first() {
        s.texts.push(SText { at: mm(v.x, v.y), text: format!("{}  {:.2}", e.name, v.z), rgb: C_FL, height_mm: 0.0, px: 10.5, anchor: 3, prio: 5, halo: true });
    }
    // 3D: the line at its elevations, in millimetres from the element's origin.
    let origin = (e.origin.x / 1000.0, e.origin.y / 1000.0);
    let l3: Vec<[f64; 3]> = flat.iter().map(|f| [(f.p[0] - origin.0) * 1000.0, (f.p[1] - origin.1) * 1000.0, f.p[2] * 1000.0]).collect();
    s.lines3.push(L3 { pts: l3, rgb: C_FL, width: 2.2 });
    let zs: Vec<f64> = d.line.vertices.iter().map(|v| v.z).collect();
    s.derived.push(("Site".into(), d.site.clone()));
    s.derived.push(("Length (m)".into(), d.line.length().map(|l| format!("{l:.3}")).unwrap_or_default()));
    s.derived.push(("Vertices".into(), format!("{} ({} PIs)", d.line.vertices.len(), d.line.pi_indices().len())));
    s.derived.push(("Lowest level (m)".into(), format!("{:.3}", zs.iter().cloned().fold(f64::INFINITY, f64::min))));
    s.derived.push(("Highest level (m)".into(), format!("{:.3}", zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max))));
    if let Ok(rows) = d.line.grade_table() {
        if let Some(r) = rows.iter().max_by(|a, b| a.grade_percent.abs().total_cmp(&b.grade_percent.abs())) {
            s.derived.push(("Steepest grade (%)".into(), format!("{:+.2} between stations {:.2} and {:.2} m along the line", r.grade_percent, r.station_from, r.station_to)));
        }
    }
    s.derived.push(("Closed".into(), if d.line.closed { "yes".into() } else { "no".to_string() }));
    s.derived.push(("Style priority".into(), d.line.style_priority.to_string()));
    if d.example {
        s.derived.push(("Data".into(), "example values, not a design".into()));
    }
    Ok(Built { scene: s, ..Default::default() })
}

/// Builds a group from the elements of the context: its site's feature lines and its ground.
pub fn run_group(d: &GradingData, ctx: &Ctx) -> Result<GroupBuild, String> {
    let site = site_from(&ctx.elements, &d.site);
    let ground = d.ground.and_then(|g| ctx.tin(g));
    d.group.build(&site, ground.as_deref()).map_err(|e| e.to_string())
}

pub fn build_grading(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let d: GradingData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The grading data cannot be read: {x}"))?;
    let b = run_group(&d, ctx)?;
    let mut s = Scene::default();
    let origin = (e.origin.x / 1000.0, e.origin.y / 1000.0);
    for r in &b.gradings {
        for dl in &r.daylight_lines {
            let mut pts: Vec<[f64; 2]> = dl.points.iter().map(|p| mm(p[0], p[1])).collect();
            if dl.closed {
                if let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
            }
            s.lines.push(SLine::new(pts, C_DAYLIGHT, 1.8));
            let mut l3: Vec<[f64; 3]> = dl.points.iter().map(|p| [(p[0] - origin.0) * 1000.0, (p[1] - origin.1) * 1000.0, p[2] * 1000.0]).collect();
            if dl.closed {
                if let Some(f) = l3.first().copied() {
                    l3.push(f);
                }
            }
            s.lines3.push(L3 { pts: l3, rgb: C_DAYLIGHT, width: 2.0 });
        }
        for pl in &r.projection_lines {
            s.lines.push(SLine::new(vec![mm(pl[0][0], pl[0][1]), mm(pl[1][0], pl[1][1])], C_PROJ, 0.6));
        }
        // The slope pattern: ticks between the footprint and the daylight line, long and short in turn.
        if let Some(sp) = d.tick_spacing_m {
            if let Ok(ticks) = slope_ticks(r, sp, 0.5) {
                for t in ticks {
                    s.lines.push(SLine::new(vec![mm(t.from[0], t.from[1]), mm(t.to[0], t.to[1])], C_TICK, if t.long { 1.0 } else { 0.8 }));
                }
            }
        }
        for w in &r.warnings {
            s.notes.push(format!("{}: {w}", r.name));
        }
        if !r.complete {
            s.notes.push(format!("Grading {}: some rays found no daylight; the faces beside them are missing and the surface does not close.", r.name));
        }
    }
    for w in &b.warnings {
        s.notes.push(w.clone());
    }
    let (mesh, thinned) = crate::civil_build2::mesh_local(&b.tin, origin, MESH_CAP);
    if thinned {
        s.notes.push("The 3D view shows a thinned copy of this surface.".into());
    }
    s.meshes.push(mesh);
    if let Some(f) = b.gradings.first().and_then(|g| g.daylight_lines.first()).and_then(|l| l.points.first()) {
        s.texts.push(SText { at: mm(f[0], f[1]), text: d.group.name.clone(), rgb: C_TEXT, height_mm: 0.0, px: 10.0, anchor: 4, prio: 3, halo: true });
    }
    s.derived.push(("Site".into(), d.site.clone()));
    s.derived.push(("Gradings".into(), d.group.gradings.len().to_string()));
    s.derived.push(("Triangles".into(), b.tin.triangles().len().to_string()));
    s.derived.push(("Shift (m)".into(), format!("{:.3}", d.group.shift)));
    // The footprint lines of the gradings, by name, in the group's site.
    let footprint_len: f64 = d.group.gradings.iter().filter_map(|g| ctx.elements.iter().filter(|x| x.category == FEATURELINE && x.name == g.footprint).find_map(|x| serde_json::from_value::<FeatureLineData>(x.data.clone()).ok().filter(|fl| fl.site == d.site))).filter_map(|fl| fl.line.length().ok()).sum();
    s.derived.push(("Footprint length (m)".into(), format!("{footprint_len:.3}")));
    s.derived.push(("Surface area (m2)".into(), format!("{:.2}", afrobim_terrain::areas(&b.tin).surface)));
    let (rays, failed) = (b.gradings.iter().map(|g| g.rays.len()).sum::<usize>(), b.gradings.iter().flat_map(|g| g.rays.iter()).filter(|r| r.status != afrobim_grading::RayStatus::Ok).count());
    s.derived.push(("Rays".into(), format!("{rays} ({failed} without daylight or trimmed)")));
    if let Some(base) = d.base.or(d.ground).and_then(|g| ctx.tin(g)) {
        if let Ok(v) = b.volume_against(&base, None) {
            s.derived.push(("Cut (m3)".into(), format!("{:.1}", v.cut)));
            s.derived.push(("Fill (m3)".into(), format!("{:.1}", v.fill)));
            s.derived.push(("Net, cut less fill (m3)".into(), format!("{:.1}", v.net)));
        }
    }
    if d.example {
        s.derived.push(("Data".into(), "example values, not a design".into()));
    }
    let tin = Arc::new(b.tin.clone());
    Ok(Built { scene: s, tin: Some(tin), ..Default::default() })
}
