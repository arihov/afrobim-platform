// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Builds the scene of a civil element from its data. Pure functions of cloned elements, so the
//! work can run on a worker thread (`civil_cache`).

use std::collections::HashMap;
use std::sync::Arc;

use afrobim_alignment::Alignment;
use afrobim_corridor::Tin;
use afrobim_kernel::Element;

use crate::civil::{self, *};
use crate::civil_scene::*;

/// Colours of the drawn objects (not from any standard: a legible set for the canvas).
pub const C_GROUND: [u8; 3] = [0x4F, 0xA0, 0x62];
pub const C_DESIGN: [u8; 3] = [0xD9, 0x55, 0x4A];
pub const C_GRID: [u8; 3] = [0x88, 0x90, 0x9A];
pub const C_CONTOUR: [u8; 3] = [0xB0, 0x82, 0x4E];
pub const C_BORDER: [u8; 3] = [0x5C, 0x8F, 0xD6];
pub const C_PARCEL: [u8; 3] = [0xC9, 0x9A, 0x2E];
pub const C_TEXT: [u8; 3] = [0xA8, 0xB0, 0xBA];

/// The flatness used when an arc of a parcel is drawn. A display setting only: it is not a survey
/// tolerance and no figure is computed with it.
pub const DISPLAY_FLATNESS_M: f64 = 0.005;

/// The most triangle edges drawn as a wireframe.
const EDGE_CAP: usize = 60_000;
/// The most triangles in a 3D mesh before the display mesh is thinned (see `surface_display_mesh`).
pub const MESH_CAP: usize = 40_000;

/// What a build reads: every element (cloned) and the finished TINs of surfaces by element id.
pub struct Ctx {
    pub elements: Vec<Element>,
    pub tins: HashMap<u64, Arc<Tin>>,
}

impl Ctx {
    pub fn element(&self, id: u64) -> Option<&Element> {
        self.elements.iter().find(|e| e.id.0 == id)
    }
    pub fn tin(&self, id: u64) -> Option<Arc<Tin>> {
        if let Some(t) = self.tins.get(&id) {
            return Some(t.clone());
        }
        // An imported TIN surface holds its triangles in its data.
        let e = self.element(id)?;
        if e.category == afrobim_corridor::SURFACE_CATEGORY {
            return serde_json::from_value::<afrobim_corridor::SurfaceDefinition>(e.data.clone()).ok().map(|d| Arc::new(d.tin));
        }
        None
    }
    pub fn alignment_named(&self, name: &str) -> Option<Alignment> {
        self.elements.iter().filter(|e| e.category == afrobim_alignment::CATEGORY).filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok()).find(|a| a.name == name)
    }
    pub fn alignments(&self) -> Vec<Alignment> {
        self.elements.iter().filter(|e| e.category == afrobim_alignment::CATEGORY).filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok()).collect()
    }
}

/// The result of building one element.
#[derive(Default, Clone)]
pub struct Built {
    pub scene: Scene,
    /// The surface's TIN (a civil surface), shared with those that sample it.
    pub tin: Option<Arc<Tin>>,
    /// Surfaces that belong to the element (a corridor's top and datum), shown under Surfaces.
    pub extra: Vec<(String, Arc<Tin>)>,
    /// The build report, one line each.
    pub report: Vec<String>,
}

pub fn mm(e: f64, n: f64) -> [f64; 2] {
    [e * 1000.0, n * 1000.0]
}

pub fn build(e: &Element, ctx: &Ctx) -> Built {
    let result = match e.category.as_str() {
        POINTS => build_points(e),
        SURFACE => build_surface(e),
        PARCEL => build_parcel(e, ctx),
        PROFILE => build_profile(e, ctx),
        PROFILE_VIEW => build_profile_view(e, ctx),
        ASSEMBLY => build_assembly(e),
        CORRIDOR => crate::civil_build2::build_corridor(e, ctx),
        SAMPLES => crate::civil_build2::build_samples(e, ctx),
        PIPENET => crate::civil_pipes::build_pipenet(e, ctx),
        FEATURELINE => crate::civil_grading::build_featureline(e),
        GRADING => crate::civil_grading::build_grading(e, ctx),
        INTERSECTION => crate::civil_junction::build_intersection_scene(e, ctx),
        ROUNDABOUT => crate::civil_junction::build_roundabout_scene(e),
        FIGURES => crate::survey_db::build_figures_scene(e),
        POINTCLOUD => crate::cloud_ui::build_cloud_scene(e),
        PIPEPROFILE => crate::civil_pipes::build_pipe_profile(e, ctx),
        _ => Ok(Built::default()),
    };
    result.unwrap_or_else(|msg| {
        let mut b = Built::default();
        b.report.push(msg.clone());
        b.scene.notes.push(msg);
        b
    })
}

