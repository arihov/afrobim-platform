// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The model export of the road modules checked without an IFC toolkit: a document with an
//! element of every module is exported, and the entities, their counts, relationships,
//! placements, geometry, property sets and quantities are read back from the STEP text and
//! compared with what each module produced.

mod common;

use std::collections::BTreeSet;

use afrobim_draw::Mesh3;
use afrobim_ifc::{element_global_id, export_ifc_model, write_ifc_model, IfcElementInput};

use common::options;
use common::roads::{road_fixture, RoadFixture};
use common::step::{number, numbers, reference, references, split_args, StepFile};

struct Exported {
    f: RoadFixture,
    file: StepFile,
    notes: Vec<String>,
}

fn exported() -> Exported {
    let f = road_fixture();
    let export = export_ifc_model(&f.document, &f.inputs(), &options());
    let file = StepFile::parse(&export.text);
    Exported { f, file, notes: export.notes }
}

impl Exported {
    fn product(&self, name: &str) -> u32 {
        self.file.by_guid(&element_global_id(&options().project_name, self.f.id(name)))
    }

    /// The product named `name` whose predefined type is `predefined`: parts and barrels share
    /// the mark of their element.
    fn named(&self, entity: &str, name: &str) -> u32 {
        let found: Vec<u32> = self.file.of_type(entity).into_iter().filter(|id| self.file.args(*id)[2] == format!("'{name}'")).collect();
        assert_eq!(found.len(), 1, "{entity} named {name}: {found:?}");
        found[0]
    }

    fn placement(&self, product: u32) -> u32 {
        reference(&self.file.args(product)[5])
    }

    fn world(&self, product: u32) -> [f64; 4] {
        self.file.world(self.placement(product))
    }

    fn predefined(&self, product: u32) -> &str {
        self.file.args(product).last().unwrap()
    }

    /// The triangulated face sets of the Body of a product: (points, triangles, closed).
    fn face_sets(&self, product: u32) -> Vec<(Vec<[f64; 3]>, usize, bool)> {
        let Some(shape) = self.file.args(product).get(6).filter(|a| a.starts_with('#')) else { return Vec::new() };
        let representation = references(&self.file.args(reference(shape))[2])[0];
        assert_eq!(self.file.args(representation)[1], "'Body'");
        assert_eq!(self.file.args(representation)[2], "'Tessellation'");
        references(&self.file.args(representation)[3])
            .iter()
            .map(|item| {
                assert_eq!(self.file.name(*item), "IFCTRIANGULATEDFACESET");
                let a = self.file.args(*item);
                let coords = self.file.args(reference(&a[0]));
                let inner = coords[0].strip_prefix('(').and_then(|c| c.strip_suffix(')')).unwrap();
                let points: Vec<[f64; 3]> = split_args(inner).iter().map(|p| numbers(p)).map(|p| [p[0], p[1], p[2]]).collect();
                let triangles = split_args(a[3].strip_prefix('(').and_then(|c| c.strip_suffix(')')).unwrap()).len();
                (points, triangles, a[2] == ".T.")
            })
            .collect()
    }

    /// The world bounding box of the bodies of `products`.
    fn world_bounds(&self, products: &[u32]) -> ([f64; 3], [f64; 3]) {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in products {
            let [x, y, z, turn] = self.world(*p);
            let (sin, cos) = turn.to_radians().sin_cos();
            for (points, _, _) in self.face_sets(*p) {
                for v in points {
                    let w = [x + v[0] * cos - v[1] * sin, y + v[0] * sin + v[1] * cos, z + v[2]];
                    for k in 0..3 {
                        lo[k] = lo[k].min(w[k]);
                        hi[k] = hi[k].max(w[k]);
                    }
                }
            }
        }
        (lo, hi)
    }

    fn pset(&self, product: u32, set: &str) -> std::collections::BTreeMap<String, String> {
        self.file.pset(product, set).unwrap_or_else(|| panic!("{} has no {set}", self.file.args(product)[2]))
    }

    fn aggregates(&self) -> Vec<(u32, Vec<u32>)> {
        self.file.of_type("IFCRELAGGREGATES").iter().map(|r| (reference(&self.file.args(*r)[4]), references(&self.file.args(*r)[5]))).collect()
    }

    fn parts(&self, whole: u32) -> Vec<u32> {
        self.aggregates().into_iter().find(|(w, _)| *w == whole).map(|(_, p)| p).unwrap_or_default()
    }
}

/// The meshes of an element's solids with a frame origin (mm) and a turn (degrees): bounding box in the world.
fn mesh_bounds(meshes: &[&Mesh3], origin: [f64; 3], turn_deg: f64) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let (sin, cos) = turn_deg.to_radians().sin_cos();
    for m in meshes {
        for v in &m.vertices {
            let w = [origin[0] + v[0] * cos - v[1] * sin, origin[1] + v[0] * sin + v[1] * cos, origin[2] + v[2]];
            for k in 0..3 {
                lo[k] = lo[k].min(w[k]);
                hi[k] = hi[k].max(w[k]);
            }
        }
    }
    (lo, hi)
}

