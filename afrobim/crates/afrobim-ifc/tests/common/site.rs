//! A hand-built site model for the model export: signs with posts and foundations of every
//! section and shape, some placed on site and some not, a sign without supports, a post without
//! a sign, an element of an unknown category and an alignment with every kind of element.
//!
//! Every dimension, material and "design basis" here is test data. None of it is an engineering
//! value.

use std::collections::BTreeMap;

use afrobim_alignment::{Alignment, AlignmentBuilder, Turn};
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_ifc::IfcElementInput;
use afrobim_kernel::{Document, ElementId, Generated, Module, Pt, SitePlacement};
use afrobim_supports::{
    add_supports, FoundationDefinition, FoundationModule, FoundationShape, PostDefinition, PostModule, Section, Socket, SupportArrangement,
};
use serde_json::json;

pub const DESIGN_BASIS: &str = "TEST BASIS, not a design: text as the engineer's tool wrote it, O'Brien №7";
pub const STEEL: &str = "steel as specified (test text)";
pub const CONCRETE: &str = "concrete as specified (test text)";

pub struct SiteFixture {
    pub document: Document,
    pub generated: BTreeMap<ElementId, Generated>,
    pub alignment: Alignment,
}

impl SiteFixture {
    pub fn inputs(&self) -> Vec<IfcElementInput<'_>> {
        self.document.elements().iter().map(|element| IfcElementInput { element, generated: self.generated.get(&element.id) }).collect()
    }

    pub fn id(&self, name: &str) -> ElementId {
        self.document.elements().iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no element {name}")).id
    }
}

fn face(width: f64, height: f64, colour: Colour) -> Generated {
    let outline = vec![Pt2::new(0.0, 0.0), Pt2::new(width, 0.0), Pt2::new(width, height), Pt2::new(0.0, height)];
    Generated {
        drawing: Drawing { primitives: vec![Primitive::Fill { outline, colour }], extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) } },
        ..Generated::default()
    }
}

fn post(section: Section, embedment_mm: f64, material: &str) -> PostDefinition {
    // The length is set by add_supports from the arrangement.
    PostDefinition { section, length_mm: 1.0, embedment_mm, material: material.into(), finish: String::new(), placeholder: false }
}

fn block(length_mm: f64, width_mm: f64, depth_mm: f64, top_below_ground_mm: f64, concrete: &str) -> FoundationDefinition {
    FoundationDefinition {
        shape: FoundationShape::Block { length_mm, width_mm, depth_mm },
        top_below_ground_mm,
        concrete: concrete.into(),
        blinding_mm: None,
        socket: None,
        placeholder: false,
    }
}

fn arrangement(mounting_height_mm: f64, post_count: u32, edge_inset_mm: f64, setback_mm: f64, shared_foundation: bool) -> SupportArrangement {
    SupportArrangement { mounting_height_mm, post_count, edge_inset_mm, top_offset_mm: 0.0, setback_mm, shared_foundation }
}

/// The alignment MC01: from (2000 m, 1000 m) on heading 0.3 rad, station 1+000.
/// line 100, spiral 40 to R200 left, arc R200 80 left, spiral 40 back to the tangent, line 50,
/// arc R150 60 right, spiral 30 from R150 right back to the tangent. 400 m in all.
pub fn alignment() -> Alignment {
    AlignmentBuilder::new("MC01", 1000.0, 2000.0, 1000.0, 0.3)
        .line(100.0)
        .spiral(40.0, None, Some(200.0), Turn::Left)
        .arc(200.0, 80.0, Turn::Left)
        .spiral(40.0, Some(200.0), None, Turn::Left)
        .line(50.0)
        .arc(150.0, 60.0, Turn::Right)
        .spiral(30.0, Some(150.0), None, Turn::Right)
        .build()
        .expect("the test alignment")
}

