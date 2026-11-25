// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The civil packages in the model export: what is written, what is refused and why.

use afrobim_draw::Mesh3;
use afrobim_ifc::{entity_ok, export_ifc_model, CivilGroup, CivilPackage, CivilPart, CivilSet, CivilValue, GroupKind, IfcElementInput, IfcOptions, CIVIL_CATEGORY};
use afrobim_kernel::{Document, Generated, SitePlacement};

fn cube(size: f64) -> Mesh3 {
    let v = |x: f64, y: f64, z: f64| [x * size, y * size, z * size];
    Mesh3 {
        name: "cube".into(),
        vertices: vec![v(0., 0., 0.), v(1., 0., 0.), v(1., 1., 0.), v(0., 1., 0.), v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)],
        triangles: vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
        colour: [128, 128, 128],
    }
}

fn part(name: &str, entity: &str, predefined: &str, group: Option<usize>) -> CivilPart {
    CivilPart {
        name: name.into(),
        entity: entity.into(),
        predefined: predefined.into(),
        material: Some("test material".into()),
        mesh: Some(0),
        group,
        psets: vec![CivilSet { name: "AfroBIM_Test".into(), items: vec![("Code".into(), CivilValue::Text(name.into()))] }],
        qtos: vec![],
    }
}

fn export(package: &CivilPackage) -> (String, Vec<String>) {
    let mut doc = Document::default();
    let id = doc.add(CIVIL_CATEGORY, "package", afrobim_kernel::Pt::new(0.0, 0.0), 1.0, 1.0, "Civil", serde_json::to_value(package).unwrap());
    doc.find_mut(id).unwrap().site = Some(SitePlacement { x: 360_000_000.0, y: 9_960_000_000.0, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None });
    let generated = Generated { solids: vec![cube(1000.0)], ..Default::default() };
    let e = doc.find(id).unwrap().clone();
    let inputs = [IfcElementInput { element: &e, generated: Some(&generated) }];
    let out = export_ifc_model(&doc, &inputs, &IfcOptions { timestamp: Some(0), ..Default::default() });
    (out.text, out.notes)
}

#[test]
fn only_what_the_schema_has_is_written() {
    assert!(entity_ok("IFCCOURSE", "PAVEMENT") && entity_ok("IFCPAVEMENT", "FLEXIBLE") && entity_ok("IFCEARTHWORKSCUT", "CUT") && entity_ok("IFCEARTHWORKSFILL", "EMBANKMENT"));
    assert!(entity_ok("IFCKERB", "NOTDEFINED") && entity_ok("IFCPIPESEGMENT", "CULVERT") && entity_ok("IFCDISTRIBUTIONCHAMBERELEMENT", "MANHOLE") && entity_ok("IFCCIVILELEMENT", ""));
    assert!(!entity_ok("IFCCOURSE", "FLEXIBLE"), "a type of another entity");
    assert!(!entity_ok("IFCKERB", "BARRIER"), "a type the schema does not give a kerb");
    assert!(!entity_ok("IFCSURFACETREATMENT", "NOTDEFINED"), "an entity the schema does not have");
}

#[test]
fn a_part_outside_the_table_is_reported_and_not_written() {
    let package = CivilPackage {
        title: "t".into(),
        groups: vec![CivilGroup { name: "pavement".into(), kind: GroupKind::Pavement, predefined: "NOTDEFINED".into(), parent: None }],
        parts: vec![part("good", "IFCCOURSE", "PAVEMENT", Some(0)), part("bad", "IFCSURFACETREATMENT", "NOTDEFINED", None), part("also bad", "IFCKERB", "BARRIER", None)],
        ..Default::default()
    };
    let (text, notes) = export(&package);
    assert!(text.contains("IFCCOURSE(") && text.contains("IFCPAVEMENT("), "the good part and its pavement are written");
    assert!(!text.contains("IFCSURFACETREATMENT") && !text.contains("'also bad'") && !text.contains("'bad'"), "the others are not");
    assert!(notes.iter().any(|n| n.contains("bad") && n.contains("not an entity and type")), "{notes:?}");
}

#[test]
fn an_earthworks_cut_with_no_ground_to_void_is_not_written() {
    let package = CivilPackage { title: "t".into(), parts: vec![part("cut", "IFCEARTHWORKSCUT", "CUT", None)], ..Default::default() };
    let (text, notes) = export(&package);
    assert!(!text.contains("IFCEARTHWORKSCUT"), "the schema requires a cut to void an element");
    assert!(notes.iter().any(|n| n.contains("void")), "{notes:?}");
}

#[test]
fn a_quantity_the_template_does_not_have_is_dropped() {
    let mut p = part("course", "IFCCOURSE", "PAVEMENT", None);
    p.qtos = vec![CivilSet { name: "Qto_CourseBaseQuantities".into(), items: vec![("Length".into(), CivilValue::Length(1000.0)), ("Bogus".into(), CivilValue::Length(1.0))] }, CivilSet { name: "Qto_Invented".into(), items: vec![("Length".into(), CivilValue::Length(1.0))] }];
    let (text, notes) = export(&CivilPackage { title: "t".into(), parts: vec![p], ..Default::default() });
    assert!(text.contains("IFCELEMENTQUANTITY") && text.contains("'Length'") && !text.contains("'Bogus'") && !text.contains("Qto_Invented"));
    assert!(notes.iter().any(|n| n.contains("Bogus")) && notes.iter().any(|n| n.contains("Qto_Invented")), "{notes:?}");
}