fn assert_bounds(what: &str, have: ([f64; 3], [f64; 3]), want: ([f64; 3], [f64; 3])) {
    for k in 0..3 {
        assert!((have.0[k] - want.0[k]).abs() < 1e-3 && (have.1[k] - want.1[k]).abs() < 1e-3, "{what}: bounds {have:?}, expected {want:?}");
    }
}

fn site_of(f: &RoadFixture, name: &str) -> ([f64; 3], f64) {
    let s = f.document.find(f.id(name)).unwrap().site.clone().unwrap_or_else(|| panic!("{name} has no site placement"));
    ([s.x, s.y, s.z], s.rotation_deg)
}

fn solids<'a>(f: &'a RoadFixture, name: &str) -> Vec<&'a Mesh3> {
    f.generated[&f.id(name)].solids.iter().collect()
}

fn triangles(meshes: &[&Mesh3]) -> usize {
    meshes.iter().map(|m| m.triangles.len()).sum()
}

#[test]
fn every_module_has_its_entity_and_predefined_type() {
    let x = exported();
    let f = &x.file;
    let of = |entity: &str, predefined: &str| f.of_type(entity).into_iter().filter(|id| x.predefined(*id) == format!(".{predefined}.")).count();
    // Road markings: two lines that the pavement carries, and one beyond it, which is a proxy.
    assert_eq!((f.count("IFCSURFACEFEATURE"), of("IFCSURFACEFEATURE", "LINEMARKING")), (2, 2));
    assert_eq!(f.args(x.product("RM-003"))[4], "'marking'");
    // A side drain, a single culvert and the two barrels of the twin culvert.
    assert_eq!((f.count("IFCPIPESEGMENT"), of("IFCPIPESEGMENT", "GUTTER"), of("IFCPIPESEGMENT", "CULVERT")), (4, 1, 3));
    // Gantries: two assemblies, their legs, booms and foundations, and one sign on the portal.
    assert_eq!(of("IFCELEMENTASSEMBLY", "SIGNALASSEMBLY"), 2);
    assert_eq!((f.count("IFCCOLUMN"), of("IFCCOLUMN", "COLUMN")), (3, 3));
    assert_eq!((f.count("IFCBEAM"), of("IFCBEAM", "BEAM")), (2, 2));
    assert_eq!((f.count("IFCFOOTING"), of("IFCFOOTING", "PAD_FOOTING")), (3, 3));
    assert_eq!(f.count("IFCSIGN"), 1);
    // Lighting: a mast per column with its pole, brackets and luminaires.
    let (mut columns, mut luminaires) = (0, 0);
    for name in ["LZ-01", "LC-900"] {
        let data = afrobim_lighting::LightingData::from_value(&x.f.document.find(x.f.id(name)).unwrap().data).unwrap();
        columns += data.layout.columns.len();
        luminaires += data.luminaire_count();
    }
    assert_eq!((columns, luminaires), (6, 7));
    assert_eq!(of("IFCELEMENTASSEMBLY", "MAST"), columns);
    assert_eq!(f.count("IFCELEMENTASSEMBLY"), 2 + columns);
    assert_eq!(of("IFCMEMBER", "POST"), columns);
    assert_eq!(of("IFCMEMBER", "NOTDEFINED"), luminaires);
    assert_eq!((f.count("IFCLIGHTFIXTURE"), of("IFCLIGHTFIXTURE", "NOTDEFINED")), (luminaires, luminaires));
    // Restraint: the run and its two ends.
    assert_eq!((f.count("IFCRAILING"), of("IFCRAILING", "GUARDRAIL")), (3, 3));
    // Pavement: the pavement (bituminous surfacing on top) and two slabs.
    assert_eq!((f.count("IFCPAVEMENT"), of("IFCPAVEMENT", "FLEXIBLE")), (1, 1));
    assert_eq!((f.count("IFCCOURSE"), of("IFCCOURSE", "PAVEMENT")), (2, 2));
    // Corridor and ground.
    assert_eq!((f.count("IFCBUILDINGELEMENTPROXY"), of("IFCBUILDINGELEMENTPROXY", "USERDEFINED")), (2, 2));
    assert_eq!(f.args(x.product("Test corridor"))[4], "'corridor'");
    assert_eq!((f.count("IFCGEOGRAPHICELEMENT"), of("IFCGEOGRAPHICELEMENT", "TERRAIN")), (1, 1));
    // Groups: the lighting installation and the twin culvert.
    assert_eq!((f.count("IFCGROUP"), f.count("IFCRELASSIGNSTOGROUP")), (2, 2));
    assert_eq!((f.count("IFCROAD"), f.count("IFCSITE"), f.count("IFCALIGNMENT")), (1, 1, 1));
    // Every element is accounted for: the alignment, signs (the one on the gantry), and nothing is a stray proxy.
    assert_eq!(f.count("IFCVOLUMEUNIT"), 0);
    assert!(f.of_type("IFCSIUNIT").iter().any(|u| f.args(*u)[1] == ".VOLUMEUNIT."));
}

