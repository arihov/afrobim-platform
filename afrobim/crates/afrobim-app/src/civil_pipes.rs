//! Pipe networks as civil objects: the element data, the scene (plan, profile and 3D) built from the
//! `afrobim-pipes` engine, and the helpers the dialogs and the toolspace share.
//!
//! A network is one element (`civil-pipenet`) whose data is the engine's `PipeNetwork` with the
//! project's parts list, rule set and the inputs the checks need. It sits at source coordinates: its
//! origin is the least easting and northing of its structures. A run drawn in a profile view is a
//! second element (`civil-pipeprofile`) that reads the network.

use afrobim_corridor::Tin;
use afrobim_draw::Mesh3;
use afrobim_kernel::{Document, Element, ElementId};
use afrobim_pipes::{Footprint, HydraulicSettings, PartsCatalogue, PipeId, PipeNetwork, RuleSet, StructureInner, TrenchSpec};
use serde::{Deserialize, Serialize};

use crate::civil::{self, Bounds, PIPENET};
use crate::civil_build::{mm, Built, Ctx};
use crate::civil_scene::{SFill, SLine, SText, Scene};

/// The data of a pipe network element.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipeNetData {
    pub network: PipeNetwork,
    /// The parts list the network was made with (an example list is labelled as such in its name).
    pub catalogue: PartsCatalogue,
    pub rules: RuleSet,
    /// The ground surface (element id) cover and trenches are measured against.
    pub surface: Option<u64>,
    /// Kinematic viscosity for Colebrook-White roughness: an input, no default.
    pub viscosity_m2_s: Option<f64>,
    /// The tailwater level of the grade lines: an input.
    pub tailwater_m: Option<f64>,
    /// The clearance of the interference check: an input.
    pub clearance_m: Option<f64>,
    /// Spacing of the cover samples along a pipe (blank: ends only).
    pub cover_spacing_m: Option<f64>,
    /// Other networks (element ids) the interference check compares this one with.
    pub partners: Vec<u64>,
    /// The trench inputs of the quantities.
    pub trench: Option<TrenchSpec>,
    /// Demonstration data: every number is an example.
    pub example: bool,
}

impl Default for PipeNetData {
    fn default() -> Self {
        PipeNetData { network: PipeNetwork::new("Pipe network"), catalogue: PartsCatalogue::default(), rules: RuleSet::default(), surface: None, viscosity_m2_s: None, tailwater_m: None, clearance_m: None, cover_spacing_m: None, partners: vec![], trench: None, example: false }
    }
}

impl PipeNetData {
    pub fn settings(&self) -> HydraulicSettings {
        HydraulicSettings { kinematic_viscosity_m2_s: self.viscosity_m2_s }
    }
}

/// The data of a pipe profile element: a run of one network drawn in a profile view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeProfileData {
    pub network: u64,
    /// The pipes of the run, upstream to downstream (engine pipe ids).
    pub pipes: Vec<u32>,
    /// Draw along this alignment (stations and offsets) instead of the run's own path.
    pub alignment: Option<String>,
    pub surface: Option<u64>,
    /// Lower left corner of the view: easting, northing.
    pub insertion: [f64; 2],
    pub vertical_exaggeration: f64,
    /// Spacing of the ground samples along the run.
    pub ground_spacing_m: f64,
    /// Draw the hydraulic grade line from this tailwater level.
    pub tailwater_m: Option<f64>,
}

pub fn bounds_of(net: &PipeNetwork) -> Option<Bounds> {
    Bounds::of(net.structures().map(|s| (s.easting_m, s.northing_m)))
}

pub fn data_of(doc: &Document, id: ElementId) -> Option<PipeNetData> {
    civil::get(doc, id, PIPENET)
}

/// Stores an edited network (data and bounds) and bumps its revision.
pub fn store(doc: &mut Document, id: ElementId, data: &PipeNetData) {
    let b = bounds_of(&data.network);
    let v = serde_json::to_value(data).unwrap_or_default();
    civil::set(doc, id, v, b);
}