pub fn site_fixture() -> SiteFixture {
    let mut d = Document::default();
    d.standard = "uganda-tsm-2004".to_string();
    let alignment = alignment();

    // A. On the alignment at 1+250, 4.5 m to the right, ground level 1500: two CHS posts, each
    //    on a cylindrical foundation.
    let a = d.add("sign", "ADS-101", Pt::new(0.0, 0.0), 2400.0, 1200.0, "Signs", json!({ "kind": "StackType", "catalog_key": "GFS-B1" }));
    let at = alignment.point_at(1250.0).expect("a station on the alignment");
    let (x, y) = at.offset(4.5);
    d.find_mut(a).unwrap().site = Some(SitePlacement {
        x: x * 1000.0,
        y: y * 1000.0,
        z: 1500.0,
        rotation_deg: at.heading_rad.to_degrees() - 90.0,
        alignment: Some("MC01".into()),
        station_m: Some(1250.0),
        offset_m: Some(4.5),
    });
    let cylinder = FoundationDefinition {
        shape: FoundationShape::Cylinder { diameter_mm: 600.0, depth_mm: 1200.0 },
        top_below_ground_mm: 100.0,
        concrete: CONCRETE.into(),
        blinding_mm: None,
        socket: None,
        placeholder: false,
    };
    let chs = post(Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 }, 1000.0, STEEL);
    let ids = add_supports(&mut d, a, &arrangement(2100.0, 2, 300.0, 60.0, false), &chs, &cylinder).unwrap();
    for id in ids {
        d.find_mut(id).unwrap().properties.insert("design_basis".into(), DESIGN_BASIS.into());
    }

    // The alignment, stored the way the application stores it.
    let m = d.add("alignment", "MC01", Pt::new(20000.0, 0.0), 300000.0, 200000.0, "Alignments", serde_json::to_value(&alignment).unwrap());
    d.find_mut(m).unwrap().site = Some(SitePlacement { x: 2.0e6, y: 1.0e6, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None });

    // B. Placed by coordinates, turned −90°: one RHS post with corner radii on a block with
    //    blinding and a socket. No material stated.
    let b = d.add("sign", "REG-102", Pt::new(3000.0, 0.0), 600.0, 600.0, "Signs", json!({ "kind": "Regulatory" }));
    d.find_mut(b).unwrap().site = Some(SitePlacement { x: -80000.0, y: 45000.0, z: 0.0, rotation_deg: -90.0, alignment: None, station_m: None, offset_m: None });
    let rhs = post(Section::Rhs { width_mm: 80.0, depth_mm: 80.0, wall_mm: 5.0, outer_corner_radius_mm: 10.0, inner_corner_radius_mm: 5.0 }, 600.0, "");
    let mut pad = block(600.0, 400.0, 900.0, 100.0, "");
    pad.blinding_mm = Some(50.0);
    pad.socket = Some(Socket::Round { diameter_mm: 150.0, depth_mm: 500.0 });
    add_supports(&mut d, b, &arrangement(1800.0, 1, 0.0, 0.0, false), &rhs, &pad).unwrap();

    // C. Not placed on site: one I-section post on a block, 100 mm behind the face.
    let c = d.add("sign", "INF-103", Pt::new(5000.0, 2500.0), 900.0, 900.0, "Signs", json!({ "kind": "Information" }));
    let i_section = post(Section::ISection { depth_mm: 200.0, flange_width_mm: 100.0, web_mm: 6.0, flange_mm: 10.0 }, 800.0, STEEL);
    add_supports(&mut d, c, &arrangement(2000.0, 1, 0.0, 100.0, false), &i_section, &block(800.0, 800.0, 1000.0, 0.0, CONCRETE)).unwrap();

    // D. Placed, turned 135°: two channel posts on one shared block.
    let dd = d.add("sign", "GDS-104", Pt::new(7000.0, 0.0), 3000.0, 1500.0, "Signs", json!({ "kind": "Guide" }));
    d.find_mut(dd).unwrap().site = Some(SitePlacement { x: 310000.5, y: -22000.25, z: 250.0, rotation_deg: 135.0, alignment: None, station_m: None, offset_m: None });
    let channel = post(Section::Channel { depth_mm: 150.0, flange_width_mm: 75.0, web_mm: 6.0, flange_mm: 10.0 }, 900.0, STEEL);
    add_supports(&mut d, dd, &arrangement(2200.0, 2, 500.0, 80.0, true), &channel, &block(2600.0, 900.0, 1000.0, 150.0, CONCRETE)).unwrap();

    // E. Not placed: a solid round post, and a foundation whose definition the exporter cannot
    //    read, so that only the solids its module would have generated describe it.
    let e = d.add("sign", "WRN-105", Pt::new(8000.0, 4000.0), 600.0, 800.0, "Signs", json!({ "kind": "Warning" }));
    let round = post(Section::SolidRound { diameter_mm: 90.0 }, 700.0, "");
    let ids = add_supports(&mut d, e, &arrangement(1500.0, 1, 0.0, 0.0, false), &round, &block(500.0, 500.0, 700.0, 0.0, "")).unwrap();
    let bespoke = ids[1];
    d.find_mut(bespoke).unwrap().data = json!({ "type": "bespoke", "note": "not a FoundationDefinition" });

    // G. A sign without supports, placed on site, with a station but no exported supports.
    let g = d.add("sign", "MKR-106", Pt::new(11000.0, 0.0), 300.0, 900.0, "Signs", json!({ "kind": "Marker" }));
    d.find_mut(g).unwrap().site =
        Some(SitePlacement { x: 1000.0, y: 2000.0, z: 300.0, rotation_deg: 45.0, alignment: Some("MC01".into()), station_m: Some(1100.0), offset_m: Some(-3.0) });

    // H. A category without a mapping, not placed.
    d.add("marking", "MK-107", Pt::new(12000.0, 500.0), 400.0, 200.0, "0", json!({ "kind": "Arrow" }));

    // A post that no sign hosts, placed on site.
    let loose = PostDefinition {
        section: Section::SolidRect { width_mm: 80.0, depth_mm: 40.0 },
        length_mm: 2000.0,
        embedment_mm: 500.0,
        material: String::new(),
        finish: "painted (test text)".into(),
        placeholder: true,
    };
    let l = d.add("post", "LOOSE-P1", Pt::new(13000.0, 0.0), 80.0, 2000.0, "Supports", loose.to_value());
    d.find_mut(l).unwrap().site = Some(SitePlacement { x: 5000.0, y: 6000.0, z: 0.0, rotation_deg: 10.0, alignment: None, station_m: None, offset_m: None });

    let mut generated = BTreeMap::new();
    for element in d.elements() {
        let g = match element.category.as_str() {
            "sign" => Some(face(element.width, element.height, Colour::Green)),
            "marking" => Some(face(element.width, element.height, Colour::Yellow)),
            "post" => Some(PostModule::default().generate(element).unwrap()),
            "foundation" if element.id == bespoke => Some(Generated {
                solids: vec![Mesh3::cuboid("Foundation", [0.0, 0.0, -700.0], [500.0, 500.0, 0.0], [0xB4, 0xB4, 0xAC])],
                ..Generated::default()
            }),
            "foundation" => Some(FoundationModule::default().generate(element).unwrap()),
            _ => None,
        };
        if let Some(g) = g {
            generated.insert(element.id, g);
        }
    }
    SiteFixture { document: d, generated, alignment }
}