#[test]
fn the_spatial_structure_contains_the_roots_and_the_site_contains_the_ground() {
    let x = exported();
    let f = &x.file;
    let road = f.of_type("IFCROAD")[0];
    let site = f.of_type("IFCSITE")[0];
    let contained = f.of_type("IFCRELCONTAINEDINSPATIALSTRUCTURE");
    assert_eq!(contained.len(), 2);
    let in_road = contained.iter().find(|r| reference(&f.args(**r)[5]) == road).unwrap();
    let in_site = contained.iter().find(|r| reference(&f.args(**r)[5]) == site).unwrap();
    assert_eq!(references(&f.args(*in_site)[4]), [x.product("Ground")]);
    let in_road = references(&f.args(*in_road)[4]);
    // Not contained: parts of assemblies and runs, the pavement's courses, markings that adhere.
    let mut parts = BTreeSet::new();
    for (_, p) in x.aggregates() {
        parts.extend(p);
    }
    for id in &in_road {
        assert!(!parts.contains(id), "{} is a part and contained", f.args(*id)[2]);
    }
    for adheres in f.of_type("IFCRELADHERESTOELEMENT") {
        for m in references(&f.args(adheres)[5]) {
            assert!(!in_road.contains(&m), "{} adheres and is contained", f.args(m)[2]);
        }
    }
    for name in ["SD-001", "CU-001", "G-001", "G-002", "VRS-001", "PV-001", "Test corridor"] {
        assert!(in_road.contains(&x.product(name)), "{name} is in the road");
    }
    // Both barrels of the twin culvert are contained, the group is not (it has no position).
    assert!(in_road.contains(&x.named("IFCPIPESEGMENT", "CU-002, barrel 1")) && in_road.contains(&x.named("IFCPIPESEGMENT", "CU-002, barrel 2")));
    assert!(!in_road.contains(&x.product("CU-002")) && f.name(x.product("CU-002")) == "IFCGROUP");
    // The hung sign is a part of its gantry, not contained.
    assert!(!in_road.contains(&x.product("GS-001")));
    assert!(x.parts(x.product("G-001")).contains(&x.product("GS-001")));
    // The markings: two adhere to the pavement; the one beyond it is a proxy contained in the road, with a note.
    let adheres = f.of_type("IFCRELADHERESTOELEMENT");
    assert_eq!(adheres.len(), 1);
    assert_eq!(reference(&f.args(adheres[0])[4]), x.product("PV-001"));
    assert_eq!(references(&f.args(adheres[0])[5]), [x.product("RM-001"), x.product("RM-002")]);
    assert!(in_road.contains(&x.product("RM-003")));
    assert!(x.notes.iter().any(|n| n.starts_with("RM-003: no pavement in the document carries this marking")), "{:#?}", x.notes);
    // Nothing is contained that the road does not hold: every mast is.
    assert_eq!(in_road.iter().filter(|id| f.name(**id) == "IFCELEMENTASSEMBLY").count(), 2 + 6);
}