pub fn add(doc: &mut Document, name: &str, data: &PipeNetData, source: &str) -> ElementId {
    let b = bounds_of(&data.network);
    let v = serde_json::to_value(data).unwrap_or_default();
    civil::add(doc, PIPENET, name, v, b, source)
}

pub fn networks(doc: &Document) -> Vec<(ElementId, String)> {
    civil::of_category(doc, PIPENET)
}

// ─── colours ────────────────────────────────────────────────────────────────

const C_PIPE: [u8; 3] = [0x4D, 0xA3, 0xFF];
const C_STRUCT: [u8; 3] = [0xE0, 0xB0, 0x40];
const C_HGL: [u8; 3] = [0x62, 0xC9, 0xE8];
const C_EGL: [u8; 3] = [0xA4, 0x8F, 0xE0];
const C_TEXT: [u8; 3] = [0xA8, 0xB0, 0xBA];
const C_GRID: [u8; 3] = [0x88, 0x90, 0x9A];
const C_GROUND: [u8; 3] = [0x4F, 0xA0, 0x62];

fn circle_pts(cx: f64, cy: f64, r: f64, n: usize) -> Vec<[f64; 2]> {
    (0..=n).map(|i| {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        [cx + r * a.cos(), cy + r * a.sin()]
    }).collect()
}

// ─── the plan and 3D scene of a network ─────────────────────────────────────

