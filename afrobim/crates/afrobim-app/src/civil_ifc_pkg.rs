//! The civil objects as packages for the IFC exporter (see `afrobim_ifc::CivilPackage`): corridor
//! shapes as courses, kerbs and other classified parts with solids between the sections, cut and
//! fill solids from the earthworks lines, and pipe networks as segments, chambers and a system.
//! Nothing here chooses an IFC entity by itself: a shape is written as the entity its own
//! classification names, and a shape with none is an `IfcCivilElement` that carries its code.

use std::collections::BTreeMap;

use afrobim_corridor::{build_model, BaselineResult};
use afrobim_draw::Mesh3;
use afrobim_ifc::{CivilGroup, CivilPackage, CivilPart, CivilSet, CivilValue, GroupKind, CIVIL_CATEGORY};
use afrobim_kernel::{Element, Generated, SitePlacement};

use crate::civil::{self, CorridorData};
use crate::civil_build::Ctx;

/// A package element and its solids, with what could not be written.
pub struct Package {
    pub element: Element,
    pub generated: Generated,
}

fn element_of(source: &Element, name: &str, origin_m: (f64, f64), package: &CivilPackage) -> Element {
    Element {
        id: source.id,
        category: CIVIL_CATEGORY.to_string(),
        name: name.to_string(),
        origin: afrobim_kernel::model_point_from_crs(origin_m.0, origin_m.1),
        width: 1.0,
        height: 1.0,
        layer: civil::LAYER.to_string(),
        data: serde_json::to_value(package).unwrap_or_default(),
        properties: BTreeMap::new(),
        site: Some(SitePlacement { x: origin_m.0 * 1000.0, y: origin_m.1 * 1000.0, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None }),
        host: None,
    }
}

fn set(name: &str, items: Vec<(&str, CivilValue)>) -> CivilSet {
    CivilSet { name: name.to_string(), items: items.into_iter().map(|(k, v)| (k.to_string(), v)).collect() }
}

use crate::civil_build2::add_prism as prism;

/// The cut and fill pieces of a section: for each segment of the earthworks line (a vertical step has
/// no width and no piece), the quadrilateral between the line and the ground when the ground is on one
/// side of it, or two triangles split at the crossing. Each piece is keyed by its segment and part so
/// that the same piece of the next section can be found, and says whether it is cut.
fn earthwork_pieces(s: &afrobim_corridor::Section) -> Vec<((usize, u8), bool, Vec<(f64, f64)>)> {
    let mut out = Vec::new();
    for (i, w) in s.earthworks.windows(2).enumerate() {
        let ((o0, z0), (o1, z1)) = (w[0], w[1]);
        if (o1 - o0).abs() < 1e-9 {
            continue;
        }
        let (Some(g0), Some(g1)) = (s.ground_at(o0), s.ground_at(o1)) else { continue };
        let (d0, d1) = (g0 - z0, g1 - z1);
        if d0 * d1 >= 0.0 {
            out.push(((i, 0), d0 + d1 > 0.0, vec![(o0, z0), (o1, z1), (o1, g1), (o0, g0)]));
        } else {
            let t = d0 / (d0 - d1);
            let (oc, zc) = (o0 + t * (o1 - o0), z0 + t * (z1 - z0));
            out.push(((i, 0), d0 > 0.0, vec![(o0, z0), (oc, zc), (o0, g0)]));
            out.push(((i, 1), d1 > 0.0, vec![(oc, zc), (o1, z1), (o1, g1)]));
        }
    }
    out
}

fn area(poly: &[(f64, f64)]) -> f64 {
    (0..poly.len()).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        a.0 * b.1 - b.0 * a.1
    }).sum::<f64>().abs() / 2.0
}

/// The package of one corridor: its baselines' shapes, kerbs and earthworks. None when the
/// corridor cannot be built (the reason is in the notes of the second value).
pub fn corridor_package(e: &Element, ctx: &Ctx) -> Result<Package, String> {
    let d: CorridorData = serde_json::from_value(e.data.clone()).map_err(|x| format!("its data cannot be read: {x}"))?;
    let ground = ctx.tin(d.ground).ok_or("its ground surface is missing")?;
    let built = build_model(&d.model, &ctx.alignments(), &ground).map_err(|x| format!("it cannot be built: {x}"))?;
    let origin = (e.origin.x / 1000.0, e.origin.y / 1000.0);
    let mut package = CivilPackage { title: e.name.clone(), ground: Some(d.ground), ..Default::default() };
    let mut meshes: Vec<Mesh3> = Vec::new();
    for b in &built.baselines {
        let road_part = package.groups.len();
        package.groups.push(CivilGroup { name: format!("{} {}", e.name, b.name), kind: GroupKind::RoadPart, predefined: "ROADSEGMENT".into(), parent: None });
        shapes(&mut package, &mut meshes, b, origin, road_part);
        earthworks(&mut package, &mut meshes, b, origin, road_part);
        package.not_mapped.extend(b.notes.iter().map(|n| format!("{}: {n}", b.name)));
    }
    let g = Generated { solids: meshes, ..Default::default() };
    Ok(Package { element: element_of(e, &format!("{} (civil)", e.name), origin, &package), generated: g })
}