#[test]
fn products_stand_where_the_modules_put_them() {
    let x = exported();
    let close = |what: &str, have: [f64; 4], want: [f64; 3], turn: f64| {
        for k in 0..3 {
            assert!((have[k] - want[k]).abs() < 1e-3, "{what}: at {have:?}, expected {want:?}");
        }
        let d = (have[3] - turn).rem_euclid(360.0);
        assert!(d.min(360.0 - d) < 1e-6, "{what}: turned {}, expected {turn}", have[3]);
    };
    for name in ["RM-001", "RM-002", "RM-003", "SD-001", "CU-001", "G-001", "G-002", "VRS-001", "PV-001", "Ground"] {
        let (at, turn) = site_of(&x.f, name);
        close(name, x.world(x.product(name)), at, turn);
    }
    // The barrels of the twin culvert share the culvert's placement.
    let (at, turn) = site_of(&x.f, "CU-002");
    for barrel in ["CU-002, barrel 1", "CU-002, barrel 2"] {
        close(barrel, x.world(x.named("IFCPIPESEGMENT", barrel)), at, turn);
    }
    // A sign hung on the gantry is where its site placement puts it, its z the lower edge.
    let (at, turn) = site_of(&x.f, "GS-001");
    close("GS-001", x.world(x.product("GS-001")), at, turn);
    // The parts of the gantry are in its axes.
    let g = x.product("G-001");
    for part in x.parts(g).into_iter().filter(|p| x.file.name(*p) != "IFCSIGN") {
        assert_eq!(x.world(part), x.world(g));
    }
    // Every lighting column stands at its easting, northing and level.
    for name in ["LZ-01", "LC-900"] {
        let data = afrobim_lighting::LightingData::from_value(&x.f.document.find(x.f.id(name)).unwrap().data).unwrap();
        for c in &data.layout.columns {
            let mast = x.named("IFCELEMENTASSEMBLY", &c.mark);
            close(&c.mark, x.world(mast), [c.easting_m * 1000.0, c.northing_m * 1000.0, c.level_m.unwrap_or(0.0) * 1000.0], 0.0);
            let props = x.pset(mast, "AfroBIM_SitePlacement");
            assert_eq!(props["Alignment"], "IFCLABEL('MC10')");
            let station: f64 = props["StationM"].trim_start_matches("IFCREAL(").trim_end_matches(')').parse().unwrap();
            assert!((station - c.station_m).abs() < 1e-5, "{station} vs {}", c.station_m);
        }
    }
    // The terminal and the transition are at the two ends of the barrier line.
    let b = afrobim_restraint::BarrierElement::from_value(&x.f.document.find(x.f.id("VRS-001")).unwrap().data).unwrap();
    let (first, last) = (b.resolved.line.first().unwrap(), b.resolved.line.last().unwrap());
    close("terminal", x.world(x.named("IFCRAILING", "VRS-001, start terminal")), [first.x * 1000.0, first.y * 1000.0, first.z * 1000.0], first.heading_rad.to_degrees());
    close("transition", x.world(x.named("IFCRAILING", "VRS-001, end transition")), [last.x * 1000.0, last.y * 1000.0, last.z * 1000.0], last.heading_rad.to_degrees());
    // The corridor stands at the origin of its built surfaces.
    let built = afrobim_corridor::corridor_of(&x.f.document, x.f.id("Test corridor")).unwrap();
    let m = afrobim_corridor::corridor_meshes(&built);
    close("Test corridor", x.world(x.product("Test corridor")), [m.origin_x_m * 1000.0, m.origin_y_m * 1000.0, 0.0], 0.0);
}

#[test]
fn the_geometry_is_the_modules_solids_where_the_3d_view_draws_them() {
    let x = exported();
    let one = |name: &str, entity_of: &dyn Fn(&Exported) -> Vec<u32>| {
        let meshes = solids(&x.f, name);
        let products = entity_of(&x);
        let faces: Vec<_> = products.iter().flat_map(|p| x.face_sets(*p)).collect();
        assert_eq!(faces.len(), meshes.len(), "{name}: one face set per solid");
        assert_eq!(faces.iter().map(|f| f.1).sum::<usize>(), triangles(&meshes), "{name}: every triangle");
        let (origin, turn) = site_of(&x.f, name);
        assert_bounds(name, x.world_bounds(&products), mesh_bounds(&meshes, origin, turn));
        // Closed where the module's solids are closed.
        for (face, mesh) in faces.iter().zip(&meshes) {
            assert!(face.0.len() <= mesh.vertices.len(), "{name}: welding never adds points");
            assert!(face.2, "{name}: a closed solid is written closed");
            assert!(mesh.volume() > 0.0, "{name}: wound outwards");
        }
    };
    one("RM-001", &|x| vec![x.product("RM-001")]);
    one("RM-002", &|x| vec![x.product("RM-002")]);
    one("SD-001", &|x| vec![x.product("SD-001")]);
    one("CU-001", &|x| vec![x.product("CU-001")]);
    one("CU-002", &|x| vec![x.named("IFCPIPESEGMENT", "CU-002, barrel 1"), x.named("IFCPIPESEGMENT", "CU-002, barrel 2")]);
    one("VRS-001", &|x| vec![x.product("VRS-001")]);
    // The gantry: its parts, in the gantry's own frame.
    for name in ["G-001", "G-002"] {
        one(name, &|x| x.parts(x.product(name)).into_iter().filter(|p| !matches!(x.file.name(*p), "IFCSIGN")).collect());
    }
    // Pavement slabs.
    one("PV-001", &|x| x.parts(x.product("PV-001")));
    // Lighting: the parts of every mast together are all of the element's solids.
    for name in ["LZ-01", "LC-900"] {
        let data = afrobim_lighting::LightingData::from_value(&x.f.document.find(x.f.id(name)).unwrap().data).unwrap();
        let masts: Vec<u32> = data.layout.columns.iter().map(|c| x.named("IFCELEMENTASSEMBLY", &c.mark)).collect();
        let parts: Vec<u32> = masts.iter().flat_map(|m| x.parts(*m)).collect();
        let meshes = solids(&x.f, name);
        assert_eq!(parts.len(), meshes.len(), "{name}: a part per solid");
        let (origin, turn) = site_of(&x.f, name);
        assert_bounds(name, x.world_bounds(&parts), mesh_bounds(&meshes, origin, turn));
        assert_eq!(parts.iter().flat_map(|p| x.face_sets(*p)).map(|f| f.1).sum::<usize>(), triangles(&meshes));
    }
    // The ground is an open surface.
    let ground = x.face_sets(x.product("Ground"));
    assert_eq!(ground.len(), 1);
    assert!(!ground[0].2, "an open surface is written open");
    let meshes = solids(&x.f, "Ground");
    let (origin, turn) = site_of(&x.f, "Ground");
    assert_bounds("Ground", x.world_bounds(&[x.product("Ground")]), mesh_bounds(&meshes, origin, turn));
    // The corridor: the surfaces afrobim-corridor builds, open, at their own origin.
    let built = afrobim_corridor::corridor_of(&x.f.document, x.f.id("Test corridor")).unwrap();
    let m = afrobim_corridor::corridor_meshes(&built);
    let want: Vec<&Mesh3> = [&m.top, &m.cut_slopes, &m.fill_slopes].into_iter().filter(|m| !m.triangles.is_empty()).collect();
    assert_eq!(want.len(), 3, "the test ground gives cut and fill");
    let faces = x.face_sets(x.product("Test corridor"));
    assert_eq!(faces.len(), 3);
    assert!(faces.iter().all(|f| !f.2));
    assert_eq!(faces.iter().map(|f| f.1).sum::<usize>(), triangles(&want));
    // The quads the corridor is built from are written on shared points: about half as many.
    let (written, built): (usize, usize) = (faces.iter().map(|f| f.0.len()).sum(), want.iter().map(|m| m.vertices.len()).sum());
    assert!(written * 3 < built * 2, "{written} points for {built} built");
    assert_bounds("Test corridor", x.world_bounds(&[x.product("Test corridor")]), mesh_bounds(&want, [m.origin_x_m * 1000.0, m.origin_y_m * 1000.0, 0.0], 0.0));
    // The hung sign is a plate on its gantry: 12 triangles.
    assert_eq!(x.face_sets(x.product("GS-001"))[0].1, 12);
}

