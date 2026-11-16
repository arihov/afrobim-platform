//! Scenes of corridors and sample line groups (see `civil_build`).

use std::sync::Arc;

use afrobim_corridor::{build_model, corridor_surface, daylight_boundary, feature_lines, section_views, CorridorSampling, SurfaceBoundary, Tin};
use afrobim_kernel::Element;

use crate::civil::{CorridorData, SampleData};
use crate::civil_build::{mm, nice_step, Built, Ctx, C_DESIGN, C_GRID, C_GROUND, C_TEXT};
use crate::civil_scene::{L3, SLine, SText, Scene};

/// A mesh of a TIN in the frame of an element whose origin is `origin_m` (easting, northing in
/// metres): millimetres from that origin.
pub fn mesh_local(tin: &Tin, origin_m: (f64, f64), cap: usize) -> (afrobim_draw::Mesh3, bool) {
    let b = tin.bounds();
    let (mut mesh, thinned) = afrobim_corridor::surface_display_mesh(tin, cap);
    let (dx, dy) = ((b.min_x - origin_m.0) * 1000.0, (b.min_y - origin_m.1) * 1000.0);
    for v in &mut mesh.vertices {
        v[0] += dx;
        v[1] += dy;
    }
    (mesh, thinned)
}

fn colour_of(code: &str) -> [u8; 3] {
    match code {
        "Top" => [0xD9, 0x8A, 0x3C],
        "Datum" => [0x8E, 0x7C, 0xC3],
        "Daylight" => [0x3B, 0x8E, 0xE0],
        "ETW" => C_DESIGN,
        _ => [0x7A, 0xB0, 0x9A],
    }
}

/// The colour a shape (a pavement layer, a kerb, a lining) is drawn in, by its code: a display
/// choice only, the same code always in the same colour.
fn shape_colour(code: &str) -> [u8; 3] {
    let h = code.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let palette: [[u8; 3]; 6] = [[0x6E, 0x6E, 0x78], [0xB0, 0x8F, 0x5A], [0x8A, 0x9A, 0x6B], [0x9A, 0x6B, 0x6B], [0x5E, 0x8A, 0x9A], [0xA0, 0xA0, 0xA0]];
    palette[(h % palette.len() as u32) as usize]
}

/// The shapes of the assembly swept along the baseline: between two consecutive sections each
/// shape that appears in both with the same corners becomes a prism (side faces and the two end
/// faces), one mesh per shape code. Capped so that a long corridor stays light.
fn shape_solids(b: &afrobim_corridor::BaselineResult, origin_m: (f64, f64)) -> Vec<afrobim_draw::Mesh3> {
    const TRIANGLE_CAP: usize = 80_000;
    let mut meshes: Vec<afrobim_draw::Mesh3> = Vec::new();
    let mut total = 0usize;
    let at = |m: &afrobim_corridor::ModelSection, (o, z): (f64, f64)| {
        let (x, y) = m.section.plan_point(o);
        [(x - origin_m.0) * 1000.0, (y - origin_m.1) * 1000.0, z * 1000.0]
    };
    for pair in b.sections.windows(2) {
        let (m0, m1) = (&pair[0], &pair[1]);
        if m1.section.station_m <= m0.section.station_m {
            continue;
        }
        for sh in &m0.design.shapes {
            let n = sh.polygon.len();
            if n < 3 {
                continue;
            }
            // The same shape at the next section: same code, side, source and number of corners.
            let Some(next) = m1.design.shapes.iter().find(|x| x.code == sh.code && x.side == sh.side && x.subassembly == sh.subassembly && x.polygon.len() == n) else { continue };
            if total + n * 2 + 2 * (n - 2) > TRIANGLE_CAP {
                return meshes;
            }
            let idx = match meshes.iter().position(|m| m.name == sh.code) {
                Some(i) => i,
                None => {
                    meshes.push(afrobim_draw::Mesh3 { name: sh.code.clone(), vertices: vec![], triangles: vec![], colour: shape_colour(&sh.code) });
                    meshes.len() - 1
                }
            };
            let mesh = &mut meshes[idx];
            let base = mesh.vertices.len() as u32;
            for p in &sh.polygon {
                mesh.vertices.push(at(m0, *p));
            }
            for p in &next.polygon {
                mesh.vertices.push(at(m1, *p));
            }
            let (a, c) = (base, base + n as u32);
            for i in 0..n as u32 {
                let j = (i + 1) % n as u32;
                mesh.triangles.push([a + i, a + j, c + j]);
                mesh.triangles.push([a + i, c + j, c + i]);
            }
            for i in 1..n as u32 - 1 {
                mesh.triangles.push([a, a + i, a + i + 1]);
                mesh.triangles.push([c, c + i + 1, c + i]);
            }
            total += n * 2 + 2 * (n - 2);
        }
    }
    meshes
}

