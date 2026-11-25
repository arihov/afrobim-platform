// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The importer proper: schema layout, units, placements, geometry items, properties, tree.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use afrobim_draw::Mesh3;

use super::stepfile::{Entity, StepFile, Value};
use super::tables::{Product, SchemaTable, TABLES};
use super::tess::{circle, prism, triangulate_3d};
use super::xform::{cross, placement_2d, placement_3d, scale, unit, Mat, V3};
use super::*;

mod linear;

const GREY: [u8; 3] = [170, 170, 178];
const MAX_DEPTH: usize = 24;

/// A mesh in some local frame, before the product transform.
struct LocalMesh {
    name: &'static str,
    verts: Vec<V3>,
    tris: Vec<[u32; 3]>,
    /// Faces whose triangulation did not add up to outline minus holes.
    bad_faces: usize,
}

impl LocalMesh {
    /// Drops vertices no triangle uses (point lists are often shared between items or carry
    /// points that only another item references) and triangles that point outside the list.
    fn compact(mut self) -> LocalMesh {
        let n = self.verts.len();
        self.tris.retain(|t| t.iter().all(|&i| (i as usize) < n));
        let mut map = vec![u32::MAX; n];
        let mut used = 0u32;
        for t in &self.tris {
            for &i in t {
                if map[i as usize] == u32::MAX {
                    map[i as usize] = used;
                    used += 1;
                }
            }
        }
        if used as usize == n {
            return self;
        }
        let mut verts = vec![[0.0; 3]; used as usize];
        for (old, &new) in map.iter().enumerate() {
            if new != u32::MAX {
                verts[new as usize] = self.verts[old];
            }
        }
        for t in &mut self.tris {
            for i in t.iter_mut() {
                *i = map[*i as usize];
            }
        }
        self.verts = verts;
        self
    }
}

/// Tallies for one product.
#[derive(Default)]
struct Acc {
    read: BTreeMap<String, usize>,
    unread: BTreeMap<String, usize>,
    bad_faces: usize,
}

/// A profile outline and its holes.
type Profile = (Vec<[f64; 2]>, Vec<Vec<[f64; 2]>>);

struct MappedRep {
    meshes: Vec<LocalMesh>,
    acc: Acc,
}

struct Imp<'a> {
    f: &'a StepFile,
    table: &'static SchemaTable,
    attrs: HashMap<&'static str, &'static [&'static str]>,
    opts: &'a ImportOptions,
    unit_mm: f64,
    placements: HashMap<u32, Result<Mat, String>>,
    /// Basis curves of linear placements, evaluated once.
    curves: HashMap<u32, Result<Rc<linear::CurveEval>, String>>,
    maps: HashMap<u32, Rc<MappedRep>>,
    report: ImportReport,
}

fn family_for(schemas: &[String]) -> Result<(&'static SchemaTable, Option<String>), ImportError> {
    let first = schemas.first().map(|s| s.to_ascii_uppercase()).unwrap_or_default();
    let pick = |fam: &str| TABLES.iter().find(|t| t.family == fam).copied().unwrap();
    match first.as_str() {
        "IFC2X3" | "IFC2X3_TC1" => Ok((pick("IFC2X3"), None)),
        "IFC4" | "IFC4_ADD1" | "IFC4_ADD2" | "IFC4_ADD2_TC1" => Ok((pick("IFC4"), None)),
        "IFC4X3" | "IFC4X3_ADD1" | "IFC4X3_ADD2" | "IFC4X3_TC1" => Ok((pick("IFC4X3"), None)),
        s if s.starts_with("IFC4X3") => Ok((pick("IFC4X3"), Some(format!("schema {s}: read with the IFC4X3_ADD2 attribute layout")))),
        s if s.starts_with("IFC4X1") || s.starts_with("IFC4X2") => Ok((pick("IFC4"), Some(format!("schema {s}: read with the IFC4 attribute layout (IFC4X1 and IFC4X2 add entities, not attributes of the ones read here)")))),
        s if s.starts_with("IFC2X") => Ok((pick("IFC2X3"), Some(format!("schema {s}: read with the IFC2X3 attribute layout")))),
        other => Err(ImportError::UnsupportedSchema(other.to_string())),
    }
}

pub(super) fn import(f: &StepFile, opts: &ImportOptions) -> Result<ReferenceModel, ImportError> {
    let (table, note) = family_for(&f.header.schemas)?;
    let mut imp = Imp {
        f,
        table,
        attrs: table.attrs.iter().copied().collect(),
        opts,
        unit_mm: 1000.0,
        placements: HashMap::new(),
        curves: HashMap::new(),
        maps: HashMap::new(),
        report: ImportReport {
            schema: f.header.schemas.first().cloned().unwrap_or_default(),
            file_bytes: f.byte_len(),
            instances: f.len(),
            complex_instances: f.complex_instances,
            ..ImportReport::default()
        },
    };
    if let Some(n) = note {
        imp.report.warnings.push(n);
    }
    imp.run()
}