#[test]
fn standard_property_and_quantity_sets_hold_what_the_modules_measured() {
    let x = exported();
    let close = |text: &str, want: f64| {
        let v: f64 = text.trim_end_matches(')').rsplit('(').next().unwrap().parse().unwrap_or_else(|_| number(text));
        assert!((v - want).abs() <= 1e-6 * want.abs().max(1.0), "{text} vs {want}");
    };
    let element = |name: &str| x.f.document.find(x.f.id(name)).unwrap();

    // Markings: IfcSurfaceFeature LINEMARKING, Pset_RoadMarkingCommon, Pset_MarkingLinesCommon.
    let def = afrobim_markings::MarkingDefinition::from_value(&element("RM-001").data).unwrap();
    let q = afrobim_markings::MarkingQuantities::measure("RM-001", &def).unwrap();
    let rm = x.product("RM-001");
    let common = x.pset(rm, "Pset_RoadMarkingCommon");
    assert_eq!(common["DiagramNumber"], "IFCLABEL('1008.1')");
    assert_eq!(common["MaterialColour"], "IFCLABEL('white')");
    assert_eq!(common["MaterialType"], "IFCLABEL('thermoplastic')");
    let lines = x.pset(rm, "Pset_MarkingLinesCommon");
    assert_eq!(lines["DashedLine"], "IFCBOOLEAN(.T.)");
    assert_eq!(lines["DashedLinePattern"], "IFCLABEL('3000 mm mark, 6000 mm gap')");
    assert_eq!(lines["NominalWidth"], "IFCNONNEGATIVELENGTHMEASURE(100.)");
    assert_eq!(x.pset(x.product("RM-002"), "Pset_MarkingLinesCommon")["DashedLine"], "IFCBOOLEAN(.F.)");
    let qto = x.pset(rm, "Qto_SurfaceFeatureBaseQuantities");
    assert_eq!(qto.keys().map(String::as_str).collect::<Vec<_>>(), ["Area", "Length"]);
    close(&qto["Area"], q.painted_area_m2);
    close(&qto["Length"], q.painted_length_m * 1000.0);
    let own = x.pset(rm, "AfroBIM_Quantities");
    close(&own["Marks"], q.marks as f64);
    close(&own["OverallLength"], q.overall_length_m * 1000.0);
    let id = x.pset(rm, "AfroBIM_RoadMarking");
    assert_eq!(id["Mark"], "IFCLABEL('RM-001')");
    assert_eq!(id["Alignment"], "IFCLABEL('MC10')");
    close(&id["StartStationM"], 50.0);
    close(&id["EndStationM"], 150.0);
    close(&id["OffsetM"], 0.0);
    assert!(id["DesignBasis"].contains("Traffic Signs Manual, Chapter 5"), "{}", id["DesignBasis"]);
    assert!(x.pset(rm, "AfroBIM_Derived").contains_key("Painted length"));

    // Side drain: IfcPipeSegment GUTTER.
    let sd = x.product("SD-001");
    let drain = afrobim_drainage::SideDrainDefinition::from_value(&element("SD-001").data).unwrap();
    let dq = afrobim_drainage::DrainQuantities::measure("SD-001", &drain).unwrap();
    assert_eq!(x.pset(sd, "Pset_PipeSegmentTypeGutter")["IsCovered"], "IFCBOOLEAN(.F.)");
    close(&x.pset(sd, "Qto_PipeSegmentBaseQuantities")["Length"], dq.length_m * 1000.0);
    let own = x.pset(sd, "AfroBIM_Quantities");
    close(&own["Excavation"], dq.excavation_m3);
    close(&own["LiningVolume"], dq.lining_volume_m3);
    assert_eq!(x.pset(sd, "AfroBIM_SideDrain")["PlaceholderDimensions"], "IFCBOOLEAN(.T.)");
    close(&x.pset(sd, "Pset_PipeSegmentOccurrence")["Gradient"], drain.gradient_m_per_m);

    // Culverts: one pipe segment per barrel.
    let cu = x.product("CU-001");
    let culvert = afrobim_drainage::PipeCulvertDefinition::from_value(&element("CU-001").data).unwrap();
    let c = x.pset(cu, "Pset_PipeSegmentTypeCommon");
    close(&c["InnerDiameter"], culvert.internal_diameter_mm);
    close(&c["OuterDiameter"], culvert.internal_diameter_mm + 2.0 * culvert.wall_thickness_mm);
    close(&c["Length"], culvert.length_mm);
    close(&x.pset(cu, "Qto_PipeSegmentBaseQuantities")["Length"], culvert.length_mm);
    let twin = x.named("IFCPIPESEGMENT", "CU-002, barrel 2");
    assert_eq!(x.pset(twin, "AfroBIM_PipeCulvert")["Barrel"], "IFCINTEGER(2)");
    close(&x.pset(twin, "AfroBIM_Quantities")["TotalLength"], 2.0 * culvert.length_mm);
    close(&x.pset(x.named("IFCPIPESEGMENT", "CU-002, barrel 1"), "Pset_PipeSegmentTypeCulvert")["InternalWidth"], culvert.internal_diameter_mm);
    // The material is associated with the barrels that state it.
    assert!(x.file.of_type("IFCRELASSOCIATESMATERIAL").iter().any(|r| references(&x.file.args(*r)[4]).contains(&cu)));

    // Gantry: Qto of its members, the sign's mount.
    let g = x.product("G-001");
    let gantry = afrobim_gantry::GantryDefinition::from_value(&element("G-001").data).unwrap();
    let gq = afrobim_gantry::GantryQuantities::measure("G-001", &gantry).unwrap();
    close(&x.pset(g, "AfroBIM_Quantities")["ClearSpan"], gq.span_or_reach_mm);
    close(&x.pset(g, "AfroBIM_Quantities")["FoundationConcrete"], gq.foundation_concrete_m3);
    assert_eq!(x.pset(g, "Pset_ElementAssemblyCommon")["Reference"], "IFCIDENTIFIER('G-001')");
    let legs: Vec<u32> = x.parts(g).into_iter().filter(|p| x.file.name(*p) == "IFCCOLUMN").collect();
    assert_eq!(legs.len(), 2);
    close(&x.pset(legs[0], "Qto_ColumnBaseQuantities")["Length"], gq.leg_lengths_mm[0]);
    close(&x.pset(legs[1], "Qto_ColumnBaseQuantities")["Length"], gq.leg_lengths_mm[1]);
    let boom = x.parts(g).into_iter().find(|p| x.file.name(*p) == "IFCBEAM").unwrap();
    close(&x.pset(boom, "Qto_BeamBaseQuantities")["Length"], gq.boom_length_mm);
    let foundation = x.parts(g).into_iter().find(|p| x.file.name(*p) == "IFCFOOTING").unwrap();
    close(&x.pset(foundation, "Qto_FootingBaseQuantities")["GrossVolume"], gq.foundation_concrete_m3 / 2.0);
    let mount = x.pset(x.product("GS-001"), "AfroBIM_GantryMount");
    assert_eq!(mount["GantryMark"], "IFCLABEL('G-001')");
    close(&mount["BottomAboveBoomUndersideMm"], 100.0);
    assert_eq!(mount["Serves"], "IFCLABEL('up-station traffic')");

    // Lighting.
    let inst = afrobim_lighting::LightingData::from_value(&element("LZ-01").data).unwrap();
    let group = x.product("LZ-01");
    assert_eq!(x.file.name(group), "IFCGROUP");
    close(&x.pset(group, "AfroBIM_LightingInstallation")["ConnectedLoadW"], inst.connected_load_w());
    let first = &inst.layout.columns[0];
    let mast = x.named("IFCELEMENTASSEMBLY", &first.mark);
    let props = x.pset(mast, "AfroBIM_LightingColumn");
    assert_eq!(props["Side"], format!("IFCLABEL('{}')", first.side.label()));
    close(&props["MountingHeightM"], 10.0);
    let fixture = x.parts(mast).into_iter().find(|p| x.file.name(*p) == "IFCLIGHTFIXTURE").unwrap();
    close(&x.pset(fixture, "Pset_LightFixtureTypeCommon")["TotalWattage"], 90.0);
    assert_eq!(x.pset(fixture, "AfroBIM_LightingPart")["Led"], "IFCBOOLEAN(.T.)");
    let pole = x.parts(mast).into_iter().find(|p| x.file.name(*p) == "IFCMEMBER" && x.predefined(*p) == ".POST.").unwrap();
    assert_eq!(x.pset(pole, "Pset_MemberCommon")["IsExternal"], "IFCBOOLEAN(.T.)");
    // The single column on the alignment has two luminaires, one to each side.
    assert_eq!(x.parts(x.named("IFCELEMENTASSEMBLY", "LC-900")).iter().filter(|p| x.file.name(**p) == "IFCLIGHTFIXTURE").count(), 2);

    // Restraint: Pset_RoadGuardElement and Qto_RailingBaseQuantities.
    let run = x.product("VRS-001");
    let barrier = afrobim_restraint::BarrierElement::from_value(&element("VRS-001").data).unwrap();
    let guard = x.pset(run, "Pset_RoadGuardElement");
    assert_eq!((guard["IsTerminal"].as_str(), guard["IsTransition"].as_str()), ("IFCBOOLEAN(.F.)", "IFCBOOLEAN(.F.)"));
    close(&x.pset(run, "Qto_RailingBaseQuantities")["Length"], barrier.resolved.length_m * 1000.0);
    let props = x.pset(run, "AfroBIM_BarrierRun");
    assert_eq!(props["ContainmentLevel"], "IFCLABEL('N2')");
    assert_eq!(props["StartEnd"], "IFCLABEL('terminal T110')");
    assert_eq!(x.parts(run), [x.named("IFCRAILING", "VRS-001, start terminal"), x.named("IFCRAILING", "VRS-001, end transition")]);
    let terminal = x.pset(x.named("IFCRAILING", "VRS-001, start terminal"), "Pset_RoadGuardElement");
    assert_eq!((terminal["IsTerminal"].as_str(), terminal["IsTransition"].as_str(), terminal["TerminalType"].as_str()), ("IFCBOOLEAN(.T.)", "IFCBOOLEAN(.F.)", "IFCLABEL('T110')"));
    let transition = x.pset(x.named("IFCRAILING", "VRS-001, end transition"), "Pset_RoadGuardElement");
    assert_eq!((transition["IsTerminal"].as_str(), transition["IsTransition"].as_str()), ("IFCBOOLEAN(.F.)", "IFCBOOLEAN(.T.)"));
    assert!(x.file.of_type("IFCRAILING").iter().filter(|r| x.file.args(**r)[6] == "$").count() == 2, "the terminal and the transition have no geometry");

    // Pavement: IfcPavement with its courses.
    let pv = x.product("PV-001");
    let pavement = afrobim_pavement::PavementDefinition::from_value(&element("PV-001").data).unwrap();
    let rows = afrobim_pavement::measure_definition("PV-001", &pavement).unwrap();
    let total: f64 = rows.iter().map(|r| r.volume_m3).sum();
    let qto = x.pset(pv, "Qto_PavementBaseQuantities");
    close(&qto["Length"], 450.0 * 1000.0);
    close(&qto["NetVolume"], total);
    let common = x.pset(pv, "Pset_PavementCommon");
    assert_eq!(common["Reference"], "IFCIDENTIFIER('PV-001')");
    close(&common["NominalThickness"], 200.0);
    close(&common["NominalWidth"], 7000.0);
    let courses = x.parts(pv);
    assert_eq!(courses.len(), 2);
    for (course, row) in courses.iter().zip(rows.iter().filter(|r| r.thickness_mm > 0.0)) {
        let qto = x.pset(*course, "Qto_CourseBaseQuantities");
        close(&qto["Volume"], row.volume_m3);
        close(&qto["Thickness"], row.thickness_mm);
        close(&qto["Width"], row.width_m * 1000.0);
        close(&x.pset(*course, "Pset_CourseCommon")["NominalThickness"], row.thickness_mm);
    }
    assert_eq!(x.pset(pv, "AfroBIM_Pavement")["AreaOnlyLayers"], "IFCLABEL('Carriageway: Seal')");
    // Materials: one IfcMaterial per material text, associated with its courses.
    let materials: Vec<String> = x.file.of_type("IFCMATERIAL").iter().map(|m| x.file.args(*m)[0].clone()).collect();
    assert!(materials.contains(&"'Hot mix asphalt'".to_string()) && materials.contains(&"'Granular base'".to_string()) && materials.contains(&"'concrete pipe (test text)'".to_string()));

    // Corridor: totals of the computation, as quantities.
    let data = afrobim_corridor::corridor_element(&x.f.document, x.f.id("Test corridor")).unwrap();
    let c = data.computed.unwrap();
    let own = x.pset(x.product("Test corridor"), "AfroBIM_Quantities");
    close(&own["TotalCut"], c.total_cut_m3);
    close(&own["TotalFill"], c.total_fill_m3);
    close(&own["Sections"], c.sections as f64);
    assert!(c.total_cut_m3 > 0.0 && c.total_fill_m3 > 0.0);
    assert_eq!(x.pset(x.product("Test corridor"), "AfroBIM_Corridor")["GroundSurface"], "IFCLABEL('Ground')");

    // Notes the modules attached are properties, and the export says what it approximated.
    assert!(x.pset(sd, "AfroBIM_Notes").keys().any(|k| k.to_lowercase().contains("placeholder")));
    assert!(x.notes.iter().any(|n| n.contains("Test corridor") && n.contains("IfcBuildingElementProxy")));
    assert!(x.notes.iter().any(|n| n.contains("VRS-001: the start terminal has no geometry")));
}

