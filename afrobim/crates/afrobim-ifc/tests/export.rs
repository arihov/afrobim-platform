// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Checks that need no IFC toolkit: determinism, identifiers, STEP syntax and the triangulation.

mod common;

use std::collections::BTreeSet;

use afrobim_draw::Colour;
use afrobim_ifc::{element_global_id, export_ifc, global_id, tessellate_artwork, write_ifc, IfcElementInput, IfcOptions};
use afrobim_kernel::{Document, ElementId, Pt};

use common::{fixture, options};

fn exported() -> String {
    let f = fixture();
    write_ifc(&f.document, &f.inputs(), &options())
}

/// One `#n=NAME(args);` line of the DATA section.
struct Instance {
    id: u32,
    name: String,
    args: String,
}

/// The DATA section, one instance per line as the exporter writes it.
fn instances(text: &str) -> Vec<Instance> {
    let data = text.split("DATA;\n").nth(1).expect("a DATA section").split("ENDSEC;").next().unwrap();
    data.lines()
        .map(|line| {
            let (id, rest) = line.strip_prefix('#').and_then(|l| l.split_once('=')).unwrap_or_else(|| panic!("not an instance: {line}"));
            let open = rest.find('(').unwrap();
            assert!(rest.ends_with(");"), "{line}");
            Instance { id: id.parse().unwrap(), name: rest[..open].to_string(), args: rest[open + 1..rest.len() - 2].to_string() }
        })
        .collect()
}

/// Every `#n` in an argument list, ignoring the contents of strings.
fn references(args: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut chars = args.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if c == '\'' {
            in_string = !in_string; // a doubled apostrophe toggles twice
        } else if c == '#' && !in_string {
            let mut digits = String::new();
            while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                digits.push(*d);
                chars.next();
            }
            out.push(digits.parse().expect("digits after #"));
        }
    }
    assert!(!in_string, "unterminated string in {args}");
    out
}

/// The string literals of an argument list, still encoded.
fn strings(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut chars = args.chars().peekable();
    while let Some(c) = chars.next() {
        match (&mut current, c) {
            (None, '\'') => current = Some(String::new()),
            (Some(s), '\'') if chars.peek() == Some(&'\'') => {
                s.push_str("''");
                chars.next();
            }
            (Some(_), '\'') => out.push(current.take().unwrap()),
            (Some(s), c) => s.push(c),
            (None, _) => {}
        }
    }
    out
}

#[test]
fn the_same_input_gives_byte_identical_text() {
    let a = exported();
    let b = exported();
    assert_eq!(a, b);

    // Only the header time stamp depends on the clock.
    let f = fixture();
    let later = write_ifc(&f.document, &f.inputs(), &IfcOptions { timestamp: Some(common::TIMESTAMP + 86_400), ..options() });
    let differing: Vec<(&str, &str)> = a.lines().zip(later.lines()).filter(|(x, y)| x != y).collect();
    assert_eq!(differing.len(), 1);
    assert!(differing[0].0.starts_with("FILE_NAME(") && differing[0].0.contains("'2026-11-13T00:00:00'"));
    assert!(differing[0].1.contains("'2026-11-14T00:00:00'"));
}

#[test]
fn the_file_has_the_step_envelope_and_schema() {
    let text = exported();
    assert!(text.is_ascii());
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "ISO-10303-21;");
    assert_eq!(lines[1], "HEADER;");
    assert_eq!(lines[2], "FILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');");
    assert_eq!(
        lines[3],
        "FILE_NAME('Kampala Northern Bypass \\X2\\2014\\X0\\ signs.ifc','2026-11-13T00:00:00',('A. N. O''Engineer'),('AfroBIM'),'afrobim-ifc 0.1.0','AfroBIM test','');"
    );
    assert_eq!(lines[4], "FILE_SCHEMA(('IFC4X3_ADD2'));");
    assert_eq!(lines[5], "ENDSEC;");
    assert_eq!(lines[6], "DATA;");
    assert_eq!(&lines[lines.len() - 2..], ["ENDSEC;", "END-ISO-10303-21;"]);
    assert!(text.ends_with('\n'));
}

