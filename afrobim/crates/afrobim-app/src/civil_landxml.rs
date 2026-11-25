// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! LandXML to civil objects: CgPoints become point sets, Parcels become parcel elements, ProfSurf
//! becomes ground profiles shown in profile views, and a surface keeps its source data (breaklines,
//! boundaries, contours, data points) as a definition that rebuilds. Everything comes in at the
//! coordinates of the file.

use afrobim_alignment::{LandXmlCgPointGroup, LandXmlFile, LandXmlGeomElement, LandXmlParcel, LandXmlProfSurf, LandXmlSurface};
use afrobim_cogo::{Boundary, CogoPoint, Parcel, ParcelStatus, Segment, Xy};
use afrobim_kernel::{Document, ElementId};
use afrobim_terrain::{Breakline, BreaklineKind, BoundaryKind, Operation, Surface, SurfaceDefinition, SurfacePoint, SurfaceRole};

use crate::civil::{self, Bounds, PointsData, ProfileData, SurfaceData, SurfaceStyle, PARCEL, POINTS, PROFILE, SURFACE};

/// The point groups of a file that hold points, flattened with their path as a name.
pub fn point_groups(file: &LandXmlFile) -> Vec<(String, &LandXmlCgPointGroup)> {
    fn walk<'a>(g: &'a LandXmlCgPointGroup, path: String, out: &mut Vec<(String, &'a LandXmlCgPointGroup)>) {
        if !g.points.is_empty() {
            out.push((path.clone(), g));
        }
        for c in &g.groups {
            let name = c.name.clone().unwrap_or_else(|| "Group".into());
            walk(c, format!("{path} / {name}"), out);
        }
    }
    let mut out = Vec::new();
    for (i, g) in file.cg_points.iter().enumerate() {
        walk(g, g.name.clone().unwrap_or_else(|| format!("CgPoints {}", i + 1)), &mut out);
    }
    out
}

pub fn add_points(doc: &mut Document, name: &str, g: &LandXmlCgPointGroup, source: &str) -> (ElementId, usize) {
    let mut data = PointsData::default();
    let mut next = 1u32;
    for p in &g.points {
        let wanted = p.name.trim().parse::<u32>().ok();
        let number = match wanted.filter(|n| data.store.get(*n).is_none()) {
            Some(n) => n,
            None => {
                next = data.store.next_free(next);
                next
            }
        };
        let desc = p.desc.clone().or_else(|| p.code.clone()).unwrap_or_default();
        let mut cp = CogoPoint::new(number, p.easting_m, p.northing_m, p.elevation_m, &desc);
        if wanted.is_none() {
            cp.name = Some(p.name.clone());
        }
        cp.source.file = Some(source.to_string());
        let _ = data.store.add(cp);
    }
    let n = data.store.len();
    let b = Bounds::of(data.store.iter().map(|p| (p.easting, p.northing)));
    let v = serde_json::to_value(&data).unwrap_or_default();
    (civil::add(doc, POINTS, name, v, b, source), n)
}

/// The parcel of a LandXML parcel: lines and curves of its first CoordGeom as a boundary.
pub fn parcel_from(p: &LandXmlParcel, number: u32) -> Result<Parcel, String> {
    let geom = p.coord_geoms.first().ok_or("The parcel has no coordinate geometry.")?;
    let mut start: Option<Xy> = None;
    let mut segs: Vec<Segment> = Vec::new();
    for el in &geom.elements {
        match el {
            LandXmlGeomElement::Line { start: s, end } => {
                start.get_or_insert(Xy::new(s.x, s.y));
                segs.push(Segment::Line { to: Xy::new(end.x, end.y) });
            }
            LandXmlGeomElement::Curve { start: s, center, end, clockwise, radius_m, .. } => {
                start.get_or_insert(Xy::new(s.x, s.y));
                let r = radius_m.unwrap_or_else(|| (s.x - center.x).hypot(s.y - center.y));
                let (a0, a1) = ((s.y - center.y).atan2(s.x - center.x), (end.y - center.y).atan2(end.x - center.x));
                let mut sweep = if *clockwise { a0 - a1 } else { a1 - a0 };
                while sweep < 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                segs.push(Segment::Arc { to: Xy::new(end.x, end.y), radius: r, turn: if *clockwise { afrobim_cogo::Turn::Right } else { afrobim_cogo::Turn::Left }, large: sweep > std::f64::consts::PI });
            }
            LandXmlGeomElement::Spiral { start: s, end, .. } => {
                start.get_or_insert(Xy::new(s.x, s.y));
                // A spiral is carried as its chord: noted by the caller.
                segs.push(Segment::Line { to: Xy::new(end.x, end.y) });
            }
            _ => {}
        }
    }
    let start = start.ok_or("The parcel has no boundary elements.")?;
    // A boundary that repeats its start at the end is closed by the engine: drop the repeat.
    if segs.last().is_some_and(|s| (s.to().e - start.e).abs() < 1e-9 && (s.to().n - start.n).abs() < 1e-9) {
        segs.pop();
    }
    let mut parcel = Parcel::new(&p.name, number, Boundary { start, segments: segs });
    parcel.owner = p.owner.clone().unwrap_or_default();
    parcel.land_use = p.parcel_type.clone().or_else(|| p.class.clone()).unwrap_or_default();
    parcel.status = ParcelStatus::Existing;
    if let Some(d) = &p.desc {
        parcel.attributes.insert("Description".into(), d.clone());
    }
    Ok(parcel)
}