#[test]
fn the_alignment_positions_what_was_laid_out_on_it() {
    let x = exported();
    let alignment = x.file.of_type("IFCALIGNMENT")[0];
    let positions = x.file.args(x.file.of_type("IFCRELPOSITIONS")[0]);
    assert_eq!(reference(&positions[4]), alignment);
    let related = references(&positions[5]);
    for name in ["RM-001", "SD-001", "CU-001", "G-001", "VRS-001", "PV-001"] {
        assert!(related.contains(&x.product(name)), "{name}");
    }
    for c in ["LC-001", "LC-900"] {
        assert!(related.contains(&x.named("IFCELEMENTASSEMBLY", c)), "{c}");
    }
    // The terrain and the corridor have no alignment in their placement.
    assert!(!related.contains(&x.product("Ground")));
}

#[test]
fn the_export_is_deterministic_well_formed_and_needs_nothing_generated() {
    let f = road_fixture();
    let a = write_ifc_model(&f.document, &f.inputs(), &options());
    assert_eq!(a, write_ifc_model(&f.document, &f.inputs(), &options()));
    // With nothing generated the exporter runs the modules itself and writes the same file.
    let bare: Vec<IfcElementInput> = f.bare_inputs();
    assert_eq!(a, write_ifc_model(&f.document, &bare, &options()));
    // Instance ids are sequential and no reference dangles.
    let file = StepFile::parse(&a);
    let ids: Vec<u32> = file.instances.keys().copied().collect();
    assert_eq!(ids, (1..=ids.len() as u32).collect::<Vec<_>>());
    for (id, (name, args)) in &file.instances {
        for arg in args {
            let mut rest = arg.as_str();
            while let Some(at) = rest.find('#') {
                let digits: String = rest[at + 1..].chars().take_while(char::is_ascii_digit).collect();
                let target: u32 = digits.parse().unwrap_or_else(|_| panic!("#{id} {name}: bad reference in {arg}"));
                assert!(file.instances.contains_key(&target), "#{id} {name} refers to #{target}");
                rest = &rest[at + 1 + digits.len()..];
            }
        }
    }
    // GlobalIds are unique.
    let mut guids = BTreeSet::new();
    for (name, args) in file.instances.values().filter(|(name, _)| !name.starts_with("IFCPROPERTY") && !name.starts_with("IFCQUANTITY") && !name.starts_with("IFCCOLOUR")) {
        if let Some(a) = args.first().filter(|a| a.len() == 24 && a.starts_with('\'')) {
            let _ = name;
            assert!(guids.insert(a.clone()), "duplicate GlobalId {a}");
        }
    }
    // The file stays small enough to send round: a few hundred kilobytes for this document.
    assert!(a.len() < 3_000_000, "{} bytes", a.len());
}