#[test]
fn instance_ids_are_sequential_and_no_reference_dangles() {
    let text = exported();
    let all = instances(&text);
    assert!(all.len() > 100);
    for (i, instance) in all.iter().enumerate() {
        assert_eq!(instance.id as usize, i + 1, "ids must run 1, 2, 3...");
        assert!(instance.name.starts_with("IFC") && instance.name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }
    let defined: BTreeSet<u32> = all.iter().map(|i| i.id).collect();
    let mut referenced = BTreeSet::new();
    for instance in &all {
        for r in references(&instance.args) {
            assert!(defined.contains(&r), "#{} refers to undefined #{r}", instance.id);
            assert_ne!(r, instance.id, "#{r} refers to itself");
            referenced.insert(r);
        }
    }
    // Nothing is written that nobody uses: only relationships and styled items sit at the top.
    for instance in all.iter().filter(|i| !referenced.contains(&i.id)) {
        assert!(instance.name.starts_with("IFCREL") || instance.name == "IFCSTYLEDITEM", "#{} {} is unused", instance.id, instance.name);
    }
}

#[test]
fn global_ids_are_well_formed_unique_and_stable() {
    let text = exported();
    let alphabet = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
    let rooted = ["IFCPROJECT", "IFCSITE", "IFCSIGN", "IFCBUILDINGELEMENTPROXY", "IFCPROPERTYSET", "IFCELEMENTQUANTITY"];
    let mut ids = Vec::new();
    for instance in instances(&text) {
        if rooted.contains(&instance.name.as_str()) || instance.name.starts_with("IFCREL") {
            let id = strings(&instance.args).remove(0);
            assert!(instance.args.starts_with(&format!("'{id}'")));
            assert_eq!(id.len(), 22, "{id}");
            assert!(id.chars().all(|c| alphabet.contains(c)), "{id}");
            assert!("0123".contains(&id[..1]), "the first character carries two bits: {id}");
            ids.push(id);
        }
    }
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len(), "GlobalIds must be unique");

    // The element id and the project name fix the GlobalId, whatever else is in the document.
    let f = fixture();
    let project = options().project_name;
    for id in &f.ids {
        let guid = element_global_id(&project, *id);
        assert!(ids.contains(&guid));
    }
    assert_eq!(element_global_id(&project, f.ids[0]), global_id(&format!("{project}/element/{}", f.ids[0].0)));
    assert_ne!(element_global_id(&project, f.ids[0]), element_global_id("Another project", f.ids[0]));
    assert_ne!(element_global_id(&project, f.ids[0]), element_global_id(&project, f.ids[1]));
    // Pinned (and equal to Python uuid.uuid5 + ifcopenshell.guid.compress): a change here changes
    // the identity of every object ever exported.
    assert_eq!(element_global_id("A104 signs", ElementId(7)), "1pC1VfRXfL4x5e0o_jtgZw");
}

#[test]
fn strings_are_escaped_in_the_file() {
    let text = exported();
    assert!(text.contains("IFCLABEL('O''Brien / Z\\X2\\00FC\\X0\\rich \\X2\\2116\\X0\\7 \\\\ rev B')"));
    assert!(text.contains("'REG-002 ''Yield'''"));
    // A value too long for an IfcLabel (255 characters) is an IfcText.
    assert!(text.contains(&format!("IFCTEXT('{}')", "x".repeat(300))));
    assert!(!text.contains(&format!("IFCLABEL('{}')", "x".repeat(300))));
}

#[test]
fn reals_carry_a_decimal_point_and_integers_do_not() {
    let text = exported();
    assert!(text.contains("=IFCCARTESIANPOINT((1700.,0.,0.));"));
    assert!(text.contains("=IFCCARTESIANPOINT((0.,0.,1300.));"));
    assert!(text.contains("IFCQUANTITYLENGTH('Width',$,$,1200.,$)"));
    assert!(text.contains("IFCQUANTITYAREA('GrossArea',$,$,0.96,$)"));
    assert!(text.contains("IFCINTEGER(80)"));
    // The plate of the first sign: 1200 x 3 x 800 mm, front face in the plane y = 0.
    assert!(text.contains(
        "IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1200.,0.,0.),(1200.,0.,800.),(0.,0.,800.),(0.,3.,0.),(1200.,3.,0.),(1200.,3.,800.),(0.,3.,800.)),$)"
    ));
}

#[test]
fn triangle_areas_equal_the_polygon_areas_per_colour_and_layer() {
    let f = fixture();
    for (generated, expected) in f.generated.iter().zip(&f.groups) {
        let Some(generated) = generated else { continue };
        let artwork = tessellate_artwork(&generated.drawing);
        assert_eq!(artwork.meshes.len(), expected.len());
        for (mesh, group) in artwork.meshes.iter().zip(expected) {
            assert_eq!((mesh.colour, mesh.layer), (group.colour, group.layer));
            // Vertices are rounded to a micro-millimetre, which moves the area of the arc band a little.
            assert!((mesh.area() - group.area).abs() < 1e-3, "{:?} layer {}: {} != {}", mesh.colour, mesh.layer, mesh.area(), group.area);
        }
    }
}

