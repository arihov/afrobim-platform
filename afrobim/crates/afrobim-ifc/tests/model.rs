//! The model export (`export_ifc_model`) checked without an IFC toolkit: what a plain document
//! gives, entity counts, placements, relationships, units, swept-solid parameters, property
//! sets, the alignment, and the fallbacks.

mod common;

use std::collections::BTreeSet;
use std::f64::consts::PI;

use afrobim_draw::Mesh3;
use afrobim_ifc::{element_global_id, export_ifc, export_ifc_model, global_id, write_ifc, write_ifc_model, IfcElementInput};
use afrobim_kernel::{Document, Generated, Pt, SitePlacement};
use serde_json::json;

use common::site::{expected_assemblies, expected_products, site_fixture, SiteFixture, CONCRETE, DESIGN_BASIS, STEEL};
use common::step::{number, numbers, reference, references, StepFile};
use common::{fixture, options};

fn exported() -> (SiteFixture, StepFile, Vec<String>) {
    let f = site_fixture();
    let export = export_ifc_model(&f.document, &f.inputs(), &options());
    let file = StepFile::parse(&export.text);
    (f, file, export.notes)
}

fn product(f: &SiteFixture, file: &StepFile, name: &str) -> u32 {
    file.by_guid(&element_global_id(&options().project_name, f.id(name)))
}

fn assembly(f: &SiteFixture, file: &StepFile, sign: &str) -> u32 {
    file.by_guid(&global_id(&format!("{}/element/{}/assembly", options().project_name, f.id(sign).0)))
}

/// The placement of an IfcProduct: attribute 6.
fn placement(file: &StepFile, product: u32) -> u32 {
    reference(&file.args(product)[5])
}

fn assert_pose(what: &str, have: [f64; 4], want: [f64; 4]) {
    for k in 0..3 {
        assert!((have[k] - want[k]).abs() < 1e-3, "{what}: at {have:?}, expected {want:?}");
    }
    let turn = (have[3] - want[3]).rem_euclid(360.0);
    assert!(turn.min(360.0 - turn) < 1e-7, "{what}: turned {} degrees, expected {}", have[3], want[3]);
}

/// FNV-1a of a text.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

#[test]
fn a_document_without_supports_or_site_gives_what_export_ifc_gives() {
    let f = fixture();
    let old = export_ifc(&f.document, &f.inputs(), &options());
    let new = export_ifc_model(&f.document, &f.inputs(), &options());
    assert_eq!(old.text, new.text);
    assert_eq!(old.notes, new.notes);
    // And both are what the exporter wrote before it knew about supports: the DATA section of
    // the fixture as exported by the previous version (176 instances).
    let data = old.text.split("DATA;\n").nth(1).unwrap().split("ENDSEC;").next().unwrap();
    assert_eq!(data.lines().count(), 176);
    assert_eq!(fnv(data), 0x192e_91c2_5503_a822, "the plain sign export changed");
    assert!(!old.text.contains("IFCROAD") && !old.text.contains("VOLUMEUNIT"));
}

#[test]
fn export_ifc_itself_ignores_site_and_supports() {
    // The old entry point keeps its behaviour on the site model too: everything on the sheet,
    // supports as proxies, no road. That is what keeps the application compiling and working
    // until it switches.
    let f = site_fixture();
    let text = write_ifc(&f.document, &f.inputs(), &options());
    let file = StepFile::parse(&text);
    assert_eq!(file.count("IFCROAD") + file.count("IFCMEMBER") + file.count("IFCFOOTING") + file.count("IFCELEMENTASSEMBLY") + file.count("IFCALIGNMENT"), 0);
    assert_eq!(file.count("IFCSIGN"), 6);
    assert_eq!(file.count("IFCBUILDINGELEMENTPROXY"), f.document.elements().len() - 6);
}

#[test]
fn the_model_export_is_deterministic_and_well_formed() {
    let f = site_fixture();
    let a = write_ifc_model(&f.document, &f.inputs(), &options());
    let b = write_ifc_model(&site_fixture().document, &f.inputs(), &options());
    assert_eq!(a, b);
    let file = StepFile::parse(&a);
    // Sequential ids, no dangling or forward-only garbage references, unique GlobalIds.
    let ids: Vec<u32> = file.instances.keys().copied().collect();
    assert_eq!(ids, (1..=ids.len() as u32).collect::<Vec<_>>());
    let mut guids = BTreeSet::new();
    for (id, (name, args)) in &file.instances {
        for arg in args {
            let mut rest = arg.as_str();
            if rest.starts_with('\'') {
                continue;
            }
            while let Some(at) = rest.find('#') {
                let digits: String = rest[at + 1..].chars().take_while(char::is_ascii_digit).collect();
                let target: u32 = digits.parse().unwrap_or_else(|_| panic!("#{id} {name}: bad reference in {arg}"));
                assert!(file.instances.contains_key(&target), "#{id} {name} refers to missing #{target}");
                rest = &rest[at + 1 + digits.len()..];
            }
        }
        let rooted = name.starts_with("IFCREL") || args.len() > 3 && args[0].len() == 24 && args[0].starts_with('\'') && args[1] == "$";
        if rooted {
            assert!(guids.insert(args[0].clone()), "GlobalId {} used twice", args[0]);
        }
    }
    assert!(guids.len() > 150);
}