fn build_points(e: &Element) -> Result<Built, String> {
    let d: PointsData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The point data cannot be read: {x}"))?;
    let mut scene = Scene { label_number: d.show_numbers, label_elevation: d.show_elevations, label_description: d.show_descriptions, decimals: d.decimals as usize, ..Default::default() };
    scene.points = d
        .store
        .iter()
        .map(|p| PMark {
            at: mm(p.easting, p.northing),
            number: p.number,
            z: p.elevation,
            desc: if p.full_description.is_empty() { p.raw_description.clone() } else { p.full_description.clone() },
        })
        .collect();
    if let Some(l) = e.properties.get(crate::survey_db::LINK_PROP).and_then(|t| serde_json::from_str::<crate::survey_db::Link>(t).ok()) {
        scene.derived.push(("Survey network".into(), l.network.clone()));
        scene.derived.push(("Survey database".into(), l.db_name.clone()));
        scene.derived.push(("Last update".into(), format!("coordinates as of hash {} (no time is kept)", l.hash.chars().take(8).collect::<String>())));
    }
    scene.derived.push(("Points".into(), d.store.len().to_string()));
    scene.derived.push(("Groups".into(), d.groups.groups().len().to_string()));
    let with_z = d.store.iter().filter(|p| p.elevation.is_some()).count();
    scene.derived.push(("With elevation".into(), with_z.to_string()));
    Ok(Built { scene, ..Default::default() })
}

/// Builds the TIN of a civil surface and reports what the build did.
pub fn build_tin(d: &SurfaceData) -> Result<(Tin, Vec<String>), String> {
    let built = d.surface.build().map_err(|x| format!("The surface cannot be built: {x}"))?;
    let r = &built.report;
    let mut lines = vec![
        format!("Points in {}, used {}, merged {} (largest elevation difference {:.3} m), excluded {}, deleted {}, unused {}.", r.points_input, r.points_used, r.duplicates_merged, r.duplicate_max_dz, r.points_excluded, r.points_deleted, r.points_unused),
        format!("Triangles formed {}, removed by longest edge {}, removed by boundary {}, restored by show {}, deleted {}, edges swapped {}, zero area {}; {} remain.", r.triangles_formed, r.triangles_removed_max_edge, r.triangles_removed_boundary, r.triangles_restored_by_show, r.triangles_deleted, r.edges_swapped, r.triangles_degenerate, r.triangles),
        format!("Breakline crossings found: {}.", r.crossings.len()),
    ];
    lines.extend(r.warnings.iter().map(|w| format!("Warning: {w}")));
    Ok((built.tin, lines))
}

fn ramp(t: f64) -> [u8; 3] {
    // Blue (low) through green to brown (high): a display ramp, not a standard.
    let t = t.clamp(0.0, 1.0);
    let (a, b, f) = if t < 0.5 { ([0x3B, 0x6E, 0xB5], [0x5B, 0xA8, 0x6B], t * 2.0) } else { ([0x5B, 0xA8, 0x6B], [0xA8, 0x74, 0x4A], (t - 0.5) * 2.0) };
    [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * f) as u8)
}

fn build_surface(e: &Element) -> Result<Built, String> {
    let d: SurfaceData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The surface data cannot be read: {x}"))?;
    let (tin, report) = build_tin(&d)?;
    let tin = Arc::new(tin);
    let scene = surface_scene(&tin, &d.style, (e.origin.x / 1000.0, e.origin.y / 1000.0));
    Ok(Built { scene, tin: Some(tin), extra: vec![], report })
}