#[test]
fn a_road_element_without_its_data_is_exported_with_a_note_and_never_panics() {
    let mut f = road_fixture();
    let ids: Vec<_> = f.document.elements().iter().filter(|e| e.category != "alignment" && e.category != "sign").map(|e| e.id).collect();
    for id in ids {
        f.document.find_mut(id).unwrap().data = serde_json::json!({ "nonsense": true });
    }
    f.generated.clear();
    let export = export_ifc_model(&f.document, &f.bare_inputs(), &options());
    let file = StepFile::parse(&export.text);
    assert!(export.notes.iter().any(|n| n.contains("cannot be read")), "{:#?}", export.notes);
    // The elements the exporter can still name are written, without geometry.
    assert!(file.count("IFCPAVEMENT") == 1 && file.count("IFCSURFACEFEATURE") == 0 && file.count("IFCBUILDINGELEMENTPROXY") == 4);
    // And without site placements they stand at the origin, with a note.
    let ids: Vec<_> = f.document.elements().iter().map(|e| e.id).collect();
    for id in ids {
        f.document.find_mut(id).unwrap().site = None;
    }
    let export = export_ifc_model(&f.document, &f.bare_inputs(), &options());
    assert!(export.notes.iter().any(|n| n.contains("no site placement")), "{:#?}", export.notes);
}