pub fn build_pipenet(e: &Element, _ctx: &Ctx) -> Result<Built, String> {
    let d: PipeNetData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The pipe network data cannot be read: {x}"))?;
    let net = &d.network;
    let mut s = Scene::default();
    let plan = net.plan_geometry();
    // Pipes: the outside width as a band, the centreline, a flow arrow at the middle and a label.
    for p in &plan.pipes {
        let half = (p.outer_width_m / 2.0).max(0.0);
        for w in p.centreline.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = dx.hypot(dy);
            if len < 1e-9 || half <= 0.0 {
                continue;
            }
            let (nx, ny) = (-dy / len * half, dx / len * half);
            let q = [mm(a[0] + nx, a[1] + ny), mm(b[0] + nx, b[1] + ny), mm(b[0] - nx, b[1] - ny), mm(a[0] - nx, a[1] - ny)];
            s.fills.push(SFill { poly: q.to_vec(), rgba: [C_PIPE[0], C_PIPE[1], C_PIPE[2], 70] });
        }
        let pts: Vec<[f64; 2]> = p.centreline.iter().map(|q| mm(q[0], q[1])).collect();
        s.lines.push(SLine::new(pts, C_PIPE, 1.5));
        if let Ok(path) = net.pipe_path(p.id) {
            let mid = path.point_at(path.length_m / 2.0);
            let ahead = path.point_at(path.length_m / 2.0 + 0.05_f64.min(path.length_m / 4.0));
            let (dx, dy) = (ahead[0] - mid[0], ahead[1] - mid[1]);
            let l = dx.hypot(dy);
            if l > 1e-9 {
                let (ux, uy) = (dx / l, dy / l);
                let size = (p.outer_width_m * 1.2).max(0.4);
                let tip = [mid[0] + ux * size * 0.5, mid[1] + uy * size * 0.5];
                let (bx, by) = (mid[0] - ux * size * 0.5, mid[1] - uy * size * 0.5);
                let (px, py) = (-uy * size * 0.35, ux * size * 0.35);
                s.lines.push(SLine::new(vec![mm(bx + px, by + py), mm(tip[0], tip[1]), mm(bx - px, by - py)], C_PIPE, 1.5));
            }
            let label = format!("{}  {}", p.name, p.size_label);
            s.texts.push(SText { at: mm(mid[0], mid[1]), text: label, rgb: C_TEXT, height_mm: 0.0, px: 10.0, anchor: 4, prio: 3, halo: true });
        }
    }
    // Structures: their plan footprint, or a small circle where none is given (the marker has no
    // meaning as a size), and the name.
    for st in &plan.structures {
        let (x, y) = (st.easting_m, st.northing_m);
        match st.footprint {
            Footprint::Circle { radius_m } => s.lines.push(SLine::new(circle_pts(x, y, radius_m, 32).into_iter().map(|q| mm(q[0], q[1])).collect(), C_STRUCT, 1.8)),
            Footprint::Rectangle { length_m, width_m } => {
                let (hx, hy) = (length_m / 2.0, width_m / 2.0);
                s.lines.push(SLine::new(vec![mm(x - hx, y - hy), mm(x + hx, y - hy), mm(x + hx, y + hy), mm(x - hx, y + hy), mm(x - hx, y - hy)], C_STRUCT, 1.8));
            }
            Footprint::Point => s.lines.push(SLine::new(circle_pts(x, y, 0.2, 16).into_iter().map(|q| mm(q[0], q[1])).collect(), C_STRUCT, 1.8)),
        }
        s.texts.push(SText { at: mm(x, y), text: st.name.clone(), rgb: C_STRUCT, height_mm: 0.0, px: 11.0, anchor: 3, prio: 6, halo: true });
    }
    // 3D: structures as solids from their sump (or the lowest invert) to the rim, pipes as tubes.
    let origin = (e.origin.x / 1000.0, e.origin.y / 1000.0);
    let local = |x: f64, y: f64, z: f64| [(x - origin.0) * 1000.0, (y - origin.1) * 1000.0, z * 1000.0];
    for st in net.structures() {
        let lowest = net.incoming(st.id).into_iter().filter_map(|p| net.pipe(p).ok().map(|p| p.end_invert_m)).chain(net.outgoing(st.id).into_iter().filter_map(|p| net.pipe(p).ok().map(|p| p.start_invert_m))).fold(f64::INFINITY, f64::min);
        let z0 = st.sump_level_m.unwrap_or(if lowest.is_finite() { lowest } else { st.rim_level_m - 0.5 });
        let wall = st.wall_thickness_m.unwrap_or(0.0);
        match st.inner {
            Some(StructureInner::Rectangular { length_m, width_m }) => {
                let (hx, hy) = (length_m / 2.0 + wall, width_m / 2.0 + wall);
                s.meshes.push(Mesh3::cuboid(&st.name, local(st.easting_m - hx, st.northing_m - hy, z0), local(st.easting_m + hx, st.northing_m + hy, st.rim_level_m), C_STRUCT));
            }
            other => {
                let r = other.map(|i| match i { StructureInner::Circular { diameter_m } => diameter_m / 2.0, StructureInner::Rectangular { .. } => 0.0 }).unwrap_or(0.3) + wall;
                let c = local(st.easting_m, st.northing_m, z0);
                s.meshes.push(Mesh3::cylinder(&st.name, c[0], c[1], z0 * 1000.0, st.rim_level_m * 1000.0, r * 1000.0, 24, C_STRUCT));
            }
        }
    }
    for p in net.pipes() {
        let Ok(c3) = net.centreline_3d(p.id) else { continue };
        let wall = p.wall_thickness_m.unwrap_or(0.0);
        let (w, h) = (p.shape.width_m() / 2.0 + wall, p.shape.height_m() / 2.0 + wall);
        let round = matches!(p.shape, afrobim_pipes::PipeShape::Circular { .. });
        let pts: Vec<[f64; 3]> = c3.iter().map(|q| local(q[0], q[1], q[2])).collect();
        s.meshes.push(tube(&p.name, &pts, w * 1000.0, h * 1000.0, round, C_PIPE));
    }
    // Findings of the structure of the network that make the drawing misleading.
    for f in net.topology_findings() {
        if f.severity != afrobim_pipes::Severity::Info {
            s.notes.push(f.message.clone());
        }
    }
    let topo = net.topology();
    let total: f64 = net.pipes().map(|p| net.plan_length_m(p.id).unwrap_or(0.0)).sum();
    s.derived.push(("Structures".into(), net.structure_count().to_string()));
    s.derived.push(("Pipes".into(), net.pipe_count().to_string()));
    s.derived.push(("Total pipe length (m)".into(), format!("{total:.2}")));
    s.derived.push(("Type".into(), format!("{:?}", net.network_type).to_lowercase()));
    s.derived.push(("Status".into(), format!("{:?}", net.status).to_lowercase()));
    s.derived.push(("Tree".into(), if topo.is_tree() { "yes".into() } else { "no (loops or branches)".to_string() }));
    s.derived.push(("Outlets".into(), topo.outlets.iter().filter_map(|i| net.structure(*i).ok().map(|s| s.name.clone())).collect::<Vec<_>>().join(", ")));
    if d.example {
        s.derived.push(("Data".into(), "example values, not a design".into()));
    }
    Ok(Built { scene: s, ..Default::default() })
}