/// The scene of a TIN in a style. Also used for imported TIN surfaces and corridor surfaces.
pub fn surface_scene(tin: &Tin, style: &SurfaceStyle, origin_m: (f64, f64)) -> Scene {
    let mut s = Scene::default();
    let mut colour_by: Option<ColourBy> = None;
    let local = |p: &[f64; 3]| [(p[0] - origin_m.0) * 1000.0, (p[1] - origin_m.1) * 1000.0, p[2] * 1000.0];
    let mut points3 = 0usize;
    let b = tin.bounds();
    let ext = afrobim_terrain::extremes(tin);
    let a = afrobim_terrain::areas(tin);
    s.derived.push(("Points".into(), tin.points().len().to_string()));
    s.derived.push(("Triangles".into(), tin.triangles().len().to_string()));
    s.derived.push(("Plan area (m2)".into(), format!("{:.2}", a.plan)));
    s.derived.push(("Surface area (m2)".into(), format!("{:.2}", a.surface)));
    s.derived.push(("Lowest (m)".into(), format!("{:.3}", ext.lowest[2])));
    s.derived.push(("Highest (m)".into(), format!("{:.3}", ext.highest[2])));
    s.derived.push(("Extent E (m)".into(), format!("{:.3} to {:.3}", b.min_x, b.max_x)));
    s.derived.push(("Extent N (m)".into(), format!("{:.3} to {:.3}", b.min_y, b.max_y)));
    if let Some(step) = style.elevation_band_step {
        match afrobim_terrain::regular_breaks(ext.lowest[2], ext.highest[2], step).and_then(|br| afrobim_terrain::elevation_bands(tin, &br)) {
            Ok(bands) => {
                let n = bands.len().max(1) as f64;
                if let Ok(br) = afrobim_terrain::regular_breaks(ext.lowest[2], ext.highest[2], step) {
                    colour_by = Some(ColourBy::Elevation(br));
                }
                for (i, band) in bands.iter().enumerate() {
                    let c = ramp(i as f64 / (n - 1.0).max(1.0));
                    for poly in &band.polygons {
                        s.fills.push(SFill { poly: poly.iter().map(|p| mm(p[0], p[1])).collect(), rgba: [c[0], c[1], c[2], 150] });
                    }
                }
            }
            Err(x) => s.notes.push(format!("Elevation bands: {x}")),
        }
    }
    if let Some(step) = style.slope_band_step_percent {
        let top = afrobim_terrain::triangle_slopes(tin).iter().map(|t| t.slope_percent).fold(0.0, f64::max);
        match afrobim_terrain::regular_breaks(0.0, top, step).and_then(|br| afrobim_terrain::slope_bands(tin, &br, afrobim_terrain::SlopeUnit::Percent)) {
            Ok(bands) => {
                let n = bands.len().max(1) as f64;
                if colour_by.is_none() {
                    if let Ok(br) = afrobim_terrain::regular_breaks(0.0, top, step) {
                        colour_by = Some(ColourBy::Slope(br));
                    }
                }
                for (i, band) in bands.iter().enumerate() {
                    let c = ramp(1.0 - i as f64 / (n - 1.0).max(1.0));
                    for poly in &band.polygons {
                        s.fills.push(SFill { poly: poly.iter().map(|p| mm(p[0], p[1])).collect(), rgba: [c[0], c[1], c[2], 150] });
                    }
                }
            }
            Err(x) => s.notes.push(format!("Slope bands: {x}")),
        }
    }
    if style.triangles {
        if tin.triangles().len() * 3 > EDGE_CAP {
            s.notes.push(format!("The triangle display is limited to {EDGE_CAP} edges; this surface has more, so they are not drawn."));
        } else {
            for [p, q] in afrobim_terrain::display::wireframe(tin) {
                s.lines.push(SLine::new(vec![mm(p[0], p[1]), mm(q[0], q[1])], C_GRID, 0.6));
            }
        }
    }
    if let Some(minor) = style.minor_interval {
        match afrobim_terrain::contours(tin, &afrobim_terrain::ContourOptions::new(minor, style.major_interval)) {
            Ok(set) => {
                let lines = afrobim_terrain::display::contour_lines(&set);
                s.derived.push(("Contours".into(), lines.len().to_string()));
                for c in lines {
                    let pts: Vec<[f64; 2]> = c.line.points.iter().map(|p| mm(p[0], p[1])).collect();
                    if pts.len() < 2 {
                        continue;
                    }
                    // The same contour in 3D, at its own level.
                    if points3 < LINES3_CAP {
                        let mut l3: Vec<[f64; 3]> = c.line.points.iter().map(|p| local(&[p[0], p[1], c.level])).collect();
                        if c.line.closed {
                            l3.push(l3[0]);
                        }
                        points3 += l3.len();
                        s.lines3.push(L3 { pts: l3, rgb: C_CONTOUR, width: if c.major { 1.6 } else { 0.8 } });
                    }
                    let mut pts = pts;
                    if c.line.closed {
                        pts.push(pts[0]);
                    }
                    if c.major {
                        let mid = pts[pts.len() / 2];
                        s.texts.push(SText { at: mid, text: format!("{:.1}", c.level), rgb: C_CONTOUR, height_mm: 0.0, px: 9.5, anchor: 0, prio: 1, halo: false });
                    }
                    s.lines.push(SLine::new(pts, C_CONTOUR, if c.major { 1.6 } else { 0.8 }));
                }
            }
            Err(x) => s.notes.push(format!("Contours: {x}")),
        }
    }
    if let (Some(len), Some(min)) = (style.arrow_length, style.arrow_min_slope_percent) {
        match afrobim_terrain::slope_arrows(tin, len, min) {
            Ok(arrows) => {
                for ar in arrows {
                    let (t, h) = (ar.tail, ar.head);
                    s.lines.push(SLine::new(vec![mm(t[0], t[1]), mm(h[0], h[1])], C_DESIGN, 1.0));
                    let (dx, dy) = (h[0] - t[0], h[1] - t[1]);
                    let l = dx.hypot(dy).max(1e-9);
                    let (ux, uy) = (dx / l, dy / l);
                    let k = l * 0.25;
                    for sgn in [-1.0, 1.0] {
                        let (px, py) = (h[0] - k * (ux + sgn * 0.5 * uy), h[1] - k * (uy - sgn * 0.5 * ux));
                        s.lines.push(SLine::new(vec![mm(h[0], h[1]), mm(px, py)], C_DESIGN, 1.0));
                    }
                }
            }
            Err(x) => s.notes.push(format!("Slope arrows: {x}")),
        }
    }
    if style.border {
        for pl in afrobim_terrain::display::outline(tin) {
            let mut pts: Vec<[f64; 2]> = pl.points.iter().map(|p| mm(p[0], p[1])).collect();
            if pl.closed && !pts.is_empty() {
                pts.push(pts[0]);
            }
            // Draped on the surface in 3D (a point of the outline is on the TIN's edge).
            let mut l3: Vec<[f64; 3]> = pts.iter().filter_map(|p| tin.elevation_at(p[0] / 1000.0, p[1] / 1000.0).map(|z| local(&[p[0] / 1000.0, p[1] / 1000.0, z]))).collect();
            if l3.len() >= 2 && points3 < LINES3_CAP {
                points3 += l3.len();
                l3.shrink_to_fit();
                s.lines3.push(L3 { pts: l3, rgb: C_BORDER, width: 1.4 });
            }
            s.lines.push(SLine::new(pts, C_BORDER, 1.4));
        }
    }
    // The 3D mesh: element-local millimetres (see `afrobim_corridor::surface_display_mesh`).
    let (mesh, thinned) = crate::civil_build2::mesh_local(tin, origin_m, MESH_CAP);
    if thinned {
        s.notes.push("The 3D view shows a thinned copy of this surface (the 2D view and every calculation use the full surface).".into());
    }
    match colour_by {
        Some(by) => s.meshes.extend(split_mesh_by(mesh, &by)),
        None => s.meshes.push(mesh),
    }
    s
}