#[test]
fn triangle_counts_match_the_outlines() {
    let f = fixture();
    let artwork = tessellate_artwork(&f.generated[0].as_ref().unwrap().drawing);
    let count = |colour: Colour, layer: u32| artwork.meshes.iter().find(|m| m.colour == colour && m.layer == layer).unwrap().triangles.len();
    // A simple polygon of n corners gives n - 2 triangles; each hole adds two.
    assert_eq!(count(Colour::Green, 0), 2);
    // Four bands (2 each), the 7-corner arrow (5), the "O" with its counter (8 + 2 - 2) and the "I" (2).
    assert_eq!(count(Colour::White, 1), 4 * 2 + 5 + 8 + 2);
    assert_eq!(count(Colour::Black, 1), 60); // the arc: 30 quads
    assert_eq!(count(Colour::Black, 2), 2);
    assert_eq!(count(Colour::Yellow, 3), 2);
    assert_eq!((artwork.skipped_symbols, artwork.skipped_degenerate), (1, 0));
}

#[test]
fn every_element_is_exported_and_the_notes_say_what_was_left_out() {
    let f = fixture();
    let export = export_ifc(&f.document, &f.inputs(), &options());
    let all = instances(&export.text);
    let count = |name: &str| all.iter().filter(|i| i.name == name).count();
    assert_eq!(count("IFCSIGN"), 3);
    assert_eq!(count("IFCBUILDINGELEMENTPROXY"), 1);
    assert_eq!(count("IFCPROJECT"), 1);
    assert_eq!(count("IFCSITE"), 1);
    assert_eq!(count("IFCRELAGGREGATES"), 1);
    assert_eq!(count("IFCRELCONTAINEDINSPATIALSTRUCTURE"), 1);
    assert_eq!(count("IFCSHAPEREPRESENTATION"), 4);
    // Plates: 4. Artwork: 5 + 2 + 0 + 1 colour groups.
    assert_eq!(count("IFCTRIANGULATEDFACESET"), 4 + 8);
    assert_eq!(count("IFCSTYLEDITEM"), 12);
    // One style per colour in the whole file: plate grey, green, white, black, yellow, red.
    assert_eq!(count("IFCSURFACESTYLE"), 6);
    assert_eq!(count("IFCOWNERHISTORY"), 0);
    assert_eq!(
        export.notes,
        vec![
            "ADS-001: 1 arc centreline(s) exported as bands 1 mm wide".to_string(),
            "ADS-001: 1 symbol placeholder(s) not exported".to_string(),
            "MK-004: category 'marking' has no IFC mapping; exported as IfcBuildingElementProxy".to_string(),
        ]
    );
}

#[test]
fn odd_input_still_gives_a_well_formed_file() {
    let mut doc = Document::default();
    let flat = doc.add("sign", "FLAT", Pt::new(0.0, 0.0), 0.0, 100.0, "0", serde_json::json!(null));
    let fine = doc.add("sign", "", Pt::new(f64::NAN, 5.0), 100.0, 100.0, "0", serde_json::json!({ "kind": 7, "speed_kph": "fast" }));
    let inputs = [
        IfcElementInput { element: doc.find(flat).unwrap(), generated: None },
        IfcElementInput { element: doc.find(fine).unwrap(), generated: None },
        IfcElementInput { element: doc.find(fine).unwrap(), generated: None },
    ];
    let export = export_ifc(&doc, &inputs, &IfcOptions { timestamp: Some(0), plate_thickness_mm: -1.0, ..IfcOptions::default() });
    let all = instances(&export.text);
    assert_eq!(all.iter().filter(|i| i.name == "IFCSIGN").count(), 2);
    assert_eq!(all.iter().filter(|i| i.name == "IFCSHAPEREPRESENTATION").count(), 1);
    assert!(export.text.contains("IFCLABEL('fast')"));
    assert!(export.text.contains("(100.,3.,100.)"), "a thickness that is not positive falls back to 3 mm");
    assert!(!export.text.contains("NaN"));
    assert_eq!(export.notes.len(), 2);
    assert!(export.notes[0].starts_with("FLAT: no geometry exported"));
    assert!(export.notes[1].contains("more than once"));

    // An empty document is still a valid project with a site.
    let empty = write_ifc(&Document::default(), &[], &IfcOptions { timestamp: Some(0), ..IfcOptions::default() });
    let all = instances(&empty);
    assert!(all.iter().any(|i| i.name == "IFCSITE"));
    assert!(!all.iter().any(|i| i.name == "IFCRELCONTAINEDINSPATIALSTRUCTURE"));
}