/// A tube along a polyline in element-local millimetres: a round or a rectangular section of half
/// width `rx` and half height `ry`, with end caps. Triangles face outward.
pub fn tube(name: &str, pts: &[[f64; 3]], rx: f64, ry: f64, round: bool, colour: [u8; 3]) -> Mesh3 {
    let n = if round { 16 } else { 4 };
    let ring: Vec<(f64, f64)> = if round {
        (0..n).map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            (rx.max(ry) * a.cos(), rx.max(ry) * a.sin())
        }).collect()
    } else {
        vec![(rx, ry), (-rx, ry), (-rx, -ry), (rx, -ry)]
    };
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let norm = |a: [f64; 3]| {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        if l < 1e-12 { [0.0, 0.0, 0.0] } else { [a[0] / l, a[1] / l, a[2] / l] }
    };
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut centres: Vec<[f64; 3]> = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let before = if i > 0 { norm(sub(*p, pts[i - 1])) } else { [0.0; 3] };
        let after = if i + 1 < pts.len() { norm(sub(pts[i + 1], *p)) } else { [0.0; 3] };
        let d = norm([before[0] + after[0], before[1] + after[1], before[2] + after[2]]);
        let h = {
            let c = cross(d, [0.0, 0.0, 1.0]);
            if c[0].abs() + c[1].abs() < 1e-9 { [1.0, 0.0, 0.0] } else { norm(c) }
        };
        let u = norm(cross(h, d));
        for (a, b) in &ring {
            vertices.push([p[0] + a * h[0] + b * u[0], p[1] + a * h[1] + b * u[1], p[2] + a * h[2] + b * u[2]]);
        }
        centres.push(*p);
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let add = |a: u32, b: u32, c: u32, outward: [f64; 3], tris: &mut Vec<[u32; 3]>, vs: &Vec<[f64; 3]>| {
        let nrm = cross(sub(vs[b as usize], vs[a as usize]), sub(vs[c as usize], vs[a as usize]));
        let facing = nrm[0] * outward[0] + nrm[1] * outward[1] + nrm[2] * outward[2];
        if facing >= 0.0 { tris.push([a, b, c]) } else { tris.push([a, c, b]) }
    };
    for i in 0..pts.len().saturating_sub(1) {
        let axis = [(centres[i][0] + centres[i + 1][0]) / 2.0, (centres[i][1] + centres[i + 1][1]) / 2.0, (centres[i][2] + centres[i + 1][2]) / 2.0];
        for j in 0..n {
            let (j2, base0, base1) = ((j + 1) % n, i * n, (i + 1) * n);
            let (a, b, c, d) = ((base0 + j) as u32, (base0 + j2) as u32, (base1 + j2) as u32, (base1 + j) as u32);
            let mid = [(vertices[a as usize][0] + vertices[c as usize][0]) / 2.0, (vertices[a as usize][1] + vertices[c as usize][1]) / 2.0, (vertices[a as usize][2] + vertices[c as usize][2]) / 2.0];
            let outward = sub(mid, axis);
            add(a, b, c, outward, &mut tris, &vertices);
            add(a, c, d, outward, &mut tris, &vertices);
        }
    }
    // Caps: a fan around the end centre, facing along the pipe.
    if pts.len() >= 2 {
        for (end, other) in [(0usize, 1usize), (pts.len() - 1, pts.len() - 2)] {
            let c = vertices.len() as u32;
            vertices.push(centres[end]);
            let outward = sub(centres[end], centres[other]);
            for j in 0..n {
                let (a, b) = ((end * n + j) as u32, (end * n + (j + 1) % n) as u32);
                add(c, a, b, outward, &mut tris, &vertices);
            }
        }
    }
    Mesh3 { name: name.to_string(), vertices, triangles: tris, colour }
}