fn shapes(package: &mut CivilPackage, meshes: &mut Vec<Mesh3>, b: &BaselineResult, origin: (f64, f64), road_part: usize) {
    // Solids per shape code from the same builder the 3D view uses.
    let solids = crate::civil_build2::shape_solids(b, origin, 5_000_000);
    let mut pavement: Option<usize> = None;
    for mesh in solids {
        let code = mesh.name.clone();
        // The classification of the first shape of this code, and its volume by average end area.
        let mut class = None;
        let (mut volume, mut width_sum, mut width_n) = (0.0, 0.0, 0.0);
        let mut subassembly = String::new();
        for pair in b.sections.windows(2) {
            let area_of = |m: &afrobim_corridor::ModelSection| m.design.shapes.iter().filter(|s| s.code == code).map(|s| s.area_m2).sum::<f64>();
            let (a0, a1) = (area_of(&pair[0]), area_of(&pair[1]));
            volume += (a0 + a1) / 2.0 * (pair[1].section.station_m - pair[0].section.station_m);
            for s in pair[0].design.shapes.iter().filter(|s| s.code == code) {
                if class.is_none() {
                    class = Some(s.classification.clone());
                    subassembly = s.subassembly.clone();
                }
                let xs: Vec<f64> = s.polygon.iter().map(|p| p.0).collect();
                if let (Some(lo), Some(hi)) = (xs.iter().cloned().reduce(f64::min), xs.iter().cloned().reduce(f64::max)) {
                    width_sum += hi - lo;
                    width_n += 1.0;
                }
            }
        }
        let class = class.unwrap_or_default();
        let entity = class.ifc_entity.as_deref().map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty());
        let predefined = class.predefined_type.as_deref().map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty());
        let (entity, predefined, why) = match (entity, predefined) {
            (Some(en), pre) => {
                let pre = pre.unwrap_or_else(|| "NOTDEFINED".to_string());
                (en, pre, None)
            }
            _ => ("IFCCIVILELEMENT".to_string(), code.clone(), Some(format!("shape '{code}' has no IFC classification entered: written as IfcCivilElement with its code as the object type"))),
        };
        if let Some(w) = why {
            package.not_mapped.push(w);
        }
        let length = (b.end_station_m - b.start_station_m) * 1000.0;
        let thickness_mm = if width_n > 0.0 && volume > 0.0 && length > 0.0 { None } else { None::<f64> };
        let _ = thickness_mm;
        let mean_width_mm = if width_n > 0.0 { width_sum / width_n * 1000.0 } else { 0.0 };
        let material = class.material.clone();
        let mut props = vec![("Code", CivilValue::Text(code.clone())), ("Subassembly", CivilValue::Text(subassembly)), ("Baseline", CivilValue::Text(b.name.clone())), ("StartStationM", CivilValue::Real(b.start_station_m)), ("EndStationM", CivilValue::Real(b.end_station_m)), ("VolumeM3", CivilValue::Real(volume))];
        if let Some(m) = &material {
            props.push(("Material", CivilValue::Text(m.clone())));
        }
        if let Some(c) = class.cost_item_code.clone() {
            props.push(("CostItemCode", CivilValue::Text(c)));
        }
        let mut qtos = Vec::new();
        match entity.as_str() {
            "IFCCOURSE" => qtos.push(set("Qto_CourseBaseQuantities", vec![("Length", CivilValue::Length(length)), ("Width", CivilValue::Length(mean_width_mm)), ("Volume", CivilValue::Volume(volume))])),
            "IFCKERB" => qtos.push(set("Qto_KerbBaseQuantities", vec![("Length", CivilValue::Length(length)), ("Volume", CivilValue::Volume(volume))])),
            _ => {}
        }
        // Courses belong to one pavement, which the road part contains.
        let group = if entity == "IFCCOURSE" {
            Some(*pavement.get_or_insert_with(|| {
                package.groups.push(CivilGroup { name: format!("{} pavement", b.name), kind: GroupKind::Pavement, predefined: "NOTDEFINED".into(), parent: Some(road_part) });
                package.groups.len() - 1
            }))
        } else {
            Some(road_part)
        };
        let idx = meshes.len();
        meshes.push(mesh);
        package.parts.push(CivilPart { name: format!("{} {code}", b.name), entity, predefined, material, mesh: Some(idx), group, psets: vec![set("AfroBIM_CorridorShape", props)], qtos });
    }
}