#[test]
fn every_kind_of_entity_is_written_the_expected_number_of_times() {
    let (_, file, _) = exported();
    let counts = [
        ("IFCPROJECT", 1),
        ("IFCSITE", 1),
        ("IFCROAD", 1),
        ("IFCSIGN", 6),
        ("IFCELEMENTASSEMBLY", 5),
        ("IFCMEMBER", 8),
        ("IFCFOOTING", 6),
        ("IFCBUILDINGELEMENTPROXY", 1),
        // Posts: 8. Foundations: 2 cylinders, 3 blocks and one blinding; the unreadable one is tessellated.
        ("IFCEXTRUDEDAREASOLID", 14),
        ("IFCCIRCLEHOLLOWPROFILEDEF", 2),
        ("IFCRECTANGLEHOLLOWPROFILEDEF", 1),
        ("IFCISHAPEPROFILEDEF", 1),
        ("IFCUSHAPEPROFILEDEF", 2),
        ("IFCCIRCLEPROFILEDEF", 3),
        ("IFCRECTANGLEPROFILEDEF", 4),
        ("IFCMATERIAL", 2),
        ("IFCRELASSOCIATESMATERIAL", 2),
        ("IFCALIGNMENT", 1),
        ("IFCALIGNMENTHORIZONTAL", 1),
        ("IFCALIGNMENTSEGMENT", 8),
        ("IFCALIGNMENTHORIZONTALSEGMENT", 8),
        ("IFCREFERENT", 1),
        ("IFCINDEXEDPOLYCURVE", 1),
        ("IFCRELNESTS", 3),
        ("IFCRELPOSITIONS", 1),
        ("IFCRELCONTAINEDINSPATIALSTRUCTURE", 1),
        // Project > site, site > road, five assemblies, project > alignment.
        ("IFCRELAGGREGATES", 8),
        ("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", 2),
        ("IFCOWNERHISTORY", 0),
    ];
    for (name, count) in counts {
        assert_eq!(file.count(name), count, "{name}");
    }
}

#[test]
fn units_are_millimetres_square_and_cubic_metres_and_radians() {
    let (_, file, _) = exported();
    let units: Vec<String> = file.of_type("IFCSIUNIT").iter().map(|id| file.args(*id).join(",")).collect();
    assert_eq!(units, ["*,.LENGTHUNIT.,.MILLI.,.METRE.", "*,.AREAUNIT.,$,.SQUARE_METRE.", "*,.VOLUMEUNIT.,$,.CUBIC_METRE.", "*,.PLANEANGLEUNIT.,$,.RADIAN."]);
    let assignment = file.of_type("IFCUNITASSIGNMENT");
    assert_eq!(references(&file.args(assignment[0])[0]).len(), 4);
    // Without supports there are no volume quantities and no volume unit.
    let mut d = Document::default();
    let id = d.add("sign", "S", Pt::new(0.0, 0.0), 600.0, 600.0, "0", json!({}));
    d.find_mut(id).unwrap().site = Some(SitePlacement { x: 1.0, y: 2.0, z: 3.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None });
    let text = write_ifc_model(&d, &[IfcElementInput { element: d.find(id).unwrap(), generated: None }], &options());
    assert!(text.contains("IFCROAD") && !text.contains("VOLUMEUNIT"));
}