// ─── the profile of a run ───────────────────────────────────────────────────

fn network_in(ctx: &Ctx, id: u64) -> Option<PipeNetData> {
    let e = ctx.element(id)?;
    if e.category != PIPENET {
        return None;
    }
    serde_json::from_value(e.data.clone()).ok()
}

/// The profile of a run as the engine gives it, with the ground read from the surface.
pub fn run_profile(d: &PipeNetData, pp: &PipeProfileData, tin: Option<&Tin>, al: Option<&afrobim_alignment::Alignment>) -> Result<afrobim_pipes::ProfileView, String> {
    let ids: Vec<PipeId> = pp.pipes.iter().map(|i| PipeId(*i)).collect();
    if ids.is_empty() {
        return Err("The run has no pipes.".into());
    }
    let ground: Option<&dyn afrobim_pipes::Ground> = tin.map(|t| t as &dyn afrobim_pipes::Ground);
    match al {
        Some(a) => d.network.profile_on_alignment(&ids, a, ground, pp.ground_spacing_m).map_err(|e| e.to_string()),
        None => d.network.run_profile(&ids, ground, pp.ground_spacing_m).map_err(|e| e.to_string()),
    }
}

pub fn build_pipe_profile(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let pp: PipeProfileData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The pipe profile data cannot be read: {x}"))?;
    let d = network_in(ctx, pp.network).ok_or("The pipe network of this profile is not in the document.")?;
    let tin = pp.surface.and_then(|s| ctx.tin(s));
    let al = pp.alignment.as_deref().and_then(|n| ctx.alignment_named(n));
    let view = run_profile(&d, &pp, tin.as_deref(), al.as_ref())?;
    let mut s = Scene::default();
    // Datum and extent.
    let mut levels: Vec<f64> = view.ground_line.iter().map(|p| p[1]).collect();
    for p in &view.pipes {
        levels.extend(p.invert_line.iter().map(|q| q[1]));
        levels.extend(p.crown_line.iter().map(|q| q[1]));
    }
    for st in &view.structures {
        levels.push(st.rim_level_m);
    }
    let mut report: Vec<String> = Vec::new();
    let grade = pp.tailwater_m.map(|t| d.network.grade_lines(t, &d.settings()));
    if let Some(g) = &grade {
        levels.extend(g.pipes.iter().flat_map(|p| [p.hgl_start_m, p.hgl_end_m, p.egl_start_m, p.egl_end_m]));
    }
    let levels: Vec<f64> = levels.into_iter().filter(|v| v.is_finite()).collect();
    if levels.is_empty() {
        return Err("The run has no levels to draw.".into());
    }
    let (zmin, zmax) = (levels.iter().cloned().fold(f64::INFINITY, f64::min), levels.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let xs: Vec<f64> = view.pipes.iter().flat_map(|p| p.invert_line.iter().map(|q| q[0])).chain(view.structures.iter().map(|s| s.x_m)).collect();
    let (x0, x1) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let step = crate::civil_build::nice_step((zmax - zmin).max(0.5) / 5.0);
    let datum = (zmin / step).floor() * step - step;
    let top = ((zmax / step).ceil() + 1.0) * step;
    let ex = pp.vertical_exaggeration.max(0.01);
    let (ox, oy) = (pp.insertion[0] * 1000.0, pp.insertion[1] * 1000.0);
    let pt = |x: f64, z: f64| [ox + (x - x0) * 1000.0, oy + (z - datum) * ex * 1000.0];
    // Frame and horizontal grid with level labels.
    let box_ = vec![pt(x0, datum), pt(x1, datum), pt(x1, top), pt(x0, top), pt(x0, datum)];
    s.lines.push(SLine::new(box_, C_GRID, 1.0));
    let mut z = datum;
    while z <= top + 1e-9 {
        s.lines.push(SLine::new(vec![pt(x0, z), pt(x1, z)], C_GRID, 0.5));
        let at = pt(x0, z);
        s.texts.push(SText { at: [at[0] - 300.0, at[1]], text: format!("{z:.2}"), rgb: C_TEXT, height_mm: 0.0, px: 10.0, anchor: 2, prio: 2, halo: false });
        z += step;
    }
    // Ground.
    if view.ground_line.len() >= 2 {
        s.lines.push(SLine::new(view.ground_line.iter().map(|p| pt(p[0], p[1])).collect(), C_GROUND, 1.6));
    }
    // Structures: a vertical band between the rim and the sump (or lowest invert).
    for st in &view.structures {
        let half = d.network.structure(st.id).ok().and_then(|x| x.inner).map(|i| i.bounding_radius_m()).unwrap_or(0.3).max(0.2);
        let bottom = st.sump_level_m.or(st.lowest_invert_m).unwrap_or(st.rim_level_m - 0.5);
        let q = vec![pt(st.x_m - half, bottom), pt(st.x_m + half, bottom), pt(st.x_m + half, st.rim_level_m), pt(st.x_m - half, st.rim_level_m), pt(st.x_m - half, bottom)];
        s.lines.push(SLine::new(q, C_STRUCT, 1.6));
        let top_pt = pt(st.x_m, st.rim_level_m);
        s.texts.push(SText { at: top_pt, text: format!("{}  rim {:.2}", st.name, st.rim_level_m), rgb: C_STRUCT, height_mm: 0.0, px: 10.5, anchor: 3, prio: 6, halo: true });
    }
    // Pipes: invert and crown lines with the barrel between them, and a label.
    for p in &view.pipes {
        if p.invert_line.len() < 2 {
            continue;
        }
        let inv: Vec<[f64; 2]> = p.invert_line.iter().map(|q| pt(q[0], q[1])).collect();
        let crown: Vec<[f64; 2]> = p.crown_line.iter().map(|q| pt(q[0], q[1])).collect();
        for k in 0..inv.len().saturating_sub(1).min(crown.len().saturating_sub(1)) {
            s.fills.push(SFill { poly: vec![inv[k], inv[k + 1], crown[k + 1], crown[k]], rgba: [C_PIPE[0], C_PIPE[1], C_PIPE[2], 80] });
        }
        s.lines.push(SLine::new(inv.clone(), C_PIPE, 1.6));
        s.lines.push(SLine::new(crown, C_PIPE, 1.6));
        let (a, b) = (inv[0], inv[inv.len() - 1]);
        let run = (p.invert_line.last().unwrap()[0] - p.invert_line[0][0]).abs().max(1e-9);
        let slope = (p.end_invert_m - p.start_invert_m) / run * 100.0;
        s.texts.push(SText { at: [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], text: format!("{}  {}  {:.2} %", p.name, p.size_label, slope), rgb: C_TEXT, height_mm: 0.0, px: 10.0, anchor: 4, prio: 4, halo: true });
    }
    // Hydraulic and energy grade lines.
    if let Some(g) = &grade {
        for pg in &g.pipes {
            let Some(vp) = view.pipes.iter().find(|v| v.id == pg.pipe) else { continue };
            let (xa, xb) = (vp.invert_line.first().map(|q| q[0]), vp.invert_line.last().map(|q| q[0]));
            if let (Some(xa), Some(xb)) = (xa, xb) {
                s.lines.push(SLine::new(vec![pt(xa, pg.hgl_start_m), pt(xb, pg.hgl_end_m)], C_HGL, 2.0));
                s.lines.push(SLine::new(vec![pt(xa, pg.egl_start_m), pt(xb, pg.egl_end_m)], C_EGL, 1.0));
            }
        }
        s.derived.push(("Grade lines".into(), format!("tailwater {:.2} m{}", g.tailwater_level_m, if g.friction_only { ", friction only (no structure losses entered)" } else { "" })));
        for f in &g.findings {
            if f.severity == afrobim_pipes::Severity::Info {
                report.push(f.message.clone());
            } else {
                s.notes.push(f.message.clone());
            }
        }
    }
    s.derived.push(("Pipes in the run".into(), view.pipes.len().to_string()));
    s.derived.push(("Vertical exaggeration".into(), format!("{ex}")));
    s.derived.push(("Datum (m)".into(), format!("{datum:.2}")));
    Ok(Built { scene: s, report, ..Default::default() })
}