/// How many points of 3D linework one surface carries (the 2D view has them all).
const LINES3_CAP: usize = 150_000;

/// What the 3D mesh of a surface is coloured by, taken from its style.
enum ColourBy {
    /// Elevation bands with these breaks (metres).
    Elevation(Vec<f64>),
    /// Slope bands with these breaks (percent).
    Slope(Vec<f64>),
}

/// The mesh cut into one mesh per band, each in the colour the 2D view gives that band.
fn split_mesh_by(mesh: afrobim_draw::Mesh3, by: &ColourBy) -> Vec<afrobim_draw::Mesh3> {
    let (breaks, slope) = match by {
        ColourBy::Elevation(b) => (b, false),
        ColourBy::Slope(b) => (b, true),
    };
    let n = breaks.len() + 1;
    let mut parts: Vec<(Vec<[f64; 3]>, Vec<[u32; 3]>, std::collections::HashMap<u32, u32>)> = (0..n).map(|_| (vec![], vec![], Default::default())).collect();
    for t in &mesh.triangles {
        let v = t.map(|i| mesh.vertices[i as usize]);
        // Vertices are in millimetres.
        let value = if slope {
            let (ux, uy, uz) = (v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]);
            let (wx, wy, wz) = (v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]);
            let (nx, ny, nz) = (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
            if nz.abs() < 1e-12 { f64::MAX } else { 100.0 * nx.hypot(ny) / nz.abs() }
        } else {
            (v[0][2] + v[1][2] + v[2][2]) / 3000.0
        };
        let band = breaks.partition_point(|b| *b <= value);
        let (verts, tris, map) = &mut parts[band];
        let mut tri = [0u32; 3];
        for (k, corner) in t.iter().enumerate() {
            tri[k] = *map.entry(*corner).or_insert_with(|| {
                verts.push(mesh.vertices[*corner as usize]);
                verts.len() as u32 - 1
            });
        }
        tris.push(tri);
    }
    parts
        .into_iter()
        .enumerate()
        .filter(|(_, p)| !p.1.is_empty())
        .map(|(i, (vertices, triangles, _))| {
            let f = i as f64 / (n as f64 - 1.0).max(1.0);
            afrobim_draw::Mesh3 { name: mesh.name.clone(), vertices, triangles, colour: ramp(if slope { 1.0 - f } else { f }) }
        })
        .collect()
}