fn earthworks(package: &mut CivilPackage, meshes: &mut Vec<Mesh3>, b: &BaselineResult, origin: (f64, f64), road_part: usize) {
    let mut cut = Mesh3 { name: "Cut".into(), vertices: vec![], triangles: vec![], colour: [0xA0, 0x7A, 0x4F] };
    let mut fill = Mesh3 { name: "Fill".into(), vertices: vec![], triangles: vec![], colour: [0x9A, 0xA0, 0x5E] };
    let (mut cut_v, mut fill_v) = (0.0, 0.0);
    const CAP: usize = 60_000;
    for pair in b.sections.windows(2) {
        let (s0, s1) = (&pair[0].section, &pair[1].section);
        let ds = s1.station_m - s0.station_m;
        if ds <= 0.0 {
            continue;
        }
        let (p0, p1) = (earthwork_pieces(s0), earthwork_pieces(s1));
        for (key, is_cut, poly) in &p0 {
            let Some((_, _, poly1)) = p1.iter().find(|(k, c, q)| k == key && c == is_cut && q.len() == poly.len()) else { continue };
            let (a_poly, c_poly) = (poly, poly1);
            let at = |s: &afrobim_corridor::Section, (o, z): (f64, f64)| {
                let (x, y) = s.plan_point(o);
                [(x - origin.0) * 1000.0, (y - origin.1) * 1000.0, z * 1000.0]
            };
            let (qa, qc): (Vec<[f64; 3]>, Vec<[f64; 3]>) = (a_poly.iter().map(|p| at(s0, *p)).collect(), c_poly.iter().map(|p| at(s1, *p)).collect());
            let (mesh, v) = if *is_cut { (&mut cut, &mut cut_v) } else { (&mut fill, &mut fill_v) };
            if mesh.triangles.len() > CAP {
                continue;
            }
            *v += (area(a_poly) + area(c_poly)) / 2.0 * ds;
            prism(mesh, &qa, &qc);
        }
    }
    let length = (b.end_station_m - b.start_station_m) * 1000.0;
    // The corridor's own totals, by the strips between the earthworks line and the ground at every vertex of either.
    let strips = |f: &dyn Fn(&afrobim_corridor::Section) -> f64| b.sections.windows(2).map(|w| (f(&w[0].section) + f(&w[1].section)) / 2.0 * (w[1].section.station_m - w[0].section.station_m)).sum::<f64>();
    let engine = |is_cut: bool| if is_cut { strips(&|s| s.cut_area_m2.unwrap_or(0.0)) } else { strips(&|s| s.fill_area_m2.unwrap_or(0.0)) };
    for (mesh, _, is_cut) in [(cut, cut_v, true), (fill, fill_v, false)] {
        if mesh.triangles.is_empty() {
            continue;
        }
        // The quantity is the volume of the solid written, so that the quantity set and the geometry agree.
        let volume = crate::civil_build2::signed_volume(&mesh, 0) / 1.0e9;
        let strip_volume = engine(is_cut);
        if strip_volume > 0.0 && (volume - strip_volume).abs() > 0.02 * strip_volume {
            package.not_mapped.push(format!("{} {}: the solid is {volume:.1} m3 where the corridor's own end-area total is {strip_volume:.1} m3 ({:+.0} %): the solid takes the ground at the vertices of the earthworks line only", b.name, if is_cut { "cut" } else { "fill" }, (volume / strip_volume - 1.0) * 100.0));
        }
        let idx = meshes.len();
        meshes.push(mesh);
        let (entity, predefined, qto) = if is_cut {
            ("IFCEARTHWORKSCUT", "CUT", set("Qto_EarthworksCutBaseQuantities", vec![("Length", CivilValue::Length(length)), ("UndisturbedVolume", CivilValue::Volume(volume))]))
        } else {
            ("IFCEARTHWORKSFILL", "NOTDEFINED", set("Qto_EarthworksFillBaseQuantities", vec![("Length", CivilValue::Length(length)), ("CompactedVolume", CivilValue::Volume(volume))]))
        };
        let props = vec![("Baseline", CivilValue::Text(b.name.clone())), ("StartStationM", CivilValue::Real(b.start_station_m)), ("EndStationM", CivilValue::Real(b.end_station_m)), ("VolumeM3", CivilValue::Real(volume)), ("EndAreaVolumeM3", CivilValue::Real(strip_volume)), ("Note", CivilValue::Text("volume of the solid written; EndAreaVolumeM3 is the corridor's own total by average end area; both before swell and shrink".into()))];
        package.parts.push(CivilPart { name: format!("{} {}", b.name, if is_cut { "cut" } else { "fill" }), entity: entity.into(), predefined: predefined.into(), material: None, mesh: Some(idx), group: Some(road_part), psets: vec![set("AfroBIM_Earthworks", props)], qtos: vec![qto] });
    }
}