/// What the model says about every product of the fixture, worked out here from the fixture's
/// own numbers and not by the exporter: class, world position (mm) and turn about +Z (degrees),
/// the volume of its solids (mm³), and the sign whose assembly it belongs to.
///
/// Positions of supports placed on site are their own `SitePlacement`; a sign's face is one
/// mounting height above its site level. The unplaced ones are worked by hand:
///
/// * INF-103 at sheet (5000, 2500), mounting 2000, setback 100: ground point (5000, 0, 500);
///   the post (100 × 200 in plan, axis at 450 along the sign) has its corner at
///   (5000 + 450 − 50, 100 − 100, 500); the block (800 × 800) at (5000 + 450 − 400, 100 − 400, 500).
/// * WRN-105 at sheet (8000, 4000), mounting 1500, setback 0: ground point (8000, 0, 2500);
///   the post (D90, axis at 300) at (8000 + 300 − 45, −45, 2500); the block (500 × 500, from
///   its solids) at (8000 + 300 − 250, −250, 2500).
pub fn expected_products(f: &SiteFixture) -> Vec<serde_json::Value> {
    use std::f64::consts::PI;
    let site = |name: &str, lift: f64| {
        let s = f.document.find(f.id(name)).unwrap().site.clone().unwrap_or_else(|| panic!("{name} has no site placement"));
        [s.x, s.y, s.z + lift, s.rotation_deg]
    };
    let solids = |name: &str| -> Vec<serde_json::Value> {
        f.generated.get(&f.id(name)).map(|g| g.solids.iter().map(|m| json!({ "vertices": m.vertices, "triangles": m.triangles })).collect()).unwrap_or_default()
    };
    let rhs_area = (80.0 * 80.0 - (4.0 - PI) * 100.0) - (70.0 * 70.0 - (4.0 - PI) * 25.0);
    let row = |name: &str, class: &str, predefined: &str, pose: [f64; 4], volume: Option<f64>, sign: Option<&str>, body: Option<&str>, material: Option<&str>| {
        json!({
            "name": name, "class": class, "predefined_type": predefined, "pose": pose, "volume_mm3": volume,
            "assembly_of": sign, "body": body, "material": material, "solids": solids(name),
            "tag": f.id(name).0.to_string(),
        })
    };
    let m = "IfcMember";
    let ft = "IfcFooting";
    let (swept, tess) = (Some("SweptSolid"), Some("Tessellation"));
    vec![
        row("ADS-101", "IfcSign", "PICTORAL", site("ADS-101", 2100.0), None, Some("ADS-101"), tess, None),
        row("ADS-101-P1", m, "POST", site("ADS-101-P1", 0.0), Some(900.0 * PI * 4300.0), Some("ADS-101"), swept, Some(STEEL)),
        row("ADS-101-P2", m, "POST", site("ADS-101-P2", 0.0), Some(900.0 * PI * 4300.0), Some("ADS-101"), swept, Some(STEEL)),
        row("ADS-101-F1", ft, "PAD_FOOTING", site("ADS-101-F1", 0.0), Some(PI / 4.0 * 600.0 * 600.0 * 1200.0), Some("ADS-101"), swept, Some(CONCRETE)),
        row("ADS-101-F2", ft, "PAD_FOOTING", site("ADS-101-F2", 0.0), Some(PI / 4.0 * 600.0 * 600.0 * 1200.0), Some("ADS-101"), swept, Some(CONCRETE)),
        row("REG-102", "IfcSign", "PICTORAL", site("REG-102", 1800.0), None, Some("REG-102"), tess, None),
        row("REG-102-P1", m, "POST", site("REG-102-P1", 0.0), Some(rhs_area * 3000.0), Some("REG-102"), swept, None),
        row("REG-102-F1", ft, "PAD_FOOTING", site("REG-102-F1", 0.0), Some(600.0 * 400.0 * 900.0 + 600.0 * 400.0 * 50.0), Some("REG-102"), swept, None),
        row("INF-103", "IfcSign", "PICTORAL", [5000.0, 0.0, 2500.0, 0.0], None, Some("INF-103"), tess, None),
        row("INF-103-P1", m, "POST", [5400.0, 0.0, 500.0, 0.0], Some(3080.0 * 3700.0), Some("INF-103"), swept, Some(STEEL)),
        row("INF-103-F1", ft, "PAD_FOOTING", [5050.0, -300.0, 500.0, 0.0], Some(800.0 * 800.0 * 1000.0), Some("INF-103"), swept, Some(CONCRETE)),
        row("GDS-104", "IfcSign", "PICTORAL", site("GDS-104", 2200.0), None, Some("GDS-104"), tess, None),
        row("GDS-104-P1", m, "POST", site("GDS-104-P1", 0.0), Some(2280.0 * 4600.0), Some("GDS-104"), swept, Some(STEEL)),
        row("GDS-104-P2", m, "POST", site("GDS-104-P2", 0.0), Some(2280.0 * 4600.0), Some("GDS-104"), swept, Some(STEEL)),
        row("GDS-104-F1", ft, "PAD_FOOTING", site("GDS-104-F1", 0.0), Some(2600.0 * 900.0 * 1000.0), Some("GDS-104"), swept, Some(CONCRETE)),
        row("WRN-105", "IfcSign", "PICTORAL", [8000.0, 0.0, 4000.0, 0.0], None, Some("WRN-105"), tess, None),
        row("WRN-105-P1", m, "POST", [8255.0, -45.0, 2500.0, 0.0], Some(PI / 4.0 * 90.0 * 90.0 * 3000.0), Some("WRN-105"), swept, None),
        row("WRN-105-F1", ft, "PAD_FOOTING", [8050.0, -250.0, 2500.0, 0.0], Some(500.0 * 500.0 * 700.0), Some("WRN-105"), tess, None),
        row("MKR-106", "IfcSign", "PICTORAL", site("MKR-106", 0.0), None, None, tess, None),
        row("MK-107", "IfcBuildingElementProxy", "USERDEFINED", [12000.0, 0.0, 500.0, 0.0], None, None, tess, None),
        row("LOOSE-P1", m, "POST", site("LOOSE-P1", 0.0), Some(80.0 * 40.0 * 2000.0), None, swept, None),
    ]
}

/// The ground point of each assembly (the sign's site placement, or by hand for the unplaced)
/// and how many parts it aggregates.
pub fn expected_assemblies(f: &SiteFixture) -> Vec<(&'static str, [f64; 4], usize)> {
    let site = |name: &str| {
        let s = f.document.find(f.id(name)).unwrap().site.clone().unwrap();
        [s.x, s.y, s.z, s.rotation_deg]
    };
    vec![
        ("ADS-101", site("ADS-101"), 5),
        ("REG-102", site("REG-102"), 3),
        ("INF-103", [5000.0, 0.0, 500.0, 0.0], 3),
        ("GDS-104", site("GDS-104"), 4),
        ("WRN-105", [8000.0, 0.0, 2500.0, 0.0], 3),
    ]
}