pub fn build_corridor(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let d: CorridorData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The corridor data cannot be read: {x}"))?;
    let ground = ctx.tin(d.ground).ok_or("The ground surface of this corridor is missing or still building.")?;
    let alignments = ctx.alignments();
    for b in &d.model.baselines {
        b.check()?;
    }
    let built = build_model(&d.model, &alignments, &ground).map_err(|x| format!("The corridor cannot be built: {x}"))?;
    let origin = (e.origin.x / 1000.0, e.origin.y / 1000.0);
    let mut out = Built::default();
    let mut codes: Vec<String> = Vec::new();
    let mut section_count = 0;
    for b in &built.baselines {
        section_count += b.sections.len();
        for n in &b.notes {
            out.report.push(format!("{}: {n}", b.name));
        }
        if let Some(first) = b.sections.first() {
            for p in first.design.points.iter().chain(b.sections.iter().flat_map(|s| s.design.points.iter().take(0))) {
                for c in &p.codes {
                    if !codes.contains(c) {
                        codes.push(c.clone());
                    }
                }
            }
        }
        for code in &codes {
            for fl in feature_lines(b, code) {
                let pts: Vec<[f64; 2]> = fl.points.iter().map(|p| mm(p[0], p[1])).collect();
                if pts.len() >= 2 {
                    out.scene.lines.push(SLine::new(pts, colour_of(code), if code == "Daylight" { 1.6 } else { 1.0 }));
                }
            }
        }
        let ring = daylight_boundary(b);
        if ring.len() >= 3 {
            let mut pts: Vec<[f64; 2]> = ring.iter().map(|p| mm(p.0, p.1)).collect();
            pts.push(pts[0]);
            out.scene.lines.push(SLine::new(pts, colour_of("Daylight"), 1.2));
        }
        // The same lines in 3D, at their levels, and the shapes of the assembly as solids.
        for code in &codes {
            for fl in feature_lines(b, code) {
                let pts: Vec<[f64; 3]> = fl.points.iter().map(|p| [(p[0] - origin.0) * 1000.0, (p[1] - origin.1) * 1000.0, p[2] * 1000.0]).collect();
                if pts.len() >= 2 {
                    out.scene.lines3.push(L3 { pts, rgb: colour_of(code), width: if code == "Daylight" { 1.6 } else { 1.0 } });
                }
            }
        }
        out.scene.meshes.extend(shape_solids(b, origin));
        for (name, codes) in [("Top", vec!["Top"]), ("Datum", vec!["Datum"])] {
            match corridor_surface(b, &codes, SurfaceBoundary::None) {
                Ok(tin) => {
                    let (mesh, _) = mesh_local(&tin, origin, 40_000);
                    out.scene.meshes.push(mesh);
                    out.extra.push((format!("{} {name}", b.name), Arc::new(tin)));
                }
                Err(x) => out.report.push(format!("{} {name} surface: {x}", b.name)),
            }
        }
    }
    out.scene.derived.push(("Baselines".into(), built.baselines.len().to_string()));
    out.scene.derived.push(("Sections".into(), section_count.to_string()));
    if let (Some(a), Some(z)) = (built.baselines.first().map(|b| b.start_station_m), built.baselines.first().map(|b| b.end_station_m)) {
        out.scene.derived.push(("Stations".into(), format!("{a:.1} to {z:.1} m")));
    }
    out.report.insert(0, format!("Built {section_count} section(s) on {} baseline(s); {} corridor surface(s).", built.baselines.len(), out.extra.len()));
    Ok(out)
}

/// Cuts the section views of a sample line group: the surfaces it names and, when a corridor is
/// chosen, that corridor's design. Used by the scene of the group and by the section sheets.
pub fn sample_views(d: &SampleData, ctx: &Ctx) -> Result<Vec<afrobim_corridor::SectionView>, String> {
    let al = ctx.alignment_named(&d.group.alignment).ok_or_else(|| format!("The alignment '{}' of this sample line group is missing.", d.group.alignment))?;
    let tins: Vec<(String, Arc<Tin>)> = d.surfaces.iter().filter_map(|id| ctx.element(*id).and_then(|el| ctx.tin(*id).map(|t| (el.name.clone(), t)))).collect();
    let refs: Vec<(&str, &Tin)> = tins.iter().map(|(n, t)| (n.as_str(), t.as_ref())).collect();
    // The corridor's baseline for the design, when one is chosen.
    let corridor_data = d.corridor.and_then(|id| ctx.element(id)).and_then(|el| serde_json::from_value::<CorridorData>(el.data.clone()).ok());
    let ground = corridor_data.as_ref().and_then(|c| ctx.tin(c.ground));
    let baseline = corridor_data.as_ref().and_then(|c| c.model.baselines.iter().find(|b| b.alignment == d.group.alignment));
    let sampling = match (baseline, &ground) {
        (Some(b), Some(g)) => Some(CorridorSampling { baseline: b, ground: g }),
        _ => None,
    };
    section_views(&d.group, &al, &refs, sampling).map_err(|x| format!("The sections cannot be cut: {x}"))
}

