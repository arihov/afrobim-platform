//! The IFC importer on hand-written files with known geometry, units, placements and errors.

mod common;

use afrobim_draw::Mesh3;
use afrobim_ifc::import::*;
use afrobim_ifc::{element_global_id, write_ifc_model};

use common::cases::{cases, L_SHAPE};
use common::roads::road_fixture;
use common::synth::*;
use common::options;

fn volume(m: &Mesh3) -> f64 {
    m.triangles
        .iter()
        .map(|t| {
            let (a, b, c) = (m.vertices[t[0] as usize], m.vertices[t[1] as usize], m.vertices[t[2] as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        })
        .sum()
}

fn vol(p: &ReferenceProduct) -> f64 {
    p.meshes.iter().map(volume).sum()
}

fn find<'a>(m: &'a ReferenceModel, name: &str) -> &'a ReferenceProduct {
    m.products.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no product {name}"))
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn assert_bounds(p: &ReferenceProduct, lo: [f64; 3], hi: [f64; 3], tol: f64) {
    let (l, h) = p.bounds.unwrap_or_else(|| panic!("{} has no bounds", p.name));
    for k in 0..3 {
        assert!(close(l[k], lo[k], tol) && close(h[k], hi[k], tol), "{}: bounds {l:?} {h:?} expected {lo:?} {hi:?}", p.name);
    }
}

fn import_text(text: &str) -> ReferenceModel {
    import_ifc_bytes(text.as_bytes().to_vec(), &ImportOptions::default()).expect("imports")
}

#[test]
fn synthetic_cases_have_the_expected_volume_and_bounds() {
    let m = import_text(&cases(true));
    // A: a sheared prism: volume is area times the extrusion height along the profile normal.
    let a = find(&m, "A-slanted");
    let h = 3000.0 / (1.0f64 + 0.04).sqrt();
    assert!(close(vol(a), 2000.0 * 1000.0 * h, 1.0), "{}", vol(a));
    assert_eq!(a.geometry, GeometryStatus::Read);
    let (lo, hi) = a.bounds.unwrap();
    assert!(close(lo[2], 2300.0, 1e-6) && close(hi[2], 2300.0 + h, 1e-6));
    // B: L profile 450000 mm2 * 400 mm * scale^3 = 1.44e9; bounds worked out by hand with the
    // origin applied before the operator: x' = 5000 - 2y, y' = 2 (x + 1000), z' = 2z.
    let b = find(&m, "B-mapped");
    assert!(close(vol(b), 450_000.0 * 400.0 * 8.0, 1.0), "{}", vol(b));
    assert_bounds(b, [3400.0, 22000.0, 0.0], [5000.0, 24000.0, 800.0], 1e-6);
    // B2: the origin frame is turned +45 degrees, then the operator moves the result.
    let b2 = find(&m, "B2-mapped-origin");
    assert!(close(vol(b2), 450_000.0 * 400.0, 1.0));
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let rot: Vec<[f64; 2]> = L_SHAPE.iter().map(|p| [s * p[0] - s * p[1] - 3000.0, s * p[0] + s * p[1] + 1000.0]).collect();
    let (xmin, xmax) = rot.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    let (ymin, ymax) = rot.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
    assert_bounds(b2, [xmin, 30000.0 + ymin, 500.0], [xmax, 30000.0 + ymax, 900.0], 1e-6);
    // C: a tetrahedron, with and without PnIndex.
    for (name, y0) in [("C-triangulated", 40000.0), ("C2-triangulated-pnindex", 41000.0)] {
        let c = find(&m, name);
        assert!(close(vol(c), 1e9 / 6.0, 1e-3), "{name} {}", vol(c));
        assert_bounds(c, [0.0, y0, 0.0], [1000.0, y0 + 1000.0, 1000.0], 1e-9);
        assert_eq!(c.meshes[0].triangles.len(), 4);
    }
    // D and E2: a plate with a hole, 96e9 mm3 = (1e8 - 4e6) * 1000.
    for (name, y0) in [("D-polygonal-void", 50000.0), ("E2-brep-hole", 70000.0)] {
        let d = find(&m, name);
        assert!(close(vol(d), 9.6e10, 1.0), "{name} {}", vol(d));
        assert_bounds(d, [0.0, y0, 0.0], [10000.0, y0 + 10000.0, 1000.0], 1e-9);
    }
    // E: a cuboid whose top face is written reversed with .F. orientation.
    let e = find(&m, "E-brep-reversed-bound");
    assert!(close(vol(e), 6e9, 1.0), "{}", vol(e));
    assert_bounds(e, [0.0, 60000.0, 0.0], [1000.0, 62000.0, 3000.0], 1e-9);
    // F: a 32-gon turned by 45 degrees has a vertex on each axis, so the box is exact.
    let f = find(&m, "F-circle");
    let area = 0.5 * 32.0 * 500.0 * 500.0 * (std::f64::consts::TAU / 32.0).sin();
    assert!(close(vol(f), area * 2000.0, 1.0));
    assert_bounds(f, [-500.0, 79500.0, 0.0], [500.0, 80500.0, 2000.0], 1e-6);
    let f2 = find(&m, "F2-profile-with-void");
    assert!(close(vol(f2), 1.5e10, 1.0), "{}", vol(f2));
    assert_bounds(f2, [0.0, 90000.0, 0.0], [4000.0, 94000.0, 1000.0], 1e-9);
    // G: four placements deep.
    let g = find(&m, "G-chain");
    let half = 500.0 * (std::f64::consts::FRAC_PI_4.cos() + std::f64::consts::FRAC_PI_4.sin());
    let (cx, cy) = (1000.0 - 500.0 * s - 500.0 * s, 2100.0);
    assert_bounds(g, [cx - half, cy - half, 3000.0], [cx + half, cy + half, 4000.0], 1e-6);
    // Every solid has outward triangles: a positive signed volume.
    for p in &m.products {
        for mesh in &p.meshes {
            assert!(volume(mesh) > 0.0, "{} {}", p.name, mesh.name);
        }
    }
}

#[test]
fn products_without_readable_geometry_are_listed_and_counted() {
    let m = import_text(&cases(true));
    let h = find(&m, "H-box-only");
    assert_eq!(h.geometry, GeometryStatus::BoundingBoxPlaceholder);
    assert_bounds(h, [100.0, 100_200.0, 300.0], [1100.0, 102_200.0, 3300.0], 1e-9);
    let i = find(&m, "I-unread-item");
    assert_eq!(i.geometry, GeometryStatus::NotRead);
    assert_eq!(i.unread_items, vec!["IFCBOOLEANCLIPPINGRESULT".to_string()]);
    assert!(i.meshes.is_empty() && i.bounds.is_none());
    assert_eq!(find(&m, "J-no-representation").geometry, GeometryStatus::NoRepresentation);
    assert_eq!(find(&m, "K-unresolved-placement").geometry, GeometryStatus::PlacementUnresolved);
    let l = find(&m, "L-axis-and-body");
    assert_eq!(l.geometry, GeometryStatus::Read);
    assert_eq!(l.meshes.len(), 1);
    assert_eq!(l.meshes[0].triangles.len(), 12);
    let r = &m.report;
    assert_eq!(r.items_not_read["IFCBOOLEANCLIPPINGRESULT"], 1);
    assert_eq!(r.placements_unresolved["IFCGRIDPLACEMENT"], 1);
    assert_eq!((r.products_placeholder, r.products_not_read, r.products_placement_unresolved), (1, 1, 1));
    assert_eq!((r.products, r.products_spatial), (19, 3));
    assert_eq!(r.by_type["IFCBUILDINGELEMENTPROXY"].products, 16);
    assert_eq!(r.by_type["IFCBUILDINGELEMENTPROXY"].with_geometry, 13);
    assert_eq!(r.length_unit_mm, 1.0);
    assert_eq!(r.schema, "IFC4");
    assert_eq!((r.by_representation["SweptSolid"].read, r.by_representation["SweptSolid"].not_read), (5, 1));
    assert_eq!(r.by_representation["Clipping"].not_read, 1);
    // The placeholder and the unread item are not clash material only when they have no meshes.
    assert!(h.clashable() && !i.clashable());
    assert_eq!(m.clashable().len(), 13);
}

#[test]
fn tree_and_properties() {
    let m = import_text(&cases(false));
    assert_eq!(m.project_name, "Synthetic");
    assert_eq!(m.roots.len(), 1);
    let project = &m.tree[m.roots[0]];
    assert_eq!(project.entity, "IFCPROJECT");
    let site = &m.tree[project.children[0]];
    let building = &m.tree[site.children[0]];
    let storey = &m.tree[building.children[0]];
    assert_eq!((site.entity.as_str(), building.entity.as_str(), storey.entity.as_str()), ("IFCSITE", "IFCBUILDING", "IFCBUILDINGSTOREY"));
    assert_eq!(storey.contained.len(), 12);
    assert!(storey.product.is_some());
    let a = find(&m, "A-slanted");
    assert_eq!(a.container, Some(m.tree.iter().position(|n| n.entity == "IFCBUILDINGSTOREY").unwrap()));
    assert!(m.products.iter().filter(|p| p.is_spatial).count() == 3);
    assert_eq!(a.predefined_type.as_deref(), Some("NOTDEFINED"));
    assert_eq!(a.global_id, guid(1000));
    // Property sets and quantities as text.
    let props: Vec<(String, String, String, String)> = a.properties.iter().map(|p| (p.set.clone(), p.name.clone(), p.value.clone(), p.value_type.clone())).collect();
    let want = |set: &str, name: &str, value: &str, ty: &str| (set.to_string(), name.to_string(), value.to_string(), ty.to_string());
    assert!(props.contains(&want("Pset_Test", "Label", "abc", "IFCLABEL")), "{props:?}");
    assert!(props.contains(&want("Pset_Test", "Len", "12.5", "IFCLENGTHMEASURE")));
    assert!(props.contains(&want("Pset_Test", "Flag", "true", "IFCBOOLEAN")));
    assert!(props.contains(&want("Pset_Test", "Choice", "x, y", "IFCLABEL")));
    assert!(props.contains(&want("Qto_Test", "Length", "2500", "IFCQUANTITYLENGTH")));
    assert_eq!(m.report.properties_not_read["IFCPROPERTYBOUNDEDVALUE"], 1);
    // Properties off.
    let off = import_ifc_bytes(cases(false).into_bytes(), &ImportOptions { properties: false, ..ImportOptions::default() }).unwrap();
    assert!(find(&off, "A-slanted").properties.is_empty());
}

const HEAD: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\nFILE_NAME('u.ifc','2026-01-01T00:00:00',(''),(''),'','','');\n";

/// One unit cube (extruded rectangle 1 x 1, depth 1 in project units) at the origin, in a project
/// with the given unit entities.
fn unit_file(schema: &str, units: &str, extra: &str) -> String {
    format!(
        "{HEAD}FILE_SCHEMA(('{schema}'));\nENDSEC;\nDATA;\n#1=IFCPROJECT('0000000000000000000001',$,'U',$,$,$,$,(#4),#2);\n#2=IFCUNITASSIGNMENT(({units}));\n#3=IFCAXIS2PLACEMENT3D(#5,$,$);\n#4=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-5,#3,$);\n#5=IFCCARTESIANPOINT((0.,0.,0.));\n#6=IFCAXIS2PLACEMENT2D(#7,$);\n#7=IFCCARTESIANPOINT((0.,0.));\n#8=IFCRECTANGLEPROFILEDEF(.AREA.,$,#6,1.,1.);\n#9=IFCDIRECTION((0.,0.,1.));\n#10=IFCEXTRUDEDAREASOLID(#8,#3,#9,1.);\n#11=IFCSHAPEREPRESENTATION(#4,'Body','SweptSolid',(#10));\n#12=IFCPRODUCTDEFINITIONSHAPE($,$,(#11));\n#13=IFCLOCALPLACEMENT($,#3);\n#14=IFCBUILDINGELEMENTPROXY('0000000000000000000002',$,'Cube',$,$,#13,#12,$,$);\n{extra}ENDSEC;\nEND-ISO-10303-21;\n"
    )
}

#[test]
fn length_units_are_converted_to_millimetres() {
    let side = |text: &str| {
        let m = import_text(text);
        let (lo, hi) = find(&m, "Cube").bounds.unwrap();
        (hi[0] - lo[0], m.report.length_unit_mm, m.report.length_unit.clone())
    };
    let metre = side(&unit_file("IFC4", "#20", "#20=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n"));
    assert_eq!((metre.0, metre.1), (1000.0, 1000.0));
    let milli = side(&unit_file("IFC4", "#20", "#20=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n"));
    assert_eq!((milli.0, milli.2.as_str()), (1.0, "milli metre"));
    let centi = side(&unit_file("IFC2X3", "#20", "#20=IFCSIUNIT(*,.LENGTHUNIT.,.CENTI.,.METRE.);\n"));
    assert!(close(centi.0, 10.0, 1e-12));
    let kilo = side(&unit_file("IFC4", "#20", "#20=IFCSIUNIT(*,.LENGTHUNIT.,.KILO.,.METRE.);\n"));
    assert!(close(kilo.0, 1_000_000.0, 1e-6));
    // A conversion based unit: the inch is 0.0254 m, given through an IfcMeasureWithUnit.
    let inch = side(&unit_file(
        "IFC4",
        "#20,#25",
        "#20=IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.);\n#21=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n#22=IFCDIMENSIONALEXPONENTS(1,0,0,0,0,0,0);\n#23=IFCMEASUREWITHUNIT(IFCLENGTHMEASURE(0.0254),#21);\n#25=IFCCONVERSIONBASEDUNIT(#22,.LENGTHUNIT.,'INCH',#23);\n",
    ));
    assert!(close(inch.0, 25.4, 1e-9) && inch.2 == "INCH", "{inch:?}");
    // No length unit at all: metres assumed, with a warning.
    let none = import_text(&unit_file("IFC4", "#20", "#20=IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.);\n"));
    assert_eq!(none.report.length_unit_mm, 1000.0);
    assert!(none.report.warnings.iter().any(|w| w.contains("no length unit")));
}

#[test]
fn map_conversion_is_applied_only_on_request() {
    let extra = "#20=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n#21=IFCPROJECTEDCRS('EPSG:32636',$,$,$,$,$,$);\n#22=IFCMAPCONVERSION(#4,#21,500000.,6000000.,100.,0.,1.,$);\n";
    let text = unit_file("IFC4", "#20", extra);
    let plain = import_text(&text);
    assert_bounds(find(&plain, "Cube"), [-0.5, -0.5, 0.0], [0.5, 0.5, 1.0], 1e-12);
    let mc = plain.map_conversion.clone().unwrap();
    assert_eq!((mc.eastings, mc.northings, mc.orthogonal_height, mc.x_axis_abscissa, mc.x_axis_ordinate, mc.scale), (500000.0, 6000000.0, 100.0, 0.0, 1.0, 1.0));
    let applied = import_ifc_bytes(text.into_bytes(), &ImportOptions { apply_map_conversion: true, ..ImportOptions::default() }).unwrap();
    // The local X axis points along map north: (x, y) -> (E - y, N + x).
    assert_bounds(find(&applied, "Cube"), [499_999.5, 5_999_999.5, 100.0], [500_000.5, 6_000_000.5, 101.0], 1e-6);
}

#[test]
fn errors_are_reported_not_panicked() {
    let bad = |t: &str| import_ifc_bytes(t.as_bytes().to_vec(), &ImportOptions::default()).unwrap_err();
    assert!(matches!(bad("not an ifc file"), ImportError::Step(_)));
    assert!(matches!(bad(&unit_file("IFC5", "#20", "")), ImportError::UnsupportedSchema(_)));
    let cut = unit_file("IFC4", "#20", "#20=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n");
    assert!(matches!(bad(&cut[..cut.len() / 2]), ImportError::Step(_)));
    assert!(import_ifc(std::path::Path::new("/nonexistent/x.ifc"), &ImportOptions::default()).is_err());
    // IFC2X3_TC1 and an unknown IFC4 add-on name are read with a note, not refused.
    let m = import_text(&unit_file("IFC4X1", "#20", "#20=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n"));
    assert!(m.report.warnings.iter().any(|w| w.contains("IFC4X1")));
}

#[test]
fn the_own_export_imports_with_the_exporters_global_ids() {
    let f = road_fixture();
    let text = write_ifc_model(&f.document, &f.inputs(), &options());
    let m = import_text(&text);
    assert_eq!(m.schema, "IFC4X3_ADD2");
    assert_eq!(m.report.length_unit_mm, 1.0);
    let project = options().project_name;
    let mut matched = 0;
    for e in f.document.elements() {
        let id = element_global_id(&project, e.id);
        if let Some(p) = m.find_global_id(&id) {
            matched += 1;
            assert_eq!(p.name, e.name, "name of {id}");
        }
    }
    assert!(matched >= 10, "only {matched} elements found by their GlobalId");
    // Everything exported with a solid comes back with meshes.
    assert!(m.products.iter().filter(|p| !p.is_spatial && p.geometry == GeometryStatus::Read).count() >= 10);
    assert!(m.products.iter().any(|p| p.entity == "IFCROAD" && p.is_spatial));
}

#[test]
fn indexed_poly_curve_profiles_read_straight_segments_only() {
    let mut b = B::new("IFC4", "IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)");
    let pl = b.local_placement(None, [0.0; 3], None);
    let list = b.add("IFCCARTESIANPOINTLIST2D(((0.,0.),(1000.,0.),(1000.,500.),(0.,500.)))".to_string());
    for (name, segs) in [
        ("all-points", "$"),
        ("line-segments", "(IFCLINEINDEX((1,2)),IFCLINEINDEX((2,3,4)),IFCLINEINDEX((4,1)))"),
        ("with-arc", "(IFCLINEINDEX((1,2)),IFCARCINDEX((2,3,4)),IFCLINEINDEX((4,1)))"),
    ] {
        let c = b.add(format!("IFCINDEXEDPOLYCURVE(#{list},{segs},$)"));
        let prof = b.add(format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{c})"));
        let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 200.0);
        let sh = b.shape("Body", "SweptSolid", &[ex]);
        b.proxy(name, pl, Some(sh));
    }
    let m = import_text(&b.finish());
    for name in ["all-points", "line-segments"] {
        assert!(close(vol(find(&m, name)), 1e8, 1e-3), "{name}");
    }
    let arc = find(&m, "with-arc");
    assert_eq!(arc.geometry, GeometryStatus::NotRead);
    assert!(arc.unread_items.iter().any(|u| u.contains("IFCARCINDEX")), "{:?}", arc.unread_items);
}