fn find_product(table: &'static SchemaTable, ty: &str) -> Option<&'static Product> {
    table.products.binary_search_by(|p| p.name.cmp(ty)).ok().map(|i| &table.products[i])
}

impl<'a> Imp<'a> {
    /// Attribute `name` of `e`, positioned by the entity `table_entity` of the schema table.
    fn ax(&self, table_entity: &str, e: &Entity, name: &str) -> Value {
        match self.attrs.get(table_entity).and_then(|a| a.iter().position(|n| *n == name)) {
            Some(i) => e.attr(i),
            None => Value::Null,
        }
    }

    fn a(&self, e: &Entity, name: &str) -> Value {
        self.ax(e.ty, e, name)
    }

    fn get(&self, id: u32) -> Option<Entity<'a>> {
        let f: &'a StepFile = self.f;
        f.get(id)
    }

    fn of_type(&self, name: &str) -> impl Iterator<Item = Entity<'a>> {
        let f: &'a StepFile = self.f;
        f.of_type(name)
    }

    fn ent(&self, v: &Value) -> Option<Entity<'a>> {
        v.as_ref().and_then(|id| self.get(id))
    }

    fn point(&self, id: u32) -> Option<V3> {
        let e = self.get(id)?;
        if e.ty != "IFCCARTESIANPOINT" {
            return None;
        }
        let c = e.attr(0).as_reals()?;
        Some([*c.first()?, c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)])
    }

    fn direction(&self, v: &Value) -> Option<V3> {
        let e = self.ent(v)?;
        let c = e.attr(0).as_reals()?;
        Some([*c.first()?, c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)])
    }

    /// An `IfcAxis2Placement3D` (or 2D, read as a placement in the XY plane).
    fn axis_placement(&self, id: u32) -> Option<Mat> {
        let e = self.get(id)?;
        match e.ty {
            "IFCAXIS2PLACEMENT3D" => {
                let loc = self.point(self.a(&e, "Location").as_ref()?)?;
                Some(placement_3d(loc, self.direction(&self.a(&e, "Axis")), self.direction(&self.a(&e, "RefDirection"))))
            }
            "IFCAXIS2PLACEMENT2D" => {
                let loc = self.point(self.a(&e, "Location").as_ref()?)?;
                let r = self.direction(&self.a(&e, "RefDirection")).map(|d| [d[0], d[1]]);
                Some(placement_2d([loc[0], loc[1]], r))
            }
            _ => None,
        }
    }

    /// World placement of an object placement, in file length units.
    fn placement(&mut self, id: u32) -> Result<Mat, String> {
        if let Some(r) = self.placements.get(&id) {
            return r.clone();
        }
        // Walk to the root iteratively, then fold back, so a long chain cannot overflow the stack.
        let mut chain: Vec<u32> = Vec::new();
        let mut cur = Some(id);
        let mut base: Result<Mat, String> = Ok(Mat::IDENTITY);
        while let Some(c) = cur {
            if let Some(r) = self.placements.get(&c) {
                base = r.clone();
                break;
            }
            if chain.len() > 4096 || chain.contains(&c) {
                base = Err("placement chain too long or cyclic".into());
                break;
            }
            let Some(e) = self.get(c) else {
                base = Err(format!("placement #{c} does not exist"));
                break;
            };
            if e.ty != "IFCLOCALPLACEMENT" && e.ty != "IFCLINEARPLACEMENT" {
                base = Err(format!("{} placement not resolved", e.ty));
                *self.report.placements_unresolved.entry(e.ty.to_string()).or_default() += 1;
                break;
            }
            chain.push(c);
            cur = self.a(&e, "PlacementRelTo").as_ref();
        }
        let mut acc = base;
        for &c in chain.iter().rev() {
            let e = self.get(c).unwrap();
            let rel = if e.ty == "IFCLINEARPLACEMENT" {
                self.linear_relative(&e)
            } else {
                Ok(self.a(&e, "RelativePlacement").as_ref().and_then(|r| self.axis_placement(r)).unwrap_or(Mat::IDENTITY))
            };
            acc = match (acc, rel) {
                (Ok(p), Ok(r)) => Ok(p.mul(&r)),
                (Err(why), _) | (_, Err(why)) => Err(why),
            };
            self.placements.insert(c, acc.clone());
        }
        acc
    }

    // ---------------------------------------------------------------- units

    fn unit_factor_mm(&self, unit: &Entity, depth: usize) -> Option<(f64, String)> {
        if depth > 4 {
            return None;
        }
        match unit.ty {
            "IFCSIUNIT" => {
                let name = self.a(unit, "Name").as_enum()?.to_string();
                if name != "METRE" {
                    return None;
                }
                let prefix = match self.a(unit, "Prefix").as_enum() {
                    None => 1.0,
                    Some(p) => match p {
                        "EXA" => 1e18,
                        "PETA" => 1e15,
                        "TERA" => 1e12,
                        "GIGA" => 1e9,
                        "MEGA" => 1e6,
                        "KILO" => 1e3,
                        "HECTO" => 1e2,
                        "DECA" => 1e1,
                        "DECI" => 1e-1,
                        "CENTI" => 1e-2,
                        "MILLI" => 1e-3,
                        "MICRO" => 1e-6,
                        "NANO" => 1e-9,
                        "PICO" => 1e-12,
                        "FEMTO" => 1e-15,
                        "ATTO" => 1e-18,
                        _ => return None,
                    },
                };
                let label = match self.a(unit, "Prefix").as_enum() {
                    Some(p) => format!("{} metre", p.to_ascii_lowercase()),
                    None => "metre".to_string(),
                };
                Some((prefix * 1000.0, label))
            }
            "IFCCONVERSIONBASEDUNIT" => {
                let name = self.a(unit, "Name").as_str().unwrap_or("").to_string();
                let m = self.ent(&self.a(unit, "ConversionFactor"))?;
                let value = self.a(&m, "ValueComponent").as_f64()?;
                let base = self.ent(&self.a(&m, "UnitComponent"))?;
                let (mm, _) = self.unit_factor_mm(&base, depth + 1)?;
                Some((value * mm, name))
            }
            _ => None,
        }
    }

    fn length_unit(&self) -> Option<(f64, String)> {
        let project = self.of_type("IFCPROJECT").next()?;
        let ua = self.ent(&self.a(&project, "UnitsInContext"))?;
        for u in self.a(&ua, "Units").refs() {
            let Some(unit) = self.get(u) else { continue };
            if self.a(&unit, "UnitType").as_enum() == Some("LENGTHUNIT") {
                if let Some(r) = self.unit_factor_mm(&unit, 0) {
                    return Some(r);
                }
            }
        }
        None
    }

    fn map_conversion(&self) -> Option<MapConversion> {
        let e = self.of_type("IFCMAPCONVERSION").next()?;
        let num = |n: &str, d: f64| self.a(&e, n).as_f64().unwrap_or(d);
        let unit_mm = self
            .ent(&self.a(&e, "TargetCRS"))
            .and_then(|crs| self.ent(&self.a(&crs, "MapUnit")))
            .and_then(|u| self.unit_factor_mm(&u, 0))
            .map(|(mm, _)| mm)
            .unwrap_or(self.unit_mm);
        let crs = self.ent(&self.a(&e, "TargetCRS"));
        let text = |n: &str| crs.as_ref().and_then(|c| self.a(c, n).as_str().map(str::to_string)).filter(|t| !t.trim().is_empty());
        Some(MapConversion {
            crs_name: text("Name"),
            crs_description: text("Description"),
            geodetic_datum: text("GeodeticDatum"),
            vertical_datum: text("VerticalDatum"),
            map_projection: text("MapProjection"),
            map_zone: text("MapZone"),
            eastings: num("Eastings", 0.0),
            northings: num("Northings", 0.0),
            orthogonal_height: num("OrthogonalHeight", 0.0),
            x_axis_abscissa: num("XAxisAbscissa", 1.0),
            x_axis_ordinate: num("XAxisOrdinate", 0.0),
            scale: num("Scale", 1.0),
            unit_mm,
        })
    }

    // ------------------------------------------------------------- geometry

    fn profile_polygon(&self, id: u32, acc: &mut Acc) -> Result<Profile, String> {
        let e = self.get(id).ok_or("missing profile")?;
        let pos = |e: &Entity| -> Mat { self.a(e, "Position").as_ref().and_then(|p| self.axis_placement(p)).unwrap_or(Mat::IDENTITY) };
        let to2 = |m: &Mat, p: [f64; 2]| {
            let q = m.apply([p[0], p[1], 0.0]);
            [q[0], q[1]]
        };
        match e.ty {
            "IFCRECTANGLEPROFILEDEF" => {
                let (x, y) = (self.a(&e, "XDim").as_f64().ok_or("rectangle without XDim")? / 2.0, self.a(&e, "YDim").as_f64().ok_or("rectangle without YDim")? / 2.0);
                let m = pos(&e);
                Ok((vec![to2(&m, [-x, -y]), to2(&m, [x, -y]), to2(&m, [x, y]), to2(&m, [-x, y])], vec![]))
            }
            "IFCCIRCLEPROFILEDEF" => {
                let r = self.a(&e, "Radius").as_f64().ok_or("circle without Radius")?;
                let m = pos(&e);
                Ok((circle(r, self.opts.circle_segments).into_iter().map(|p| to2(&m, p)).collect(), vec![]))
            }
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let outer = self.curve_points(&self.a(&e, "OuterCurve"), acc)?;
                let mut holes = Vec::new();
                if e.ty == "IFCARBITRARYPROFILEDEFWITHVOIDS" {
                    for c in self.a(&e, "InnerCurves").refs() {
                        holes.push(self.curve_points(&Value::Ref(c), acc)?);
                    }
                }
                Ok((outer, holes))
            }
            other => Err(other.to_string()),
        }
    }

    fn curve_points(&self, v: &Value, _acc: &mut Acc) -> Result<Vec<[f64; 2]>, String> {
        let c = self.ent(v).ok_or("missing curve")?;
        let mut pts: Vec<[f64; 2]> = Vec::new();
        match c.ty {
            "IFCPOLYLINE" => {
                for p in self.a(&c, "Points").refs() {
                    let q = self.point(p).ok_or("polyline point missing")?;
                    pts.push([q[0], q[1]]);
                }
            }
            "IFCINDEXEDPOLYCURVE" => {
                // Straight segments only: the whole point list, or IfcLineIndex segments. An
                // IfcArcIndex segment makes the curve unreadable (arcs are not approximated).
                let list = self.ent(&self.a(&c, "Points")).ok_or("poly curve without points")?;
                let ix = self.attrs.get(list.ty).and_then(|a| a.iter().position(|n| *n == "CoordList")).ok_or("poly curve point list")?;
                let coords = list.attr_points(ix).ok_or("bad point list")?;
                let segments = self.a(&c, "Segments");
                match segments.as_list() {
                    None => pts.extend(coords.iter().map(|p| [p[0], p[1]])),
                    Some(segs) => {
                        let mut last: Option<u32> = None;
                        for seg in segs {
                            let Value::Typed(name, inner) = seg else { return Err("IFCINDEXEDPOLYCURVE (segment form)".into()) };
                            if name != "IFCLINEINDEX" {
                                return Err(format!("IFCINDEXEDPOLYCURVE ({name} segments)"));
                            }
                            let idx: Vec<u32> = inner.as_list().ok_or("segment list")?.iter().filter_map(|v| v.as_i64()).map(|i| i as u32).collect();
                            for &i in &idx {
                                if last == Some(i) {
                                    continue;
                                }
                                let p = coords.get((i as usize).wrapping_sub(1)).ok_or("poly curve index out of range")?;
                                pts.push([p[0], p[1]]);
                                last = Some(i);
                            }
                        }
                    }
                }
            }
            other => return Err(other.to_string()),
        }
        while pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        pts.dedup();
        Ok(pts)
    }

    fn face_loop(&self, bound: &Entity, pool: &mut Pool) -> Option<Vec<u32>> {
        let lp = self.ent(&self.a(bound, "Bound"))?;
        if lp.ty != "IFCPOLYLOOP" {
            return None;
        }
        let mut ring: Vec<u32> = Vec::new();
        for pid in self.a(&lp, "Polygon").refs() {
            let idx = match pool.by_point.get(&pid) {
                Some(i) => *i,
                None => {
                    let p = self.point(pid)?;
                    pool.verts.push(p);
                    pool.by_point.insert(pid, (pool.verts.len() - 1) as u32);
                    (pool.verts.len() - 1) as u32
                }
            };
            ring.push(idx);
        }
        if ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        if self.a(bound, "Orientation").as_enum() == Some("F") {
            ring.reverse();
        }
        (ring.len() >= 3).then_some(ring)
    }

    /// Triangulates the faces of a shell or face set into the pool. Returns false if any face had
    /// to be skipped.
    fn read_faces(&self, faces: &[u32], pool: &mut Pool) -> bool {
        let mut ok = true;
        for fid in faces {
            let Some(face) = self.get(*fid) else { ok = false; continue };
            if face.ty != "IFCFACE" {
                ok = false;
                continue;
            }
            let mut outer: Option<Vec<u32>> = None;
            let mut inner: Vec<Vec<u32>> = Vec::new();
            for b in self.a(&face, "Bounds").refs() {
                let Some(bound) = self.get(b) else { ok = false; continue };
                let Some(ring) = self.face_loop(&bound, pool) else { ok = false; continue };
                if bound.ty == "IFCFACEOUTERBOUND" && outer.is_none() {
                    outer = Some(ring);
                } else {
                    inner.push(ring);
                }
            }
            if outer.is_none() && !inner.is_empty() {
                outer = Some(inner.remove(0));
            }
            let Some(outer) = outer else { ok = false; continue };
            pool.add_face(&outer, &inner);
        }
        ok
    }

    fn read_item(&mut self, id: u32, depth: usize, acc: &mut Acc) -> Vec<LocalMesh> {
        let Some(e) = self.get(id) else {
            *acc.unread.entry("(missing item)".into()).or_default() += 1;
            return Vec::new();
        };
        let ty = e.ty;
        if depth > MAX_DEPTH {
            *acc.unread.entry(ty.to_string()).or_default() += 1;
            return Vec::new();
        }
        let result: Result<Vec<LocalMesh>, String> = match ty {
            "IFCTRIANGULATEDFACESET" => self.read_triangulated(&e),
            "IFCPOLYGONALFACESET" => self.read_polygonal(&e),
            "IFCFACETEDBREP" => self.read_brep(&e),
            "IFCSHELLBASEDSURFACEMODEL" => self.read_surface_model(&e, true),
            "IFCFACEBASEDSURFACEMODEL" => self.read_surface_model(&e, false),
            "IFCEXTRUDEDAREASOLID" => self.read_extrusion(&e, acc),
            "IFCMAPPEDITEM" => return self.read_mapped(&e, depth, acc),
            _ => Err(ty.to_string()),
        };
        match result {
            Ok(m) if !m.is_empty() => {
                *acc.read.entry(ty.to_string()).or_default() += 1;
                acc.bad_faces += m.iter().map(|x| x.bad_faces).sum::<usize>();
                m
            }
            Ok(_) => {
                *acc.unread.entry(format!("{ty} (no triangles)")).or_default() += 1;
                Vec::new()
            }
            Err(what) => {
                // A failed profile or sub-entity is reported under the item with the reason.
                let key = if what == ty { ty.to_string() } else { format!("{ty} ({what})") };
                *acc.unread.entry(key).or_default() += 1;
                Vec::new()
            }
        }
    }

    fn read_triangulated(&self, e: &Entity) -> Result<Vec<LocalMesh>, String> {
        let list = self.ent(&self.a(e, "Coordinates")).ok_or("no coordinate list")?;
        let ix = self.attrs.get(list.ty).and_then(|a| a.iter().position(|n| *n == "CoordList")).ok_or("coordinate list type")?;
        let pts = list.attr_points(ix).ok_or("bad coordinate list")?;
        let ci = self.attrs.get(e.ty).and_then(|a| a.iter().position(|n| *n == "CoordIndex")).ok_or("no CoordIndex")?;
        let idx = e.attr_index_lists(ci).ok_or("bad CoordIndex")?;
        let pn = self.attrs.get(e.ty).and_then(|a| a.iter().position(|n| *n == "PnIndex")).and_then(|i| e.attr_ints(i));
        let n = match &pn {
            Some(p) => p.len(),
            None => pts.len(),
        };
        let mut tris = Vec::with_capacity(idx.len());
        for t in &idx {
            if t.len() != 3 || t.iter().any(|&i| i == 0 || i as usize > n) {
                continue;
            }
            let m = |i: u32| -> Option<u32> {
                let k = match &pn {
                    Some(p) => p[i as usize - 1],
                    None => i,
                };
                (k >= 1 && k as usize <= pts.len()).then(|| k - 1)
            };
            if let (Some(a), Some(b), Some(c)) = (m(t[0]), m(t[1]), m(t[2])) {
                tris.push([a, b, c]);
            }
        }
        Ok(vec![LocalMesh { name: "IFCTRIANGULATEDFACESET", verts: pts, tris, bad_faces: 0 }])
    }

    fn read_polygonal(&self, e: &Entity) -> Result<Vec<LocalMesh>, String> {
        let list = self.ent(&self.a(e, "Coordinates")).ok_or("no coordinate list")?;
        let ix = self.attrs.get(list.ty).and_then(|a| a.iter().position(|n| *n == "CoordList")).ok_or("coordinate list type")?;
        let pts = list.attr_points(ix).ok_or("bad coordinate list")?;
        let pn: Option<Vec<u32>> = self.attrs.get(e.ty).and_then(|a| a.iter().position(|n| *n == "PnIndex")).and_then(|i| e.attr_ints(i));
        let map = |i: u32| -> Option<u32> {
            let k = match &pn {
                Some(p) => *p.get((i as usize).checked_sub(1)?)?,
                None => i,
            };
            (k >= 1 && k as usize <= pts.len()).then(|| k - 1)
        };
        let mut tris: Vec<[u32; 3]> = Vec::new();
        let mut bad = 0usize;
        for fid in self.a(e, "Faces").refs() {
            let Some(face) = self.get(fid) else { continue };
            let Some(outer) = face.attr_ints(0) else { continue };
            let outer: Vec<u32> = outer.iter().filter_map(|&i| map(i)).collect();
            let holes: Vec<Vec<u32>> = if face.ty == "IFCINDEXEDPOLYGONALFACEWITHVOIDS" {
                face.attr_index_lists(1).unwrap_or_default().into_iter().map(|h| h.iter().filter_map(|&i| map(i)).collect()).collect()
            } else {
                Vec::new()
            };
            let o3: Vec<V3> = outer.iter().map(|&i| pts[i as usize]).collect();
            let h3: Vec<Vec<V3>> = holes.iter().map(|h| h.iter().map(|&i| pts[i as usize]).collect()).collect();
            let (t, ok) = triangulate_3d(&o3, &h3);
            bad += usize::from(!ok);
            let mut all: Vec<u32> = outer.clone();
            for h in &holes {
                all.extend_from_slice(h);
            }
            for tri in t {
                tris.push([all[tri[0] as usize], all[tri[1] as usize], all[tri[2] as usize]]);
            }
        }
        Ok(vec![LocalMesh { name: "IFCPOLYGONALFACESET", verts: pts, tris, bad_faces: bad }])
    }

    fn read_brep(&self, e: &Entity) -> Result<Vec<LocalMesh>, String> {
        let shell = self.ent(&self.a(e, "Outer")).ok_or("no outer shell")?;
        let mut pool = Pool::default();
        let ok = self.read_faces(&self.a(&shell, "CfsFaces").refs(), &mut pool);
        if !ok {
            return Err("faces with unsupported loops".to_string());
        }
        Ok(vec![pool.into_mesh("IFCFACETEDBREP")])
    }

    fn read_surface_model(&self, e: &Entity, shells: bool) -> Result<Vec<LocalMesh>, String> {
        let sets: Vec<u32> = if shells { self.a(e, "SbsmBoundary").refs() } else { self.a(e, "FbsmFaces").refs() };
        let mut pool = Pool::default();
        for s in sets {
            let Some(set) = self.get(s) else { continue };
            self.read_faces(&self.a(&set, "CfsFaces").refs(), &mut pool);
        }
        Ok(vec![pool.into_mesh(if shells { "IFCSHELLBASEDSURFACEMODEL" } else { "IFCFACEBASEDSURFACEMODEL" })])
    }

    fn read_extrusion(&self, e: &Entity, acc: &mut Acc) -> Result<Vec<LocalMesh>, String> {
        let prof = self.a(e, "SweptArea").as_ref().ok_or("no profile")?;
        let (outer, holes) = self.profile_polygon(prof, acc).map_err(|w| format!("profile {w}"))?;
        let depth = self.a(e, "Depth").as_f64().ok_or("no depth")?;
        let dir = self.direction(&self.a(e, "ExtrudedDirection")).and_then(unit).ok_or("no extrusion direction")?;
        let m = self.a(e, "Position").as_ref().and_then(|p| self.axis_placement(p)).unwrap_or(Mat::IDENTITY);
        if outer.len() < 3 {
            return Err("profile with fewer than 3 points".into());
        }
        let (verts, tris) = prism(&outer, &holes, scale(dir, depth));
        let verts = verts.into_iter().map(|v| m.apply(v)).collect();
        let tris = if m.det() < 0.0 { tris.into_iter().map(|t| [t[0], t[2], t[1]]).collect() } else { tris };
        Ok(vec![LocalMesh { name: "IFCEXTRUDEDAREASOLID", verts, tris, bad_faces: 0 }])
    }

    fn operator(&self, e: &Entity) -> Option<Mat> {
        let origin = self.point(self.a(e, "LocalOrigin").as_ref()?)?;
        let s = self.a(e, "Scale").as_f64().unwrap_or(1.0);
        let (s2, s3) = if e.ty == "IFCCARTESIANTRANSFORMATIONOPERATOR3DNONUNIFORM" { (self.a(e, "Scale2").as_f64().unwrap_or(s), self.a(e, "Scale3").as_f64().unwrap_or(s)) } else { (s, s) };
        let a1 = self.direction(&self.a(e, "Axis1")).and_then(unit);
        let a2 = self.direction(&self.a(e, "Axis2")).and_then(unit);
        let a3 = self.direction(&self.a(e, "Axis3")).and_then(unit);
        let (x, y, z) = match (a1, a2, a3) {
            (Some(x), Some(y), Some(z)) => (x, y, z),
            (x, y, z) => {
                let z = z.unwrap_or([0.0, 0.0, 1.0]);
                let x0 = x.unwrap_or(if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] });
                let xo = unit([x0[0] - z[0] * super::xform::dot(x0, z), x0[1] - z[1] * super::xform::dot(x0, z), x0[2] - z[2] * super::xform::dot(x0, z)])?;
                let y = y.unwrap_or_else(|| cross(z, xo));
                (xo, y, z)
            }
        };
        Some(Mat::from_axes(scale(x, s), scale(y, s2), scale(z, s3), origin))
    }

    fn read_mapped(&mut self, e: &Entity, depth: usize, acc: &mut Acc) -> Vec<LocalMesh> {
        let fail = |acc: &mut Acc, why: &str| {
            *acc.unread.entry(format!("IFCMAPPEDITEM ({why})")).or_default() += 1;
            Vec::new()
        };
        let Some(src) = self.a(e, "MappingSource").as_ref() else { return fail(acc, "no source") };
        let Some(src_e) = self.get(src) else { return fail(acc, "no source") };
        let Some(target) = self.ent(&self.a(e, "MappingTarget")) else { return fail(acc, "no target") };
        if target.ty != "IFCCARTESIANTRANSFORMATIONOPERATOR3D" && target.ty != "IFCCARTESIANTRANSFORMATIONOPERATOR3DNONUNIFORM" {
            return fail(acc, target.ty);
        }
        let Some(tm) = self.operator(&target) else { return fail(acc, "bad operator") };
        // The IFC documentation says the MappingOrigin "defines the coordinate system in which the
        // MappedRepresentation is defined", so a point of the mapped representation is placed
        // by the origin first and then by the target operator: target * origin. (ISO 10303-43
        // can be read as target * inverse(origin); IfcOpenShell, which this follows, does the
        // former. The two agree for the usual identity origin.)
        let origin = self.a(&src_e, "MappingOrigin").as_ref().and_then(|o| self.axis_placement(o)).unwrap_or(Mat::IDENTITY);
        // Frame of the mapped representation -> frame of the item that uses it.
        let xf = tm.mul(&origin);
        let rep = match self.maps.get(&src) {
            Some(r) => r.clone(),
            None => {
                let mut sub = Acc::default();
                let mut meshes = Vec::new();
                if let Some(rep) = self.ent(&self.a(&src_e, "MappedRepresentation")) {
                    for it in self.a(&rep, "Items").refs() {
                        meshes.extend(self.read_item(it, depth + 1, &mut sub));
                    }
                }
                let r = Rc::new(MappedRep { meshes, acc: sub });
                self.maps.insert(src, r.clone());
                r
            }
        };
        for (k, v) in &rep.acc.read {
            *acc.read.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &rep.acc.unread {
            *acc.unread.entry(k.clone()).or_default() += v;
        }
        *acc.read.entry("IFCMAPPEDITEM".into()).or_default() += 1;
        let flip = xf.det() < 0.0;
        rep.meshes
            .iter()
            .map(|m| LocalMesh {
                name: m.name,
                verts: m.verts.iter().map(|v| xf.apply(*v)).collect(),
                tris: if flip { m.tris.iter().map(|t| [t[0], t[2], t[1]]).collect() } else { m.tris.clone() },
                bad_faces: 0,
            })
            .collect()
    }

    /// Reads a product's geometry. Returns meshes in world mm and the status.
    fn product_geometry(&mut self, product: &Entity, world: &Result<Mat, String>, pm: &Mat) -> (Vec<Mesh3>, GeometryStatus, Vec<String>, Option<String>, Acc) {
        let mut acc = Acc::default();
        let rep = self.ax("IFCPRODUCT", product, "Representation");
        let Some(pds) = self.ent(&rep) else {
            return (Vec::new(), GeometryStatus::NoRepresentation, Vec::new(), None, acc);
        };
        struct RepInfo {
            ident: String,
            rtype: String,
            items: Vec<u32>,
        }
        let reps: Vec<RepInfo> = self
            .a(&pds, "Representations")
            .refs()
            .into_iter()
            .filter_map(|r| self.get(r))
            .map(|r| RepInfo {
                ident: self.a(&r, "RepresentationIdentifier").as_str().unwrap_or("").to_string(),
                rtype: self.a(&r, "RepresentationType").as_str().unwrap_or("").to_string(),
                items: self.a(&r, "Items").refs(),
            })
            .collect();
        const BODY_TYPES: [&str; 11] = ["Tessellation", "Brep", "AdvancedBrep", "SweptSolid", "AdvancedSweptSolid", "MappedRepresentation", "SolidModel", "SurfaceModel", "CSG", "Clipping", "SurfaceOrSolidModel"];
        let mut body: Vec<&RepInfo> = reps.iter().filter(|r| r.ident.eq_ignore_ascii_case("Body")).collect();
        if body.is_empty() {
            body = reps.iter().filter(|r| BODY_TYPES.iter().any(|t| t.eq_ignore_ascii_case(&r.rtype))).collect();
        }
        let bbox = reps.iter().find(|r| r.ident.eq_ignore_ascii_case("Box") || r.rtype.eq_ignore_ascii_case("BoundingBox"));
        let repr_type = body.first().map(|r| r.rtype.clone());
        let Ok(world_m) = world else {
            return (Vec::new(), GeometryStatus::PlacementUnresolved, Vec::new(), repr_type, acc);
        };
        let mut locals: Vec<LocalMesh> = Vec::new();
        for r in &body {
            for &it in &r.items {
                locals.extend(self.read_item(it, 0, &mut acc));
            }
        }
        let unread: Vec<String> = acc.unread.keys().cloned().collect();
        let total = pm.mul(world_m);
        let flip = total.det() < 0.0;
        let mut meshes: Vec<Mesh3> = locals
            .into_iter()
            .map(LocalMesh::compact)
            .map(|m| Mesh3 {
                name: m.name.to_string(),
                vertices: m.verts.iter().map(|v| total.apply(*v)).collect(),
                triangles: if flip { m.tris.iter().map(|t| [t[0], t[2], t[1]]).collect() } else { m.tris },
                colour: GREY,
            })
            .collect();
        meshes.retain(|m| !m.triangles.is_empty());
        let status = if !meshes.is_empty() {
            if unread.is_empty() {
                GeometryStatus::Read
            } else {
                GeometryStatus::Partial
            }
        } else if self.opts.bounding_box_placeholders {
            match bbox.and_then(|b| b.items.iter().filter_map(|&i| self.get(i)).find(|i| i.ty == "IFCBOUNDINGBOX")) {
                Some(bb) => {
                    let corner = self.a(&bb, "Corner").as_ref().and_then(|c| self.point(c));
                    let (x, y, z) = (self.a(&bb, "XDim").as_f64(), self.a(&bb, "YDim").as_f64(), self.a(&bb, "ZDim").as_f64());
                    match (corner, x, y, z) {
                        (Some(c), Some(x), Some(y), Some(z)) => {
                            let cube = Mesh3::cuboid("IFCBOUNDINGBOX", [0.0; 3], [x, y, z], GREY);
                            let m = Mat::translation(c);
                            let t = total.mul(&m);
                            let flip = t.det() < 0.0;
                            meshes.push(Mesh3 {
                                name: cube.name,
                                vertices: cube.vertices.iter().map(|v| t.apply(*v)).collect(),
                                triangles: if flip { cube.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect() } else { cube.triangles },
                                colour: GREY,
                            });
                            GeometryStatus::BoundingBoxPlaceholder
                        }
                        _ => GeometryStatus::NotRead,
                    }
                }
                None => {
                    if body.is_empty() {
                        GeometryStatus::NoRepresentation
                    } else {
                        GeometryStatus::NotRead
                    }
                }
            }
        } else if body.is_empty() {
            GeometryStatus::NoRepresentation
        } else {
            GeometryStatus::NotRead
        };
        (meshes, status, unread, repr_type, acc)
    }

    // ----------------------------------------------------------- properties

    fn value_text(v: &Value) -> String {
        match v {
            Value::Null | Value::Derived => String::new(),
            Value::Int(i) => i.to_string(),
            Value::Real(r) => format!("{r}"),
            Value::Str(s) => s.clone(),
            Value::Enum(e) => match e.as_str() {
                "T" => "true".into(),
                "F" => "false".into(),
                "U" => "unknown".into(),
                other => other.to_string(),
            },
            Value::Ref(r) => format!("#{r}"),
            Value::Binary(b) => b.clone(),
            Value::Typed(_, inner) => Self::value_text(inner),
            Value::List(l) => l.iter().map(Self::value_text).collect::<Vec<_>>().join(", "),
        }
    }

    fn type_name(v: &Value) -> String {
        match v {
            Value::Typed(n, _) => n.clone(),
            Value::Str(_) => "STRING".into(),
            Value::Int(_) => "INTEGER".into(),
            Value::Real(_) => "REAL".into(),
            Value::Enum(_) => "ENUMERATION".into(),
            Value::List(l) => l.first().map(Self::type_name).unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn read_definition(&mut self, def: u32, out: &mut Vec<Property>) {
        let Some(d) = self.get(def) else { return };
        let set = self.a(&d, "Name").as_str().unwrap_or("").to_string();
        match d.ty {
            "IFCPROPERTYSET" => {
                for p in self.a(&d, "HasProperties").refs() {
                    let Some(pe) = self.get(p) else { continue };
                    let name = pe.attr(0).as_str().unwrap_or("").to_string();
                    let (value, ty) = match pe.ty {
                        "IFCPROPERTYSINGLEVALUE" => {
                            let v = pe.attr(2);
                            (Self::value_text(&v), Self::type_name(&v))
                        }
                        "IFCPROPERTYENUMERATEDVALUE" | "IFCPROPERTYLISTVALUE" => {
                            let v = pe.attr(2);
                            (Self::value_text(&v), Self::type_name(&v))
                        }
                        other => {
                            *self.report.properties_not_read.entry(other.to_string()).or_default() += 1;
                            continue;
                        }
                    };
                    out.push(Property { set: set.clone(), name, value, value_type: ty });
                }
            }
            "IFCELEMENTQUANTITY" => {
                for q in self.a(&d, "Quantities").refs() {
                    let Some(qe) = self.get(q) else { continue };
                    if !qe.ty.starts_with("IFCQUANTITY") {
                        *self.report.properties_not_read.entry(qe.ty.to_string()).or_default() += 1;
                        continue;
                    }
                    out.push(Property { set: set.clone(), name: qe.attr(0).as_str().unwrap_or("").to_string(), value: Self::value_text(&qe.attr(3)), value_type: qe.ty.to_string() });
                }
            }
            other => {
                *self.report.properties_not_read.entry(other.to_string()).or_default() += 1;
            }
        }
    }

    // ------------------------------------------------------------------ run

    fn run(&mut self) -> Result<ReferenceModel, ImportError> {
        match self.length_unit() {
            Some((mm, name)) => {
                self.unit_mm = mm;
                self.report.length_unit_mm = mm;
                self.report.length_unit = name;
            }
            None => {
                self.unit_mm = 1000.0;
                self.report.length_unit_mm = 1000.0;
                self.report.length_unit = "metre (assumed, no length unit found)".into();
                self.report.warnings.push("no length unit in IfcProject.UnitsInContext; metres assumed".into());
            }
        }
        let mapconv = self.map_conversion();
        // Transform from file units to world mm.
        let pm: Mat = match (&mapconv, self.opts.apply_map_conversion) {
            (Some(mc), true) => {
                let theta = mc.x_axis_ordinate.atan2(mc.x_axis_abscissa);
                let (s, c) = theta.sin_cos();
                let sc = mc.scale;
                let conv = Mat::from_axes([c * sc, s * sc, 0.0], [-s * sc, c * sc, 0.0], [0.0, 0.0, sc], [mc.eastings, mc.northings, mc.orthogonal_height]);
                Mat::uniform_scale(mc.unit_mm).mul(&conv)
            }
            _ => Mat::uniform_scale(self.unit_mm),
        };
        let project = self.of_type("IFCPROJECT").next();
        let project_name = project.as_ref().and_then(|p| self.a(p, "Name").as_str().map(str::to_string)).unwrap_or_default();

        // Properties: product id -> definitions.
        let mut defs: HashMap<u32, Vec<u32>> = HashMap::new();
        if self.opts.properties {
            for rel in self.of_type("IFCRELDEFINESBYPROPERTIES") {
                let objs = self.a(&rel, "RelatedObjects").refs();
                let def = self.a(&rel, "RelatingPropertyDefinition").refs();
                for o in objs {
                    defs.entry(o).or_default().extend(def.iter().copied());
                }
            }
        }

        // Products in file order.
        let mut products: Vec<ReferenceProduct> = Vec::new();
        let mut index_of: HashMap<u32, usize> = HashMap::new();
        let ids: Vec<(u32, &'static Product)> = self.f.iter().filter_map(|e| find_product(self.table, e.ty).map(|p| (e.id, p))).collect();
        for (id, info) in ids {
            let e = self.get(id).unwrap();
            let world = match self.ax("IFCPRODUCT", &e, "ObjectPlacement").as_ref() {
                Some(pl) => self.placement(pl),
                None => Ok(Mat::IDENTITY),
            };
            let (meshes, status, unread, repr_type, acc) = self.product_geometry(&e, &world, &pm);
            for (k, v) in acc.read {
                *self.report.items_read.entry(k).or_default() += v;
            }
            for (k, v) in acc.unread {
                *self.report.items_not_read.entry(k).or_default() += v;
            }
            self.report.faces_inconsistent += acc.bad_faces;
            let bounds = {
                let mut b: Option<([f64; 3], [f64; 3])> = None;
                for m in &meshes {
                    for v in &m.vertices {
                        let (lo, hi) = b.get_or_insert((*v, *v));
                        for k in 0..3 {
                            lo[k] = lo[k].min(v[k]);
                            hi[k] = hi[k].max(v[k]);
                        }
                    }
                }
                b
            };
            let mut properties = Vec::new();
            if let Some(ds) = defs.get(&id) {
                for d in ds.clone() {
                    self.read_definition(d, &mut properties);
                }
            }
            let predefined = (info.predefined >= 0).then(|| e.attr(info.predefined as usize)).and_then(|v| v.as_enum().map(str::to_string));
            let object_type = self.ax("IFCPRODUCT", &e, "ObjectType").as_str().map(str::to_string);
            self.report.triangles += meshes.iter().map(|m| m.triangles.len()).sum::<usize>();
            index_of.insert(id, products.len());
            products.push(ReferenceProduct {
                step_id: id,
                global_id: e.attr(0).as_str().unwrap_or("").to_string(),
                name: e.attr(2).as_str().unwrap_or("").to_string(),
                entity: e.ty.to_string(),
                predefined_type: predefined,
                object_type,
                is_spatial: info.spatial,
                is_opening: info.opening,
                meshes,
                bounds,
                geometry: status,
                unread_items: unread,
                representation_type: repr_type,
                container: None,
                parent: None,
                properties,
            });
        }

        // Tree.
        let mut tree: Vec<SpatialNode> = Vec::new();
        let mut node_of: HashMap<u32, usize> = HashMap::new();
        let mut has_parent: Vec<bool> = Vec::new();
        let f = self.f;
        let mut node = |id: u32, tree: &mut Vec<SpatialNode>, has_parent: &mut Vec<bool>| -> Option<usize> {
            if let Some(n) = node_of.get(&id) {
                return Some(*n);
            }
            let e = f.get(id)?;
            tree.push(SpatialNode {
                step_id: id,
                entity: e.ty.to_string(),
                name: e.attr(2).as_str().unwrap_or("").to_string(),
                global_id: e.attr(0).as_str().unwrap_or("").to_string(),
                product: index_of.get(&id).copied(),
                children: Vec::new(),
                contained: Vec::new(),
            });
            has_parent.push(false);
            node_of.insert(id, tree.len() - 1);
            Some(tree.len() - 1)
        };
        if let Some(p) = &project {
            node(p.id, &mut tree, &mut has_parent);
        }
        for rel in f.of_type("IFCRELAGGREGATES") {
            let Some(parent_id) = self.a(&rel, "RelatingObject").as_ref() else { continue };
            let Some(pn) = node(parent_id, &mut tree, &mut has_parent) else { continue };
            for c in self.a(&rel, "RelatedObjects").refs() {
                if let Some(cn) = node(c, &mut tree, &mut has_parent) {
                    tree[pn].children.push(cn);
                    has_parent[cn] = true;
                }
                if let (Some(&ci), Some(&pi)) = (index_of.get(&c), index_of.get(&parent_id)) {
                    products[ci].parent = Some(pi);
                }
            }
        }
        for rel in f.of_type("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
            let Some(sid) = self.a(&rel, "RelatingStructure").as_ref() else { continue };
            let Some(sn) = node(sid, &mut tree, &mut has_parent) else { continue };
            for c in self.a(&rel, "RelatedElements").refs() {
                if let Some(&pi) = index_of.get(&c) {
                    tree[sn].contained.push(pi);
                    products[pi].container = Some(sn);
                }
            }
        }
        let roots: Vec<usize> = (0..tree.len()).filter(|&i| !has_parent[i]).collect();

        // Report.
        if self.report.faces_inconsistent > 0 {
            self.report.warnings.push(format!(
                "{} faces were triangulated to an area that differs from their outline minus holes (self-intersecting or overlapping loops); their geometry may be wrong",
                self.report.faces_inconsistent
            ));
        }
        let r = &mut self.report;
        r.products = products.len();
        for p in &products {
            let t = r.by_type.entry(p.entity.clone()).or_default();
            t.products += 1;
            if !p.meshes.is_empty() {
                t.with_geometry += 1;
            }
            if p.is_spatial {
                r.products_spatial += 1;
                continue;
            }
            match p.geometry {
                GeometryStatus::Read => r.products_read += 1,
                GeometryStatus::Partial => r.products_partial += 1,
                GeometryStatus::BoundingBoxPlaceholder => r.products_placeholder += 1,
                GeometryStatus::NotRead => r.products_not_read += 1,
                GeometryStatus::NoRepresentation => r.products_no_representation += 1,
                GeometryStatus::PlacementUnresolved => r.products_placement_unresolved += 1,
            }
            if let Some(rt) = &p.representation_type {
                let c = r.by_representation.entry(rt.clone()).or_default();
                match p.geometry {
                    GeometryStatus::Read | GeometryStatus::Partial => c.read += 1,
                    _ => c.not_read += 1,
                }
            }
        }
        Ok(ReferenceModel { schema: r.schema.clone(), project_name, products, tree, roots, map_conversion: mapconv, report: self.report.clone() })
    }
}

/// Vertices and triangles of a face set under construction, with points shared by id.
#[derive(Default)]
struct Pool {
    verts: Vec<V3>,
    tris: Vec<[u32; 3]>,
    by_point: HashMap<u32, u32>,
    bad: usize,
}

impl Pool {
    fn add_face(&mut self, outer: &[u32], inner: &[Vec<u32>]) {
        if inner.is_empty() && outer.len() == 3 {
            self.tris.push([outer[0], outer[1], outer[2]]);
            return;
        }
        let o3: Vec<V3> = outer.iter().map(|&i| self.verts[i as usize]).collect();
        let h3: Vec<Vec<V3>> = inner.iter().map(|h| h.iter().map(|&i| self.verts[i as usize]).collect()).collect();
        let mut all: Vec<u32> = outer.to_vec();
        for h in inner {
            all.extend_from_slice(h);
        }
        let (tris, ok) = triangulate_3d(&o3, &h3);
        self.bad += usize::from(!ok);
        for t in tris {
            self.tris.push([all[t[0] as usize], all[t[1] as usize], all[t[2] as usize]]);
        }
    }

    fn into_mesh(self, name: &'static str) -> LocalMesh {
        LocalMesh { name, verts: self.verts, tris: self.tris, bad_faces: self.bad }
    }
}