fn build_parcel(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let p: afrobim_cogo::Parcel = serde_json::from_value(e.data.clone()).map_err(|x| format!("The parcel data cannot be read: {x}"))?;
    let mut s = Scene::default();
    let ring = p.outline(DISPLAY_FLATNESS_M).map_err(|x| format!("The parcel outline cannot be drawn: {x}"))?;
    let mut pts: Vec<[f64; 2]> = ring.iter().map(|q| mm(q.e, q.n)).collect();
    if let Some(f) = pts.first().copied() {
        pts.push(f);
    }
    let colour = match p.status {
        afrobim_cogo::ParcelStatus::ToBeAcquired => C_DESIGN,
        afrobim_cogo::ParcelStatus::Acquired => [0x8E, 0x7C, 0xC3],
        _ => C_PARCEL,
    };
    // The outline in 3D: draped on the first civil surface, or at level zero where there is none
    // (or outside it). Long sides are cut into pieces so that the line follows the ground.
    let ground = ctx.elements.iter().filter(|x| x.category == SURFACE).find_map(|x| ctx.tins.get(&x.id.0));
    let origin_m = (e.origin.x / 1000.0, e.origin.y / 1000.0);
    let mut l3: Vec<[f64; 3]> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len_m = (b[0] - a[0]).hypot(b[1] - a[1]) / 1000.0;
        let k = ((len_m / 5.0).ceil() as usize).clamp(1, 400);
        for i in 0..k {
            let t = i as f64 / k as f64;
            let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            let z = ground.and_then(|g| g.elevation_at(x / 1000.0, y / 1000.0)).unwrap_or(0.0);
            l3.push([x - origin_m.0 * 1000.0, y - origin_m.1 * 1000.0, z * 1000.0]);
        }
    }
    if let Some(last) = pts.last() {
        let z = ground.and_then(|g| g.elevation_at(last[0] / 1000.0, last[1] / 1000.0)).unwrap_or(0.0);
        l3.push([last[0] - origin_m.0 * 1000.0, last[1] - origin_m.1 * 1000.0, z * 1000.0]);
    }
    if l3.len() >= 2 {
        s.lines3.push(L3 { pts: l3, rgb: colour, width: 2.0 });
    }
    s.lines.push(SLine::new(pts, colour, 1.6));
    if let Ok(l) = p.label(0, DISPLAY_FLATNESS_M) {
        for (i, t) in l.lines.iter().enumerate() {
            s.texts.push(SText { at: [l.at.e * 1000.0, l.at.n * 1000.0], text: t.clone(), rgb: colour, height_mm: 0.0, px: if i == 0 { 12.0 } else { 10.5 }, anchor: if i == 0 { 3 } else { 4 }, prio: if i == 0 { 6 } else { 5 }, halo: true });
        }
    }
    if let (Ok(a), Ok(per)) = (p.area(), p.perimeter()) {
        s.derived.push(("Area (m2)".into(), format!("{a:.2}")));
        s.derived.push(("Area (ha)".into(), format!("{:.4}", a / afrobim_cogo::HECTARE_M2)));
        s.derived.push(("Perimeter (m)".into(), format!("{per:.2}")));
    }
    s.derived.push(("Owner".into(), p.owner.clone()));
    s.derived.push(("Land reference".into(), p.land_reference.clone()));
    s.derived.push(("Status".into(), p.status.label()));
    Ok(Built { scene: s, ..Default::default() })
}