/// The package of one pipe network.
pub fn pipe_package(e: &Element, d: &crate::civil_pipes::PipeNetData) -> Result<Package, String> {
    use afrobim_pipes::{NetworkType, PipeShape, StructureKind};
    let net = &d.network;
    let (mut lo_e, mut lo_n) = (f64::INFINITY, f64::INFINITY);
    for s in net.structures() {
        lo_e = lo_e.min(s.easting_m);
        lo_n = lo_n.min(s.northing_m);
    }
    if !lo_e.is_finite() {
        return Err("it has no structures".into());
    }
    let origin = (lo_e.floor(), lo_n.floor());
    let mut package = CivilPackage { title: e.name.clone(), ..Default::default() };
    let system_type = match net.network_type {
        NetworkType::Storm => "STORMWATER",
        NetworkType::Sanitary => "SEWAGE",
        NetworkType::Water | NetworkType::Other => "NOTDEFINED",
    };
    package.groups.push(CivilGroup { name: net.name.clone(), kind: GroupKind::System, predefined: system_type.into(), parent: None });
    if matches!(net.network_type, NetworkType::Water | NetworkType::Other) {
        package.not_mapped.push(format!("network type {:?} has no verified system type; the system is written as NOTDEFINED", net.network_type));
    }
    let mut meshes: Vec<Mesh3> = Vec::new();
    let local = |x: f64, y: f64, z: f64| [(x - origin.0) * 1000.0, (y - origin.1) * 1000.0, z * 1000.0];
    let ring = |n: usize, r: f64| (0..n).map(|i| { let a = std::f64::consts::TAU * i as f64 / n as f64; (r * a.cos(), r * a.sin()) }).collect::<Vec<_>>();
    for s in net.structures() {
        let (entity_pre, object_type) = match s.kind {
            StructureKind::Manhole => ("MANHOLE", None),
            StructureKind::CatchBasin => ("USERDEFINED", Some("CatchBasin")),
            StructureKind::Junction => ("USERDEFINED", Some("Junction")),
            StructureKind::Headwall | StructureKind::Outfall | StructureKind::Null => {
                package.not_mapped.push(format!("structure '{}' ({:?}) is not a chamber and has no verified IFC entity", s.name, s.kind));
                continue;
            }
        };
        let bottom = s.sump_level_m.unwrap_or(s.rim_level_m - 1.0);
        let r = s.inner.map_or(0.5, |i| i.bounding_radius_m()) + s.wall_thickness_m.unwrap_or(0.0);
        let mut mesh = Mesh3 { name: s.name.clone(), vertices: vec![], triangles: vec![], colour: [0x8A, 0x8F, 0x96] };
        let pts = ring(16, r);
        let lower: Vec<[f64; 3]> = pts.iter().map(|p| local(s.easting_m + p.0, s.northing_m + p.1, bottom)).collect();
        let upper: Vec<[f64; 3]> = pts.iter().map(|p| local(s.easting_m + p.0, s.northing_m + p.1, s.rim_level_m)).collect();
        prism(&mut mesh, &lower, &upper);
        let idx = meshes.len();
        meshes.push(mesh);
        let props = vec![("RimLevelM", CivilValue::Real(s.rim_level_m)), ("SumpLevelM", s.sump_level_m.map_or(CivilValue::Text("not set".into()), CivilValue::Real)), ("Kind", CivilValue::Text(format!("{:?}", s.kind)))];
        let mut part = CivilPart { name: s.name.clone(), entity: "IFCDISTRIBUTIONCHAMBERELEMENT".into(), predefined: entity_pre.into(), material: None, mesh: Some(idx), group: Some(0), psets: vec![set("AfroBIM_Structure", props)], qtos: vec![set("Qto_DistributionChamberElementBaseQuantities", vec![("Depth", CivilValue::Length((s.rim_level_m - bottom) * 1000.0))])] };
        if let Some(o) = object_type {
            part.psets.push(set("AfroBIM_StructureKind", vec![("ObjectType", CivilValue::Text(o.into()))]));
        }
        package.parts.push(part);
    }
    for p in net.pipes() {
        let Ok(path) = net.pipe_path(p.id) else {
            package.not_mapped.push(format!("pipe '{}' has no plan path", p.name));
            continue;
        };
        let (dia, rect) = match &p.shape {
            PipeShape::Circular { diameter_m } => (*diameter_m, None),
            PipeShape::Box { width_m, height_m } => (width_m.max(*height_m), Some((*width_m, *height_m))),
            _ => {
                package.not_mapped.push(format!("pipe '{}' has a section with no solid here (only circular and box sections are drawn)", p.name));
                continue;
            }
        };
        let wall = p.wall_thickness_m.unwrap_or(0.0);
        let chain = &path.chainage_m;
        let total = path.length_m.max(1e-9);
        let level_at = |c: f64| p.start_invert_m + (p.end_invert_m - p.start_invert_m) * (c / total);
        let mut mesh = Mesh3 { name: p.name.clone(), vertices: vec![], triangles: vec![], colour: [0x4D, 0xA3, 0xFF] };
        let profile: Vec<(f64, f64)> = match rect {
            Some((w, h)) => vec![(-(w / 2.0 + wall), 0.0), (w / 2.0 + wall, 0.0), (w / 2.0 + wall, h + 2.0 * wall), (-(w / 2.0 + wall), h + 2.0 * wall)],
            None => ring(16, dia / 2.0 + wall).into_iter().map(|(x, y)| (x, y + dia / 2.0 + wall)).collect(),
        };
        for i in 1..path.points.len() {
            let (a, c) = (path.points[i - 1], path.points[i]);
            let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
            let l = dx.hypot(dy);
            if l < 1e-9 {
                continue;
            }
            let (nx, ny) = (-dy / l, dx / l);
            let at = |pt: [f64; 2], chain_m: f64| -> Vec<[f64; 3]> { profile.iter().map(|(u, v)| local(pt[0] + nx * u, pt[1] + ny * u, level_at(chain_m) - wall + v)).collect() };
            prism(&mut mesh, &at(a, chain[i - 1]), &at(c, chain[i]));
        }
        let idx = meshes.len();
        meshes.push(mesh);
        let section_area = match rect { Some((w, h)) => w * h, None => std::f64::consts::PI * dia * dia / 4.0 };
        let mut props = vec![("InnerDiameterMm", CivilValue::Length(dia * 1000.0)), ("StartInvertM", CivilValue::Real(p.start_invert_m)), ("EndInvertM", CivilValue::Real(p.end_invert_m)), ("PlanLengthM", CivilValue::Real(path.length_m))];
        if let Some(w) = p.wall_thickness_m {
            props.push(("WallThicknessMm", CivilValue::Length(w * 1000.0)));
        }
        if let Some((w, h)) = rect {
            props.push(("BoxWidthMm", CivilValue::Length(w * 1000.0)));
            props.push(("BoxHeightMm", CivilValue::Length(h * 1000.0)));
        }
        if let (Ok(a), Ok(b)) = (net.structure(p.start), net.structure(p.end)) {
            props.push(("FromStructure", CivilValue::Text(a.name.clone())));
            props.push(("ToStructure", CivilValue::Text(b.name.clone())));
        }
        package.parts.push(CivilPart {
            name: p.name.clone(),
            entity: "IFCPIPESEGMENT".into(),
            predefined: "NOTDEFINED".into(),
            material: p.material.clone(),
            mesh: Some(idx),
            group: Some(0),
            psets: vec![set("AfroBIM_Pipe", props)],
            qtos: vec![set("Qto_PipeSegmentBaseQuantities", vec![("Length", CivilValue::Length(path.length_m * 1000.0)), ("NetCrossSectionArea", CivilValue::Area(section_area))])],
        });
    }
    let g = Generated { solids: meshes, ..Default::default() };
    Ok(Package { element: element_of(e, &format!("{} (pipes)", e.name), origin, &package), generated: g })
}