pub fn add_parcel(doc: &mut Document, parcel: &Parcel, source: &str) -> Option<ElementId> {
    let ring = parcel.outline(crate::civil_build::DISPLAY_FLATNESS_M).ok()?;
    let b = Bounds::of(ring.iter().map(|q| (q.e, q.n)));
    let v = serde_json::to_value(parcel).ok()?;
    Some(civil::add(doc, PARCEL, &parcel.name, v, b, source))
}

/// Flattens nested parcels.
pub fn all_parcels(file: &LandXmlFile) -> Vec<&LandXmlParcel> {
    fn walk<'a>(p: &'a LandXmlParcel, out: &mut Vec<&'a LandXmlParcel>) {
        out.push(p);
        for c in &p.parcels {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    for p in &file.parcels {
        walk(p, &mut out);
    }
    out
}

pub fn add_profile(doc: &mut Document, ps: &LandXmlProfSurf, source: &str) -> Vec<ElementId> {
    let mut ids = Vec::new();
    for (i, list) in ps.point_lists.iter().enumerate() {
        if list.len() < 2 {
            continue;
        }
        let name = if ps.point_lists.len() > 1 { format!("{} {}", ps.name, i + 1) } else { ps.name.clone() };
        let d = ProfileData { alignment: ps.alignment.clone(), surface: 0, interval_m: 0.0, imported: list.clone() };
        let v = serde_json::to_value(&d).unwrap_or_default();
        let id = civil::add(doc, PROFILE, &name, v, None, source);
        ids.push(id);
    }
    ids
}

/// A surface that keeps the source data of the LandXML surface: its points, breaklines, boundaries
/// and contours as operations. Returns None when the surface has no source data to rebuild from.
pub fn surface_definition(s: &LandXmlSurface, source: &str) -> Option<Surface> {
    if s.source_data.is_empty() {
        return None;
    }
    let mut def = SurfaceDefinition::new();
    let data_points: Vec<SurfacePoint> = s.source_data.data_points.iter().flat_map(|d| d.points.iter()).enumerate().map(|(i, p)| SurfacePoint::new(format!("{}", i + 1), p.x, p.y, p.z.unwrap_or(0.0))).collect();
    let pts = if data_points.is_empty() { s.points.iter().map(|p| SurfacePoint::new(p.id.to_string(), p.easting_m, p.northing_m, p.elevation_m)).collect() } else { data_points };
    if !pts.is_empty() {
        def.push(Operation::AddPoints { group: "Surface points".into(), points: pts });
    }
    let lines: Vec<Breakline> = s
        .source_data
        .breaklines
        .iter()
        .filter(|b| b.points.len() >= 2 && b.points.iter().all(|p| p.z.is_some()))
        .map(|b| Breakline { name: b.name.clone().unwrap_or_else(|| "Breakline".into()), kind: BreaklineKind::Standard, vertices: b.points.iter().map(|p| [p.x, p.y, p.z.unwrap_or(0.0)]).collect() })
        .collect();
    if !lines.is_empty() {
        def.push(Operation::AddBreaklines { lines });
    }
    let contour_lines: Vec<Vec<[f64; 3]>> = s.source_data.contours.iter().filter(|c| c.points.len() >= 2).map(|c| c.points.iter().map(|p| [p.x, p.y, c.elevation_m]).collect()).collect();
    if !contour_lines.is_empty() {
        def.push(Operation::AddContourData { name: "Contours".into(), lines: contour_lines, thin_distance: None, max_segment: None });
    }
    for b in &s.source_data.boundaries {
        let t = b.bnd_type.to_lowercase();
        let kind = if t.contains("outer") {
            BoundaryKind::Outer
        } else if t.contains("show") {
            BoundaryKind::Show
        } else {
            BoundaryKind::Hide
        };
        if b.points.len() >= 3 {
            def.push(Operation::AddBoundary { name: b.name.clone().unwrap_or_else(|| "Boundary".into()), kind, vertices: b.points.iter().map(|p| [p.x, p.y]).collect() });
        }
    }
    let mut surface = Surface::new(s.name.clone(), SurfaceRole::ExistingGround);
    surface.definition = def;
    surface.source.description = format!("LandXML surface {} with its source data", s.name);
    surface.source.reference = source.to_string();
    Some(surface)
}

pub fn add_surface_def(doc: &mut Document, surface: Surface, source: &str) -> ElementId {
    // The bounds come from the definition's first points operation.
    let b = surface.definition.operations.iter().find_map(|o| match o {
        Operation::AddPoints { points, .. } => Bounds::of(points.iter().map(|p| (p.x, p.y))),
        _ => None,
    });
    let name = surface.name.clone();
    let data = SurfaceData { surface, style: SurfaceStyle { border: true, ..Default::default() } };
    civil::add(doc, SURFACE, &name, serde_json::to_value(&data).unwrap_or_default(), b, source)
}