fn build_assembly(e: &Element) -> Result<Built, String> {
    let a: afrobim_corridor::Assembly = serde_json::from_value(e.data.clone()).map_err(|x| format!("The assembly data cannot be read: {x}"))?;
    let mut s = Scene::default();
    s.derived.push(("Left parts".into(), a.left.len().to_string()));
    s.derived.push(("Right parts".into(), a.right.len().to_string()));
    if let Err(x) = a.check() {
        s.notes.push(format!("The assembly does not pass its check: {x}"));
    }
    Ok(Built { scene: s, ..Default::default() })
}

/// The ground levels along an alignment: station and level (None off the surface).
pub fn sample_ground(tin: &Tin, alignment: &Alignment, interval_m: f64) -> Vec<(f64, Option<f64>)> {
    let (s0, s1) = (alignment.start_station_m, alignment.end_station_m());
    let step = if interval_m.is_finite() && interval_m > 0.0 { interval_m } else { (s1 - s0).max(1.0) };
    let n = (((s1 - s0) / step).ceil() as usize).clamp(1, 20_000);
    let mut out = Vec::new();
    for i in 0..=n {
        let s = (s0 + step * i as f64).min(s1);
        out.push((s, alignment.point_at(s).and_then(|p| tin.elevation_at(p.x, p.y))));
        if s >= s1 {
            break;
        }
    }
    out
}

/// The sampled or imported ground profile of a profile element: station and level.
pub fn profile_points(d: &ProfileData, ctx: &Ctx) -> Result<Vec<(f64, Option<f64>)>, String> {
    if !d.imported.is_empty() {
        return Ok(d.imported.iter().map(|p| (p[0], Some(p[1]))).collect());
    }
    let tin = ctx.tin(d.surface).ok_or("The surface of this ground profile is missing.")?;
    let al = ctx.alignment_named(&d.alignment).ok_or_else(|| format!("The alignment '{}' of this ground profile is missing.", d.alignment))?;
    Ok(sample_ground(&tin, &al, d.interval_m))
}

fn build_profile(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let d: ProfileData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The profile data cannot be read: {x}"))?;
    let pts = profile_points(&d, ctx)?;
    let mut s = Scene::default();
    let zs: Vec<f64> = pts.iter().filter_map(|p| p.1).collect();
    s.derived.push(("Alignment".into(), d.alignment.clone()));
    s.derived.push(("Samples".into(), pts.len().to_string()));
    if let (Some(lo), Some(hi)) = (zs.iter().cloned().reduce(f64::min), zs.iter().cloned().reduce(f64::max)) {
        s.derived.push(("Lowest (m)".into(), format!("{lo:.3}")));
        s.derived.push(("Highest (m)".into(), format!("{hi:.3}")));
    }
    if !d.imported.is_empty() && d.surface != 0 {
        if let (Some(tin), Some(al)) = (ctx.tin(d.surface), ctx.alignment_named(&d.alignment)) {
            let c = crate::civil_sections::compare_profile(&al, &d.imported, &tin);
            let name = ctx.element(d.surface).map(|e| e.name.clone()).unwrap_or_default();
            s.derived.push(("Against AfroBIM's own profile".into(), format!("largest difference {:.1} mm at station {:.3} ({} stations compared, {} off the surface {name})", c.max_diff_m * 1000.0, c.at_station_m, c.compared, c.off_surface)));
        }
    }
    let off = pts.iter().filter(|p| p.1.is_none()).count();
    if off > 0 {
        s.notes.push(format!("{off} sample station(s) are off the surface and have no level."));
    }
    Ok(Built { scene: s, ..Default::default() })
}