pub fn build_samples(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let d: SampleData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The sample line data cannot be read: {x}"))?;
    let views = sample_views(&d, ctx)?;
    let mut sc = Scene::default();
    // In plan: each sample line across the alignment, with its name.
    for v in &views {
        let (l, r) = (v.line.left_swath_m, v.line.right_swath_m);
        let (s, c) = v.heading_rad.sin_cos();
        let a = (v.x + l * s, v.y - l * c);
        let b = (v.x - r * s, v.y + r * c);
        sc.lines.push(SLine::new(vec![mm(a.0, a.1), mm(b.0, b.1)], [0xE0, 0xC0, 0x3B], 1.2));
        sc.texts.push(SText { at: mm(a.0, a.1), text: v.label.clone(), rgb: [0xE0, 0xC0, 0x3B], height_mm: 0.0, px: 9.5, anchor: 3, prio: 3, halo: false });
    }
    // The section views as an array in the model, from the insertion point.
    let cols = d.columns.max(1);
    let ex = if d.vertical_exaggeration.is_finite() && d.vertical_exaggeration > 0.0 { d.vertical_exaggeration } else { 1.0 };
    let swath_w = views.iter().map(|v| v.line.left_swath_m + v.line.right_swath_m).fold(0.0, f64::max);
    // Pipes of the networks of the document that cross each section line (cut by the line at its
    // station, within the swath), drawn in the section view.
    let nets: Vec<afrobim_pipes::PipeNetwork> = ctx.elements.iter().filter(|x| x.category == crate::civil::PIPENET).filter_map(|x| serde_json::from_value::<crate::civil_pipes::PipeNetData>(x.data.clone()).ok().map(|n| n.network)).collect();
    let align = ctx.alignment_named(&d.group.alignment);
    let pipe_secs: Vec<Vec<afrobim_pipes::SectionView>> = views.iter().map(|v| nets.iter().filter_map(|n| align.as_ref().and_then(|a| n.section_at_station(a, v.line.station_m, v.line.left_swath_m.max(v.line.right_swath_m), None).ok())).collect()).collect();
    let (mut zlo, mut zhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for secs in &pipe_secs {
        for sec in secs {
            for c in &sec.crossings {
                zlo = zlo.min(c.outer_bottom_m);
                zhi = zhi.max(c.outer_top_m);
            }
        }
    }
    for set in &d.slices {
        for l in &set.lines {
            for p in &l.points {
                zlo = zlo.min(p[1]);
                zhi = zhi.max(p[1]);
            }
        }
    }
    for v in &views {
        for gl in &v.ground_lines {
            for run in &gl.runs {
                for p in run {
                    zlo = zlo.min(p.1);
                    zhi = zhi.max(p.1);
                }
            }
        }
        if let Some(dv) = &v.design {
            for l in &dv.geometry.links {
                for p in &l.points {
                    zlo = zlo.min(p.1);
                    zhi = zhi.max(p.1);
                }
            }
        }
    }
    if views.is_empty() || !zlo.is_finite() {
        sc.notes.push("There are no section views to draw: add sample lines and a surface.".into());
        return Ok(Built { scene: sc, ..Default::default() });
    }
    let (cell_w, cell_h) = (swath_w + 8.0, (zhi - zlo) * ex + 6.0);
    let step = nice_step((zhi - zlo).max(1e-3) / 3.0);
    for (k, v) in views.iter().enumerate() {
        let (col, row) = ((k % cols) as f64, (k / cols) as f64);
        let (x0, y0) = (d.insertion[0] + col * cell_w, d.insertion[1] - (row + 1.0) * cell_h);
        let left = v.line.left_swath_m;
        let px = |off: f64| (x0 + 4.0 + left + off) * 1000.0;
        let py = |z: f64| (y0 + 2.0 + (z - zlo) * ex) * 1000.0;
        let frame = [[x0 * 1000.0, y0 * 1000.0], [(x0 + cell_w - 1.0) * 1000.0, y0 * 1000.0], [(x0 + cell_w - 1.0) * 1000.0, (y0 + cell_h - 0.5) * 1000.0], [x0 * 1000.0, (y0 + cell_h - 0.5) * 1000.0], [x0 * 1000.0, y0 * 1000.0]];
        sc.lines.push(SLine::new(frame.to_vec(), C_GRID, 0.8));
        sc.lines.push(SLine::new(vec![[px(0.0), py(zlo)], [px(0.0), py(zhi)]], C_GRID, 0.5));
        sc.texts.push(SText { at: [(x0 + 1.0) * 1000.0, (y0 + cell_h - 1.2) * 1000.0], text: v.label.clone(), rgb: C_TEXT, height_mm: 0.0, px: 11.0, anchor: 1, prio: 4, halo: false });
        let mut z = (zlo / step).ceil() * step;
        while z <= zhi + 1e-9 {
            sc.texts.push(SText { at: [(x0 + 0.5) * 1000.0, py(z)], text: format!("{z:.1}"), rgb: C_TEXT, height_mm: 0.0, px: 8.5, anchor: 1, prio: 2, halo: false });
            z += step;
        }
        for gl in &v.ground_lines {
            for run in &gl.runs {
                if run.len() >= 2 {
                    sc.lines.push(SLine::new(run.iter().map(|p| [px(p.0), py(p.1)]).collect(), C_GROUND, 1.6));
                }
            }
        }
        // Point cloud slices: the points within half the thickness of the sample line.
        for set in &d.slices {
            if let Some(l) = set.lines.iter().find(|l| l.name == v.line.name) {
                for p in &l.points {
                    sc.points.push(crate::civil_scene::PMark { at: [px(p[0]), py(p[1])], number: 0, z: None, desc: String::new() });
                }
            }
        }
        if let Some(dv) = &v.design {
            for l in &dv.geometry.links {
                if l.points.len() >= 2 {
                    sc.lines.push(SLine::new(l.points.iter().map(|p| [px(p.0), py(p.1)]).collect(), C_DESIGN, 1.4));
                }
            }
            for sh in &dv.geometry.shapes {
                if sh.polygon.len() >= 2 {
                    let mut pts: Vec<[f64; 2]> = sh.polygon.iter().map(|p| [px(p.0), py(p.1)]).collect();
                    pts.push(pts[0]);
                    sc.lines.push(SLine::new(pts, colour_of(&sh.code), 0.8));
                }
            }
        }
        // Pipe crossings: the outside of the barrel at the section, as an ellipse (the cut of a
        // round pipe by a plane square to the alignment is a circle; the view's exaggeration
        // stretches it), and a label.
        for sec in &pipe_secs[k] {
            for c in &sec.crossings {
                let (cz, rz, rx) = ((c.outer_bottom_m + c.outer_top_m) / 2.0, (c.outer_top_m - c.outer_bottom_m) / 2.0, (c.inside_width_m + (c.outer_top_m - c.outer_bottom_m - c.inside_height_m)) / 2.0);
                let ring: Vec<[f64; 2]> = (0..=24).map(|i| { let a = std::f64::consts::TAU * i as f64 / 24.0; [px(c.offset_m + rx * a.cos()), py(cz + rz * a.sin())] }).collect();
                sc.lines.push(SLine::new(ring, [0x4D, 0xA3, 0xFF], 1.6));
                sc.texts.push(SText { at: [px(c.offset_m), py(c.outer_top_m)], text: format!("{} {}", c.name, c.size_label), rgb: [0x4D, 0xA3, 0xFF], height_mm: 0.0, px: 8.5, anchor: 3, prio: 3, halo: true });
            }
            for st in &sec.structures {
                let (x, bottom) = (st.offset_m, st.sump_level_m.unwrap_or(st.rim_level_m - 1.0));
                let q = vec![[px(x - st.outer_radius_m), py(bottom)], [px(x + st.outer_radius_m), py(bottom)], [px(x + st.outer_radius_m), py(st.rim_level_m)], [px(x - st.outer_radius_m), py(st.rim_level_m)], [px(x - st.outer_radius_m), py(bottom)]];
                sc.lines.push(SLine::new(q, [0xE0, 0xB0, 0x40], 1.2));
            }
        }
    }
    sc.derived.push(("Sample lines".into(), views.len().to_string()));
    sc.derived.push(("Alignment".into(), d.group.alignment.clone()));
    sc.derived.push(("Views drawn in".into(), format!("{cols} column(s)")));
    Ok(Built { scene: sc, ..Default::default() })
}