#[test]
fn every_product_stands_where_the_model_puts_it() {
    let (f, file, _) = exported();
    for e in expected_products(&f) {
        let name = e["name"].as_str().unwrap();
        let id = product(&f, &file, name);
        let class = e["class"].as_str().unwrap().to_uppercase();
        assert_eq!(file.name(id), class, "{name}");
        assert_eq!(file.args(id).last().unwrap(), &format!(".{}.", e["predefined_type"].as_str().unwrap()), "{name}");
        let pose: Vec<f64> = e["pose"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_pose(name, file.world(placement(&file, id)), [pose[0], pose[1], pose[2], pose[3]]);
    }
    for (sign, pose, _) in expected_assemblies(&f) {
        assert_pose(sign, file.world(placement(&file, assembly(&f, &file, sign))), pose);
    }
}

#[test]
fn site_positions_follow_the_convention_of_the_3d_view() {
    // The sign ADS-101: rotation = heading − 90°, local +X along the face, the face one mounting
    // height above the ground point; a post axis at u along the face and 60 behind it.
    let (f, file, _) = exported();
    let s = f.document.find(f.id("ADS-101")).unwrap().site.clone().unwrap();
    let at = f.alignment.point_at(1250.0).unwrap();
    assert!((s.rotation_deg - (at.heading_rad.to_degrees() - 90.0)).abs() < 1e-12);
    let (sin, cos) = s.rotation_deg.to_radians().sin_cos();
    // Post 1: axis 300 along and 60 behind, so its 100 × 100 plan corner is at (250, 10).
    let want = [s.x + 250.0 * cos - 10.0 * sin, s.y + 250.0 * sin + 10.0 * cos, 1500.0, s.rotation_deg];
    assert_pose("ADS-101-P1", file.world(placement(&file, product(&f, &file, "ADS-101-P1"))), want);
    // Foundation 2: axis 2100 along, 600 across, so its corner is at (1800, −240).
    let want = [s.x + 1800.0 * cos + 240.0 * sin, s.y + 1800.0 * sin - 240.0 * cos, 1500.0, s.rotation_deg];
    assert_pose("ADS-101-F2", file.world(placement(&file, product(&f, &file, "ADS-101-F2"))), want);
    // The sign: the same point, 2100 up.
    assert_pose("ADS-101", file.world(placement(&file, product(&f, &file, "ADS-101"))), [s.x, s.y, 3600.0, s.rotation_deg]);
    // In the file the parts are placed in the assembly's axes: the sign straight above its origin.
    let sign_placement = file.args(placement(&file, product(&f, &file, "ADS-101")));
    let axes = file.args(reference(&sign_placement[1]));
    assert_eq!(numbers(&file.args(reference(&axes[0]))[0]), [0.0, 0.0, 2100.0]);
    assert_eq!((axes[1].as_str(), axes[2].as_str()), ("$", "$"));
    let post_axes = file.args(reference(&file.args(placement(&file, product(&f, &file, "ADS-101-P1")))[1]));
    let local = numbers(&file.args(reference(&post_axes[0]))[0]);
    assert!((local[0] - 250.0).abs() < 1e-6 && (local[1] - 10.0).abs() < 1e-6 && local[2].abs() < 1e-6, "{local:?}");
}

#[test]
fn assemblies_aggregate_and_only_roots_are_contained() {
    let (f, file, _) = exported();
    let road = file.of_type("IFCROAD")[0];
    let site = file.of_type("IFCSITE")[0];
    let project = file.of_type("IFCPROJECT")[0];
    let aggregates: Vec<(u32, Vec<u32>)> = file.of_type("IFCRELAGGREGATES").iter().map(|r| (reference(&file.args(*r)[4]), references(&file.args(*r)[5]))).collect();
    assert!(aggregates.contains(&(project, vec![site])));
    assert!(aggregates.contains(&(site, vec![road])));
    let alignment = file.of_type("IFCALIGNMENT")[0];
    assert!(aggregates.contains(&(project, vec![alignment])), "the alignment is aggregated by the project");

    let mut parts_of_all = BTreeSet::new();
    for (sign, _, count) in expected_assemblies(&f) {
        let whole = assembly(&f, &file, sign);
        assert_eq!(file.args(whole).last().unwrap(), ".SIGNALASSEMBLY.");
        assert_eq!(file.args(whole)[6], "$", "an assembly has no representation of its own");
        let parts = &aggregates.iter().find(|(w, _)| *w == whole).unwrap_or_else(|| panic!("{sign} aggregates nothing")).1;
        assert_eq!(parts.len(), count, "{sign}");
        assert_eq!(parts[0], product(&f, &file, sign), "the sign comes first");
        for part in parts {
            // Placed relative to the assembly's placement.
            assert_eq!(reference(&file.args(placement(&file, *part))[0]), placement(&file, whole), "{sign}");
            parts_of_all.insert(*part);
        }
        // The assembly itself is placed relative to the road.
        assert_eq!(reference(&file.args(placement(&file, whole))[0]), placement(&file, road));
    }
    let expected_parts: BTreeSet<u32> = expected_products(&f).iter().filter(|e| !e["assembly_of"].is_null()).map(|e| product(&f, &file, e["name"].as_str().unwrap())).collect();
    assert_eq!(parts_of_all, expected_parts);
    // GDS-104 shares one foundation between two posts.
    let gds = &aggregates.iter().find(|(w, _)| *w == assembly(&f, &file, "GDS-104")).unwrap().1;
    assert_eq!(gds.iter().filter(|p| file.name(**p) == "IFCFOOTING").count(), 1);

    let contained = file.of_type("IFCRELCONTAINEDINSPATIALSTRUCTURE");
    let args = file.args(contained[0]);
    assert_eq!(reference(&args[5]), road);
    let mut roots: Vec<u32> = ["ADS-101", "REG-102", "INF-103", "GDS-104", "WRN-105"].iter().map(|s| assembly(&f, &file, s)).collect();
    roots.extend(["MKR-106", "MK-107", "LOOSE-P1"].iter().map(|n| product(&f, &file, n)));
    assert_eq!(references(&args[4]), roots, "the road contains the assemblies and the stand-alone elements, in document order");
    assert!(!references(&args[4]).contains(&alignment), "an alignment is never contained");

    // The alignment positions what was placed from it.
    let positions = file.args(file.of_type("IFCRELPOSITIONS")[0]);
    assert_eq!(reference(&positions[4]), alignment);
    assert_eq!(references(&positions[5]), [assembly(&f, &file, "ADS-101"), product(&f, &file, "MKR-106")]);
}

#[test]
fn posts_and_foundations_are_swept_solids_of_their_definitions() {
    let (f, file, _) = exported();
    let solids = |name: &str| -> Vec<(String, Vec<String>, [f64; 3], f64)> {
        let shape = reference(&file.args(product(&f, &file, name))[6]);
        let representation = references(&file.args(shape)[2])[0];
        assert_eq!(file.args(representation)[2], "'SweptSolid'", "{name}");
        references(&file.args(representation)[3])
            .iter()
            .map(|solid| {
                let a = file.args(*solid);
                assert_eq!(file.name(*solid), "IFCEXTRUDEDAREASOLID");
                assert_eq!(numbers(&file.args(reference(&a[2]))[0]), [0.0, 0.0, 1.0]);
                let profile = reference(&a[0]);
                let at = numbers(&file.args(reference(&file.args(reference(&a[1]))[0]))[0]);
                (file.name(profile).to_string(), file.args(profile)[3..].to_vec(), [at[0], at[1], at[2]], number(&a[3]))
            })
            .collect()
    };
    let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // CHS D100 t10, 4300 long, 1000 in the ground: radius 50, wall 10, from z = −1000.
    assert_eq!(solids("ADS-101-P1"), [("IFCCIRCLEHOLLOWPROFILEDEF".to_string(), strings(&["50.", "10."]), [50.0, 50.0, -1000.0], 4300.0)]);
    // Cylinder D600 × 1200 with its top 100 below ground.
    assert_eq!(solids("ADS-101-F1"), [("IFCCIRCLEPROFILEDEF".to_string(), strings(&["300."]), [300.0, 300.0, -1300.0], 1200.0)]);
    // RHS 80 × 80 t5 with inner fillet 5 and outer fillet 10, in that order.
    assert_eq!(solids("REG-102-P1"), [("IFCRECTANGLEHOLLOWPROFILEDEF".to_string(), strings(&["80.", "80.", "5.", "5.", "10."]), [40.0, 40.0, -600.0], 3000.0)]);
    // Block 600 × 400 × 900 with its top 100 below ground, and 50 of blinding under it.
    let block = ("IFCRECTANGLEPROFILEDEF".to_string(), strings(&["600.", "400."]));
    assert_eq!(solids("REG-102-F1"), [(block.0.clone(), block.1.clone(), [300.0, 200.0, -1000.0], 900.0), (block.0, block.1, [300.0, 200.0, -1050.0], 50.0)]);
    // I: overall width 100, overall depth 200, web 6, flange 10.
    assert_eq!(solids("INF-103-P1"), [("IFCISHAPEPROFILEDEF".to_string(), strings(&["100.", "200.", "6.", "10.", "$", "$", "$"]), [50.0, 100.0, -800.0], 3700.0)]);
    // Channel: depth 150, flange width 75, web 6, flange 10.
    assert_eq!(solids("GDS-104-P1"), [("IFCUSHAPEPROFILEDEF".to_string(), strings(&["150.", "75.", "6.", "10.", "$", "$", "$"]), [37.5, 75.0, -900.0], 4600.0)]);
    assert_eq!(solids("WRN-105-P1"), [("IFCCIRCLEPROFILEDEF".to_string(), strings(&["45."]), [45.0, 45.0, -700.0], 3000.0)]);
    assert_eq!(solids("LOOSE-P1"), [("IFCRECTANGLEPROFILEDEF".to_string(), strings(&["80.", "40."]), [40.0, 20.0, -500.0], 2000.0)]);
}

#[test]
fn quantities_come_from_the_definitions() {
    let (f, file, _) = exported();
    let close = |text: &str, want: f64| (number(text) - want).abs() <= 1e-9 * want.abs().max(1.0);
    // Steel of a CHS D100 t10 × 4300: 900π mm² × 4300 mm.
    let q = file.pset(product(&f, &file, "ADS-101-P1"), "Qto_MemberBaseQuantities").unwrap();
    assert_eq!(q.keys().map(String::as_str).collect::<Vec<_>>(), ["CrossSectionArea", "Length", "NetVolume", "OuterSurfaceArea"]);
    assert!(close(&q["Length"], 4300.0));
    assert!(close(&q["CrossSectionArea"], 900.0 * PI / 1e6), "{}", q["CrossSectionArea"]);
    assert!(close(&q["OuterSurfaceArea"], PI * 100.0 * 4300.0 / 1e6));
    assert!(close(&q["NetVolume"], 900.0 * PI * 4300.0 / 1e9), "{}", q["NetVolume"]);
    // Block 600 × 400 × 900 less a round socket D150 × 500.
    let q = file.pset(product(&f, &file, "REG-102-F1"), "Qto_FootingBaseQuantities").unwrap();
    assert!(close(&q["Length"], 600.0) && close(&q["Width"], 400.0) && close(&q["Height"], 900.0));
    assert!(close(&q["GrossVolume"], 0.216));
    assert!(close(&q["NetVolume"], 0.216 - PI / 4.0 * 150.0 * 150.0 * 500.0 / 1e9), "{}", q["NetVolume"]);
    // A cylinder has no length and width.
    let q = file.pset(product(&f, &file, "ADS-101-F1"), "Qto_FootingBaseQuantities").unwrap();
    assert_eq!(q.keys().map(String::as_str).collect::<Vec<_>>(), ["CrossSectionArea", "GrossVolume", "Height", "NetVolume"]);
    assert!(close(&q["GrossVolume"], PI / 4.0 * 600.0 * 600.0 * 1200.0 / 1e9));
    // A foundation without a readable definition has no quantities.
    assert!(file.pset(product(&f, &file, "WRN-105-F1"), "Qto_FootingBaseQuantities").is_none());
}

#[test]
fn property_sets_carry_the_site_position_and_the_design_basis() {
    let (f, file, _) = exported();
    let s = f.document.find(f.id("ADS-101")).unwrap().site.clone().unwrap();
    for holder in [product(&f, &file, "ADS-101"), assembly(&f, &file, "ADS-101")] {
        let p = file.pset(holder, "AfroBIM_SitePlacement").unwrap();
        assert!(p["X"].starts_with("IFCLENGTHMEASURE(") && p["RotationDeg"].starts_with("IFCREAL("));
        let value = |key: &str| number(p[key].split_once('(').unwrap().1.trim_end_matches(')'));
        assert!((value("X") - s.x).abs() < 1e-6 && (value("Y") - s.y).abs() < 1e-6 && value("Z") == 1500.0);
        assert!((value("RotationDeg") - s.rotation_deg).abs() < 1e-6);
        assert_eq!((p["Alignment"].as_str(), p["StationM"].as_str(), p["OffsetM"].as_str()), ("IFCLABEL('MC01')", "IFCREAL(1250.)", "IFCREAL(4.5)"));
    }
    // A support carries its own position, without the station that belongs to the sign's point.
    let post = product(&f, &file, "ADS-101-P1");
    let p = file.pset(post, "AfroBIM_SitePlacement").unwrap();
    assert_eq!(p.keys().map(String::as_str).collect::<Vec<_>>(), ["RotationDeg", "X", "Y", "Z"]);
    // An unplaced element has no such set.
    assert!(file.pset(product(&f, &file, "INF-103"), "AfroBIM_SitePlacement").is_none());
    assert!(file.pset(product(&f, &file, "INF-103-P1"), "AfroBIM_SitePlacement").is_none());

    // The design basis is copied as written (here with an apostrophe and a non-ASCII character).
    let basis = format!("IFCLABEL({})", afrobim_ifc::step_string(DESIGN_BASIS));
    let p = file.pset(post, "AfroBIM_Post").unwrap();
    assert_eq!(p["DesignBasis"], basis);
    assert_eq!(p["SignMark"], "IFCLABEL('ADS-101')");
    assert_eq!(p["Section"], "IFCLABEL('CHS D100 t10')");
    assert_eq!((p["Length"].as_str(), p["Embedment"].as_str(), p["AboveGround"].as_str()), ("IFCLENGTHMEASURE(4300.)", "IFCLENGTHMEASURE(1000.)", "IFCLENGTHMEASURE(3300.)"));
    assert_eq!(p["Material"], format!("IFCLABEL('{STEEL}')"));
    assert!(!p.contains_key("Finish") && !p.contains_key("PlaceholderDimensions"));
    assert_eq!(file.pset(post, "AfroBIM_Properties").unwrap()["design_basis"], basis, "the element's own properties stay complete");
    assert_eq!(file.pset(post, "Pset_MemberCommon").unwrap()["IsExternal"], "IFCBOOLEAN(.T.)");
    assert!(file.pset(post, "AfroBIM_Derived").unwrap().contains_key("Section area"));
    let p = file.pset(product(&f, &file, "ADS-101-F1"), "AfroBIM_Foundation").unwrap();
    assert_eq!(p["DesignBasis"], basis);
    assert_eq!((p["SignMark"].as_str(), p["HostMark"].as_str()), ("IFCLABEL('ADS-101')", "IFCLABEL('ADS-101-P1')"));
    assert_eq!((p["Diameter"].as_str(), p["Depth"].as_str(), p["TopBelowGround"].as_str()), ("IFCLENGTHMEASURE(600.)", "IFCLENGTHMEASURE(1200.)", "IFCLENGTHMEASURE(100.)"));
    let p = file.pset(assembly(&f, &file, "ADS-101"), "AfroBIM_SignAssembly").unwrap();
    assert_eq!(p["DesignBasis"], basis);
    assert_eq!((p["Posts"].as_str(), p["Foundations"].as_str(), p["MountingHeight"].as_str()), ("IFCINTEGER(2)", "IFCINTEGER(2)", "IFCLENGTHMEASURE(2100.)"));
    // No design basis where the model has none.
    assert!(!file.pset(product(&f, &file, "REG-102-P1"), "AfroBIM_Post").unwrap().contains_key("DesignBasis"));
    assert!(!file.pset(assembly(&f, &file, "REG-102"), "AfroBIM_SignAssembly").unwrap().contains_key("DesignBasis"));
    // The socket and the blinding are stated; the placeholder flag and the finish only when set.
    let p = file.pset(product(&f, &file, "REG-102-F1"), "AfroBIM_Foundation").unwrap();
    assert_eq!(p["Socket"], "IFCLABEL('round, diameter 150 mm, depth 500 mm')");
    assert_eq!(p["BlindingThickness"], "IFCLENGTHMEASURE(50.)");
    assert!(!p.contains_key("Concrete"));
    let p = file.pset(product(&f, &file, "LOOSE-P1"), "AfroBIM_Post").unwrap();
    assert_eq!((p["PlaceholderDimensions"].as_str(), p["Finish"].as_str()), ("IFCBOOLEAN(.T.)", "IFCLABEL('painted (test text)')"));
    assert!(!p.contains_key("SignMark") && !p.contains_key("Material"));

    // Only standard sets use the reserved prefixes.
    for set in file.of_type("IFCPROPERTYSET").iter().chain(&file.of_type("IFCELEMENTQUANTITY")) {
        let name = file.args(*set)[2].trim_matches('\'');
        let standard = ["Pset_SignCommon", "Pset_MemberCommon", "Pset_Stationing", "Qto_SignBaseQuantities", "Qto_MemberBaseQuantities", "Qto_FootingBaseQuantities"];
        assert!(name.starts_with("AfroBIM_") || standard.contains(&name), "{name}");
    }
}

#[test]
fn materials_are_the_models_free_text_and_nothing_else() {
    let (f, file, _) = exported();
    let materials: Vec<String> = file.of_type("IFCMATERIAL").iter().map(|m| file.args(*m).join(",")).collect();
    assert_eq!(materials, [format!("'{CONCRETE}',$,$"), format!("'{STEEL}',$,$")], "name only: no description, no category");
    let made_of = |text: &str| -> Vec<u32> {
        let material = file.of_type("IFCMATERIAL").into_iter().find(|m| file.args(*m)[0] == format!("'{text}'")).unwrap();
        let rel = file.of_type("IFCRELASSOCIATESMATERIAL").into_iter().find(|r| reference(&file.args(*r)[5]) == material).unwrap();
        references(&file.args(rel)[4])
    };
    let products = |names: &[&str]| names.iter().map(|n| product(&f, &file, n)).collect::<Vec<_>>();
    assert_eq!(made_of(STEEL), products(&["ADS-101-P1", "ADS-101-P2", "INF-103-P1", "GDS-104-P1", "GDS-104-P2"]));
    assert_eq!(made_of(CONCRETE), products(&["ADS-101-F1", "ADS-101-F2", "INF-103-F1", "GDS-104-F1"]));
}

#[test]
fn the_alignment_is_exported_as_its_horizontal_segments_and_a_polyline() {
    let (f, file, notes) = exported();
    let alignment = product(&f, &file, "MC01");
    assert_eq!(file.name(alignment), "IFCALIGNMENT");
    assert_eq!(file.args(alignment)[2], "'MC01'");
    // No placement relative to anything: alignment coordinates are the site's.
    assert_eq!(file.args(placement(&file, alignment))[0], "$");

    let nests: Vec<(u32, Vec<u32>)> = file.of_type("IFCRELNESTS").iter().map(|r| (reference(&file.args(*r)[4]), references(&file.args(*r)[5]))).collect();
    let horizontal = file.of_type("IFCALIGNMENTHORIZONTAL")[0];
    let referent = file.of_type("IFCREFERENT")[0];
    assert!(nests.contains(&(alignment, vec![horizontal])) && nests.contains(&(alignment, vec![referent])));
    let segments = &nests.iter().find(|(w, _)| *w == horizontal).unwrap().1;
    let design: Vec<&[String]> = segments.iter().map(|s| file.args(reference(file.args(*s).last().unwrap()))).collect();
    // start direction, start radius, end radius, length (mm), type. Positive radii turn left.
    let rows: Vec<(f64, f64, f64, f64, &str)> = design.iter().map(|a| (number(&a[3]), number(&a[4]), number(&a[5]), number(&a[6]), a[8].as_str())).collect();
    let want = [
        (0.3, 0.0, 0.0, 100_000.0, ".LINE."),
        (0.3, 0.0, 200_000.0, 40_000.0, ".CLOTHOID."),
        (0.4, 200_000.0, 200_000.0, 80_000.0, ".CIRCULARARC."),
        (0.8, 200_000.0, 0.0, 40_000.0, ".CLOTHOID."),
        (0.9, 0.0, 0.0, 50_000.0, ".LINE."),
        (0.9, -150_000.0, -150_000.0, 60_000.0, ".CIRCULARARC."),
        (0.5, -150_000.0, 0.0, 30_000.0, ".CLOTHOID."),
        // The zero-length segment that closes the layout, on the end direction 0.5 − 30/(2·150).
        (0.4, 0.0, 0.0, 0.0, ".LINE."),
    ];
    assert_eq!(rows.len(), want.len());
    for (i, (have, want)) in rows.iter().zip(&want).enumerate() {
        assert!((have.0 - want.0).abs() < 1e-12 && (have.1 - want.1).abs() < 1e-5 && (have.2 - want.2).abs() < 1e-5 && have.3 == want.3 && have.4 == want.4, "segment {}: {have:?}, expected {want:?}", i + 1);
    }
    // Every segment starts where the alignment's element starts; the last where the alignment ends.
    for (a, e) in design.iter().zip(&f.alignment.elements) {
        let p = numbers(&file.args(reference(&a[2]))[0]);
        assert!((p[0] - e.x * 1000.0).abs() < 1e-6 && (p[1] - e.y * 1000.0).abs() < 1e-6);
    }
    let end = f.alignment.point_at(f.alignment.end_station_m()).unwrap();
    let last = numbers(&file.args(reference(&design[7][2]))[0]);
    assert!((last[0] - end.x * 1000.0).abs() < 1e-3 && (last[1] - end.y * 1000.0).abs() < 1e-3);

    // The Axis: a 2D polyline in its own subcontext, straights as two points, curves in chords of 1 m.
    let shape = reference(&file.args(alignment)[6]);
    let representation = file.args(references(&file.args(shape)[2])[0]);
    assert_eq!((representation[1].as_str(), representation[2].as_str()), ("'Axis'", "'Curve2D'"));
    assert_eq!(file.args(reference(&representation[0]))[0], "'Axis'");
    let curve = references(&representation[3])[0];
    assert_eq!(file.name(curve), "IFCINDEXEDPOLYCURVE");
    let list = &file.args(reference(&file.args(curve)[0]))[0];
    let points = list.matches("),(").count() + 1;
    assert_eq!(points, 1 + 1 + 40 + 80 + 40 + 1 + 60 + 30);
    assert!(list.starts_with("((2000000.,1000000.),("));

    // The start station, as a referent with Pset_Stationing in millimetres.
    assert_eq!(file.args(referent)[2], "'1+000.000'");
    assert_eq!(file.args(referent).last().unwrap(), ".STATION.");
    assert_eq!(file.pset(referent, "Pset_Stationing").unwrap()["Station"], "IFCLENGTHMEASURE(1000000.)");
    assert_pose("referent", file.world(placement(&file, referent)), [2.0e6, 1.0e6, 0.0, 0.3_f64.to_degrees()]);
    let p = file.pset(alignment, "AfroBIM_Alignment").unwrap();
    assert_eq!((p["StartStationM"].as_str(), p["EndStationM"].as_str(), p["LengthM"].as_str(), p["Elements"].as_str()), ("IFCREAL(1000.)", "IFCREAL(1400.)", "IFCREAL(400.)", "IFCINTEGER(7)"));
    assert!(notes.iter().any(|n| n.starts_with("MC01: the Axis is a polyline")));
}

#[test]
fn the_notes_say_what_was_approximated_or_left_out() {
    let (_, _, notes) = exported();
    let has = |needle: &str| notes.iter().any(|n| n.contains(needle));
    assert!(has("WRN-105-F1: its definition cannot be read; the module's solids are exported as a tessellation"), "{notes:#?}");
    assert!(has("REG-102-F1: the socket is not cut out of the solid"));
    assert!(has("LOOSE-P1: no sign among the exported elements hosts this post"));
    assert!(has("LOOSE-P1: its dimensions are placeholders"));
    assert!(has("MK-107: category 'marking' has no IFC mapping"));
    assert!(has("3 element(s) have no site placement and stand at their sheet position"));
    assert!(!has("MC01: alignment not exported"));
}

#[test]
fn unreadable_supports_fall_back_to_their_solids_or_to_nothing() {
    let (f, file, _) = exported();
    // WRN-105-F1: the cuboid its module supplied, as a closed face set, still a PAD_FOOTING.
    let footing = product(&f, &file, "WRN-105-F1");
    let representation = file.args(references(&file.args(reference(&file.args(footing)[6]))[2])[0]);
    assert_eq!(representation[2], "'Tessellation'");
    let set = references(&representation[3])[0];
    assert_eq!(file.name(set), "IFCTRIANGULATEDFACESET");
    assert_eq!(file.args(set)[2], ".T.");
    assert_eq!(file.args(footing)[3], "$", "no description without a definition");

    // Without solids either: the product is still there, without a shape, and a note says so.
    // An open mesh is exported as an open face set; a broken one is dropped.
    let mut d = Document::default();
    let sign = d.add("sign", "S-1", Pt::new(0.0, 0.0), 600.0, 600.0, "0", json!({}));
    let bare = d.add("post", "S-1-P1", Pt::new(900.0, 0.0), 100.0, 3000.0, "Supports", json!("nonsense"));
    let open = d.add("post", "S-1-P2", Pt::new(1200.0, 0.0), 100.0, 3000.0, "Supports", json!({ "section": { "type": "Chs", "outside_diameter_mm": 100.0, "wall_mm": 60.0 } }));
    for id in [bare, open] {
        d.find_mut(id).unwrap().host = Some(sign);
    }
    let mut sheet = Mesh3::cuboid("Post", [0.0, 0.0, 0.0], [100.0, 100.0, 3000.0], [1, 2, 3]);
    sheet.triangles.truncate(10);
    let broken = Mesh3 { name: "Broken".into(), vertices: vec![[0.0; 3]], triangles: vec![[0, 1, 2]], colour: [0; 3] };
    let generated = Generated { solids: vec![sheet, broken], ..Generated::default() };
    let inputs = [
        IfcElementInput { element: d.find(sign).unwrap(), generated: None },
        IfcElementInput { element: d.find(bare).unwrap(), generated: None },
        IfcElementInput { element: d.find(open).unwrap(), generated: Some(&generated) },
    ];
    let export = export_ifc_model(&d, &inputs, &options());
    let file = StepFile::parse(&export.text);
    let members = file.of_type("IFCMEMBER");
    assert_eq!(members.len(), 2);
    assert_eq!(file.args(members[0])[6], "$");
    assert_ne!(file.args(members[1])[6], "$");
    let sets = file.of_type("IFCTRIANGULATEDFACESET");
    assert_eq!(sets.iter().filter(|s| file.args(**s)[2] == ".F.").count(), 1, "the open mesh is not claimed closed");
    assert!(export.notes.iter().any(|n| n == "S-1-P1: no geometry exported, its definition cannot be read and it has no solids"), "{:#?}", export.notes);
    assert!(export.notes.iter().any(|n| n == "S-1-P2: solid 'Broken' is not a sound triangle mesh; not exported"));
    // No position recorded under the sign: both stand at their sheet positions, and say so.
    assert_eq!(export.notes.iter().filter(|n| n.contains("its position under S-1 is not recorded")).count(), 2);
    assert_pose("S-1-P1", file.world(placement(&file, members[0])), [900.0, 0.0, 0.0, 0.0]);
    // No site placement anywhere and no alignment: the site contains the assembly, as before.
    assert_eq!(file.count("IFCROAD"), 0);
    assert_eq!(file.count("IFCELEMENTASSEMBLY"), 1);
}

#[test]
fn alignments_that_cannot_be_written_correctly_are_left_out() {
    let mut d = Document::default();
    let unreadable = d.add("alignment", "BAD-1", Pt::new(0.0, 0.0), 10.0, 10.0, "Alignments", json!({ "name": "BAD-1" }));
    // A spiral from a left-hand to a right-hand radius: one clothoid segment cannot state it.
    let s_curve = json!({ "name": "BAD-2", "start_station_m": 0.0, "elements": [
        { "x": 0.0, "y": 0.0, "heading_rad": 0.0, "length": 50.0, "curvature_start": 0.01, "curvature_end": -0.01 }
    ] });
    let reverse = d.add("alignment", "BAD-2", Pt::new(0.0, 0.0), 10.0, 10.0, "Alignments", s_curve);
    let empty = d.add("alignment", "BAD-3", Pt::new(0.0, 0.0), 10.0, 10.0, "Alignments", json!({ "name": "BAD-3", "start_station_m": 0.0, "elements": [] }));
    let inputs: Vec<IfcElementInput> = [unreadable, reverse, empty].iter().map(|id| IfcElementInput { element: d.find(*id).unwrap(), generated: None }).collect();
    let export = export_ifc_model(&d, &inputs, &options());
    let file = StepFile::parse(&export.text);
    assert_eq!(file.count("IFCALIGNMENT") + file.count("IFCBUILDINGELEMENTPROXY") + file.count("IFCROAD") + file.count("IFCRELCONTAINEDINSPATIALSTRUCTURE"), 0);
    assert_eq!(export.notes.len(), 3, "{:#?}", export.notes);
    assert!(export.notes[0].starts_with("BAD-1: alignment not exported, "));
    assert_eq!(export.notes[1], "BAD-2: alignment not exported, element 1 is a spiral that changes hand, which this exporter does not write");
    assert_eq!(export.notes[2], "BAD-3: alignment not exported, it has no elements");
}

#[test]
fn odd_site_input_still_gives_a_well_formed_file() {
    let mut d = Document::default();
    let sign = d.add("sign", "S-1", Pt::new(100.0, 200.0), 600.0, 600.0, "0", json!({}));
    d.find_mut(sign).unwrap().site = Some(SitePlacement { x: f64::NAN, y: 0.0, z: 0.0, rotation_deg: 0.0, alignment: Some("NOWHERE".into()), station_m: None, offset_m: None });
    // A full turn and a negative turn are the same placement as no turn and a positive one.
    let turned = d.add("sign", "S-2", Pt::new(0.0, 0.0), 600.0, 600.0, "0", json!({}));
    d.find_mut(turned).unwrap().site = Some(SitePlacement { x: 5.0e8, y: -7.5e8, z: 12.5, rotation_deg: 720.0, alignment: None, station_m: Some(f64::INFINITY), offset_m: None });
    let back = d.add("sign", "S-3", Pt::new(0.0, 0.0), 600.0, 600.0, "0", json!({}));
    d.find_mut(back).unwrap().site = Some(SitePlacement { x: 0.0, y: 0.0, z: 0.0, rotation_deg: -270.0, alignment: None, station_m: None, offset_m: None });
    // A foundation that hosts itself must not hang the exporter.
    let knot = d.add("foundation", "KNOT", Pt::new(0.0, 0.0), 10.0, 10.0, "0", json!({}));
    d.find_mut(knot).unwrap().host = Some(knot);
    let e = |id| IfcElementInput { element: d.find(id).unwrap(), generated: None };
    let export = export_ifc_model(&d, &[e(sign), e(turned), e(back), e(knot), e(turned)], &options());
    let file = StepFile::parse(&export.text);
    assert_eq!(file.count("IFCSIGN"), 3);
    assert_eq!(file.count("IFCFOOTING"), 1);
    let guid = |id| file.by_guid(&element_global_id(&options().project_name, id));
    assert_pose("S-1", file.world(placement(&file, guid(sign))), [100.0, 0.0, 200.0, 0.0]);
    assert_pose("S-2", file.world(placement(&file, guid(turned))), [5.0e8, -7.5e8, 12.5, 0.0]);
    assert_pose("S-3", file.world(placement(&file, guid(back))), [0.0, 0.0, 0.0, 90.0]);
    assert!(export.text.contains("(500000000.,-750000000.,12.5)"));
    assert!(file.pset(guid(sign), "AfroBIM_SitePlacement").is_none());
    assert!(!file.pset(guid(turned), "AfroBIM_SitePlacement").unwrap().contains_key("StationM"));
    let has = |needle: &str| export.notes.iter().any(|n| n.contains(needle));
    assert!(has("S-1: the site placement is not a finite position"));
    assert!(has("S-2: element id") && has("given more than once"));
    assert!(has("KNOT: no sign among the exported elements hosts this foundation"));
    assert!(!export.text.contains("NaN") && !export.text.contains("inf"));
}