/// A round step near `target` from 1, 2, 5 times a power of ten.
pub fn nice_step(target: f64) -> f64 {
    if !(target.is_finite() && target > 0.0) {
        return 1.0;
    }
    let p = 10f64.powf(target.log10().floor());
    for k in [1.0, 2.0, 5.0, 10.0] {
        if k * p >= target {
            return k * p;
        }
    }
    10.0 * p
}

fn build_profile_view(e: &Element, ctx: &Ctx) -> Result<Built, String> {
    let d: ProfileViewData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The profile view data cannot be read: {x}"))?;
    let al = ctx.alignment_named(&d.alignment).ok_or_else(|| format!("The alignment '{}' of this profile view is missing.", d.alignment))?;
    if !(d.vertical_exaggeration.is_finite() && d.vertical_exaggeration > 0.0) {
        return Err("The vertical exaggeration of this profile view must be greater than 0.".into());
    }
    let (s0, s1) = (al.start_station_m, al.end_station_m());
    let mut grounds: Vec<(String, Vec<(f64, Option<f64>)>)> = Vec::new();
    for id in &d.profiles {
        if let Some(pe) = ctx.element(*id) {
            if let Ok(pd) = serde_json::from_value::<ProfileData>(pe.data.clone()) {
                if let Ok(p) = profile_points(&pd, ctx) {
                    grounds.push((pe.name.clone(), p));
                }
            }
        }
    }
    let design = |s: f64| al.profile.as_ref().and_then(|p| p.elevation_at(s).ok());
    let mut zs: Vec<f64> = grounds.iter().flat_map(|g| g.1.iter().filter_map(|p| p.1)).collect();
    zs.extend(d.cloud.iter().flat_map(|c| c.points.iter().map(|p| p[1])));
    if d.show_design {
        let n = 200;
        zs.extend((0..=n).filter_map(|i| design(s0 + (s1 - s0) * i as f64 / n as f64)));
    }
    if zs.is_empty() {
        return Err("This profile view has no levels to draw: add a ground profile, or give the alignment a design profile.".into());
    }
    let (zlo, zhi) = (zs.iter().cloned().fold(f64::INFINITY, f64::min), zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let zstep = nice_step((zhi - zlo).max(1e-3) / 6.0);
    let datum = d.datum.unwrap_or_else(|| (zlo / zstep).floor() * zstep);
    let ztop = ((zhi - datum) / zstep).ceil().max(1.0) * zstep + datum;
    let ex = d.vertical_exaggeration;
    let (ox, oy) = (d.insertion[0], d.insertion[1]);
    // Model millimetres: along the station 1 m is 1 m; up, 1 m of level is `ex` metres.
    let px = |s: f64| (ox + (s - s0)) * 1000.0;
    let py = |z: f64| (oy + (z - datum) * ex) * 1000.0;
    let mut sc = Scene::default();
    let (x0, x1, y0, y1) = (px(s0), px(s1), py(datum), py(ztop));
    sc.lines.push(SLine::new(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]], C_GRID, 1.4));
    // Text 2.5 m high at the drawing's true horizontal scale (5 mm at 1:500); it shows when it would be 4 px or more on screen.
    let text_h = 2500.0;
    let mut z = datum;
    while z <= ztop + 1e-9 {
        sc.lines.push(SLine::new(vec![[x0, py(z)], [x1, py(z)]], C_GRID, 0.5));
        sc.texts.push(SText { at: [x0 - text_h * 0.3, py(z)], text: format!("{z:.2}"), rgb: C_TEXT, height_mm: text_h, px: 0.0, anchor: 2, prio: 4, halo: false });
        z += zstep;
    }
    let sstep = nice_step((s1 - s0) / 8.0);
    let mut stations = Vec::new();
    let mut s = (s0 / sstep).ceil() * sstep;
    while s <= s1 + 1e-9 {
        stations.push(s);
        sc.lines.push(SLine::new(vec![[px(s), y0], [px(s), y1]], C_GRID, 0.5));
        s += sstep;
    }
    for c in &d.cloud {
        for p in &c.points {
            sc.points.push(PMark { at: [px(p[0]), py(p[1])], number: 0, z: None, desc: String::new() });
        }
    }
    for g in &grounds {
        let mut run: Vec<[f64; 2]> = Vec::new();
        for (s, z) in &g.1 {
            match z {
                Some(z) => run.push([px(*s), py(*z)]),
                None => {
                    if run.len() > 1 {
                        sc.lines.push(SLine::new(std::mem::take(&mut run), C_GROUND, 1.8));
                    }
                    run.clear();
                }
            }
        }
        if run.len() > 1 {
            sc.lines.push(SLine::new(run, C_GROUND, 1.8));
        }
    }
    if d.show_design {
        let n = 400;
        let line: Vec<[f64; 2]> = (0..=n).filter_map(|i| {
            let s = s0 + (s1 - s0) * i as f64 / n as f64;
            design(s).map(|z| [px(s), py(z)])
        }).collect();
        if line.len() > 1 {
            sc.lines.push(SLine::new(line, C_DESIGN, 1.8));
        }
    }
    if d.show_bands {
        // Rows below the datum line: station, ground level, design level, depth (design minus ground:
        // positive is fill, negative is cut).
        let row = d.band_height_m.max(0.1) * 1000.0;
        let labels = ["Station", "Ground level", "Design level", "Fill (+) / cut (-)"];
        for (i, l) in labels.iter().enumerate() {
            let yb = y0 - row * (i + 1) as f64;
            sc.lines.push(SLine::new(vec![[x0, yb], [x1, yb]], C_GRID, 0.5));
            sc.texts.push(SText { at: [x0 - row * 0.2, yb + row / 2.0], text: l.to_string(), rgb: C_TEXT, height_mm: row * 0.6, px: 0.0, anchor: 2, prio: 4, halo: false });
        }
        let ground_at = |s: f64| grounds.first().and_then(|g| nearest(&g.1, s));
        for s in &stations {
            let cells = [
                format!("{s:.1}"),
                ground_at(*s).map(|z| format!("{z:.2}")).unwrap_or_default(),
                design(*s).map(|z| format!("{z:.2}")).unwrap_or_default(),
                match (ground_at(*s), design(*s)) {
                    (Some(g), Some(dz)) => format!("{:+.2}", dz - g),
                    _ => String::new(),
                },
            ];
            for (i, t) in cells.iter().enumerate() {
                let yb = y0 - row * (i + 1) as f64;
                sc.texts.push(SText { at: [px(*s), yb + row / 2.0], text: t.clone(), rgb: C_TEXT, height_mm: row * 0.6, px: 0.0, anchor: 0, prio: 4, halo: false });
            }
        }
        sc.lines.push(SLine::new(vec![[x0, y0 - row * 4.0], [x1, y0 - row * 4.0]], C_GRID, 1.0));
    }
    sc.texts.push(SText { at: [x0, y1 + (zstep * ex * 1000.0) * 0.4], text: format!("{}  datum {datum:.2} m, vertical exaggeration {ex}:1", d.alignment), rgb: C_TEXT, height_mm: text_h * 1.2, px: 0.0, anchor: 1, prio: 5, halo: false });
    sc.derived.push(("Alignment".into(), d.alignment.clone()));
    sc.derived.push(("Datum (m)".into(), format!("{datum:.3}")));
    sc.derived.push(("Vertical exaggeration".into(), format!("{ex}")));
    Ok(Built { scene: sc, ..Default::default() })
}

/// The level at the sample nearest a station.
fn nearest(g: &[(f64, Option<f64>)], s: f64) -> Option<f64> {
    g.iter().min_by(|a, b| (a.0 - s).abs().total_cmp(&(b.0 - s).abs())).and_then(|p| p.1)
}

/// The bounds of an element's scene in metres, for setting the element's size.
pub fn scene_bounds(s: &Scene) -> Option<civil::Bounds> {
    civil::Bounds::of(s.lines.iter().flat_map(|l| l.pts.iter()).map(|p| (p[0] / 1000.0, p[1] / 1000.0)).chain(s.points.iter().map(|p| (p.at[0] / 1000.0, p.at[1] / 1000.0))))
}
