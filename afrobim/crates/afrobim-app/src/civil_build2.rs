//! Scenes of corridors and sample line groups (see `civil_build`).

use std::sync::Arc;

use afrobim_corridor::{build_model, corridor_surface, daylight_boundary, feature_lines, section_views, CorridorSampling, SurfaceBoundary, Tin};
use afrobim_kernel::Element;

use crate::civil::{CorridorData, SampleData};
use crate::civil_build::{mm, nice_step, Built, Ctx, C_DESIGN, C_GRID, C_GROUND, C_TEXT};
use crate::civil_scene::{SLine, SText, Scene};

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

pub fn build_samples(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let d: SampleData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The sample line data cannot be read: {x}"))?;
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
    let views = section_views(&d.group, &al, &refs, sampling).map_err(|x| format!("The sections cannot be cut: {x}"))?;
    let mut sc = Scene::default();
    // In plan: each sample line across the alignment, with its name.
    for v in &views {
        let (l, r) = (v.line.left_swath_m, v.line.right_swath_m);
        let (s, c) = v.heading_rad.sin_cos();
        let a = (v.x + l * s, v.y - l * c);
        let b = (v.x - r * s, v.y + r * c);
        sc.lines.push(SLine::new(vec![mm(a.0, a.1), mm(b.0, b.1)], [0xE0, 0xC0, 0x3B], 1.2));
        sc.texts.push(SText { at: mm(a.0, a.1), text: v.label.clone(), rgb: [0xE0, 0xC0, 0x3B], height_mm: 0.0, px: 9.5, anchor: 3 });
    }
    // The section views as an array in the model, from the insertion point.
    let cols = d.columns.max(1);
    let ex = if d.vertical_exaggeration.is_finite() && d.vertical_exaggeration > 0.0 { d.vertical_exaggeration } else { 1.0 };
    let swath_w = views.iter().map(|v| v.line.left_swath_m + v.line.right_swath_m).fold(0.0, f64::max);
    let (mut zlo, mut zhi) = (f64::INFINITY, f64::NEG_INFINITY);
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
        sc.texts.push(SText { at: [(x0 + 1.0) * 1000.0, (y0 + cell_h - 1.2) * 1000.0], text: v.label.clone(), rgb: C_TEXT, height_mm: 0.0, px: 11.0, anchor: 1 });
        let mut z = (zlo / step).ceil() * step;
        while z <= zhi + 1e-9 {
            sc.texts.push(SText { at: [(x0 + 0.5) * 1000.0, py(z)], text: format!("{z:.1}"), rgb: C_TEXT, height_mm: 0.0, px: 8.5, anchor: 1 });
            z += step;
        }
        for gl in &v.ground_lines {
            for run in &gl.runs {
                if run.len() >= 2 {
                    sc.lines.push(SLine::new(run.iter().map(|p| [px(p.0), py(p.1)]).collect(), C_GROUND, 1.6));
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
    }
    sc.derived.push(("Sample lines".into(), views.len().to_string()));
    sc.derived.push(("Alignment".into(), d.group.alignment.clone()));
    sc.derived.push(("Views drawn in".into(), format!("{cols} column(s)")));
    Ok(Built { scene: sc, ..Default::default() })
}
