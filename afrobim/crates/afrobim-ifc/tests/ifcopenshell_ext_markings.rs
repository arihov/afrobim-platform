//! The second marking set (category `marking_ext`) in the model export, and its validation by
//! IfcOpenShell.
//!
//! The document has a pavement over stations 0 to 450 and one marking of every kind the
//! catalogue draws (lines, transverse lines, crossings, triangle, arrows, worded blocks, hatching,
//! chevrons, island marking, zig-zag, road studs, UK and Uganda), plus one the pavement does not
//! carry. The export is written and `tests/check_ext_markings_ifc.py` opens it with IfcOpenShell,
//! validates it (schema and EXPRESS rules), checks the class, predefined type, adhesion and
//! properties of each marking against what this file says, and checks the standard property
//! sets against the IFC4X3 templates. A missing `python3` or `ifcopenshell` makes the test print
//! why and pass, unless `AFROBIM_REQUIRE_IFCOPENSHELL` is set.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_ifc::{ext_feature_type, write_ifc_model, IfcElementInput};
use afrobim_kernel::{Document, ElementId, Generated, ModuleRegistry, Pt};
use afrobim_markings::{add_ext_marking, Basis, ExtDefinition, ExtMarkingModule, GeometryStatus, Setting, StudColour};
use afrobim_pavement::{Layer, Material, PavementDefinition};
use serde_json::{json, Value};

use common::roads::alignment;
use common::{options, python_with_ifcopenshell};

/// Every marking: name, definition, expected `IfcSurfaceFeature` predefined type.
fn definitions() -> Vec<(&'static str, ExtDefinition, &'static str)> {
    use Basis::{Uganda, Uk};
    vec![
        ("X-1017", ExtDefinition::linear("1017", Uk, 20.0, 60.0, 3.4).with_width(75), "LINEMARKING"),
        ("X-1018", ExtDefinition::linear("1018.1", Uk, 20.0, 60.0, -3.4).with_width(100).with_separation(75), "LINEMARKING"),
        ("X-1002", ExtDefinition::across("1002.1", Uk, 50.0, -3.4, 0.0), "LINEMARKING"),
        ("X-1003A", ExtDefinition::across("1003A", Uk, 80.0, 0.0, 3.4), "LINEMARKING"),
        ("X-1023A", ExtDefinition::point("1023A", Uk, 100.0, 0.0, 0.0), "SYMBOLMARKING"),
        ("X-1022", ExtDefinition::point("1022", Uk, 120.0, 0.0, 0.0).with_size(2800), "SYMBOLMARKING"),
        ("X-1038", ExtDefinition::point("1038-ahead", Uk, 140.0, 1.75, 0.0).with_speed(30, "30 mph limit"), "SYMBOLMARKING"),
        ("X-1014", ExtDefinition::point("1014", Uk, 160.0, 0.0, 0.0).with_speed(30, "85th percentile"), "SYMBOLMARKING"),
        ("X-1040", ExtDefinition::linear("1040", Uk, 170.0, 194.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "30 mph limit"), "HATCHMARKING"),
        ("X-1041", ExtDefinition::linear("1041", Uk, 200.0, 224.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "30 mph limit"), "HATCHMARKING"),
        ("X-STUD", ExtDefinition::studs(Uk, 10.0, 100.0, 0.0, 18000, StudColour::White), "USERDEFINED"),
        ("X-M01", ExtDefinition::linear("M01", Uganda, 250.0, 400.0, 0.0), "LINEMARKING"),
        ("X-M03", ExtDefinition::linear("M03", Uganda, 250.0, 340.0, 1.8).with_setting(Setting::Urban), "LINEMARKING"),
        ("X-M05", ExtDefinition::linear("M05", Uganda, 300.0, 321.5, -1.0), "LINEMARKING"),
        ("X-M33", ExtDefinition { count: Some(5), gap_mm: Some(600), ..ExtDefinition::across("M33", Uganda, 60.0, -2.0, 2.0).with_width(600) }, "PAVEMENTSURFACEMARKING"),
        ("X-M34", ExtDefinition { length_mm: Some(4000), ..ExtDefinition::across("M34", Uganda, 70.0, 0.0, 3.0) }, "LINEMARKING"),
        ("X-M41", ExtDefinition::point("M41", Uganda, 410.0, 0.0, 0.0).with_speed(50, "50 km/h").with_material("thermoplastic"), "SYMBOLMARKING"),
        ("X-M53", ExtDefinition::linear("M53", Uganda, 330.0, 350.0, 0.0).with_strip_width(3000), "HATCHMARKING"),
        ("X-M62", ExtDefinition::point("M62", Uganda, 360.0, 0.0, 0.0).with_size(2800), "SYMBOLMARKING"),
        ("X-M61", ExtDefinition::point("M61", Uganda, 370.0, 0.0, 0.0), "SYMBOLMARKING"),
    ]
}

/// The one the pavement (ending at station 450) does not carry.
fn uncarried() -> (&'static str, ExtDefinition) {
    ("X-BEYOND", ExtDefinition::across("1001", Basis::Uk, 480.0, -3.4, 3.4).with_width(300))
}

struct Fixture {
    document: Document,
    generated: std::collections::BTreeMap<ElementId, Generated>,
}

impl Fixture {
    fn inputs(&self) -> Vec<IfcElementInput<'_>> {
        self.document.elements().iter().map(|element| IfcElementInput { element, generated: self.generated.get(&element.id) }).collect()
    }
}

fn fixture(with_pavement: bool) -> Fixture {
    let a = alignment();
    let mut doc = Document::default();
    let id = doc.add(afrobim_alignment::CATEGORY, "MC10", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());
    for (name, def, _) in definitions() {
        add_ext_marking(&mut doc, id, name, &def).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let (name, def) = uncarried();
    add_ext_marking(&mut doc, id, name, &def).unwrap();
    if with_pavement {
        let layers = vec![Layer::new("Surfacing", Material::HotMixAsphalt, 50.0), Layer::new("Base", Material::GranularBase, 150.0)];
        let mut pavement = PavementDefinition::new(a.clone(), 7.0, 2.5, layers);
        pavement.start_station_m = 0.0;
        pavement.end_station_m = 450.0;
        afrobim_pavement::add_pavement(&mut doc, "PV-001", &pavement).unwrap();
    }
    let mut modules = ModuleRegistry::default();
    modules.register(Box::new(ExtMarkingModule));
    modules.register(Box::new(afrobim_pavement::PavementModule::default()));
    let generated = doc.elements().iter().filter_map(|e| modules.generate(e).ok().map(|g| (e.id, g))).collect();
    Fixture { document: doc, generated }
}

fn expectations() -> Value {
    let mut markings = Vec::new();
    for (name, def, predefined) in definitions() {
        let spec = afrobim_markings::ext_spec(&def.type_id);
        let placeholder = spec.is_some_and(|s| matches!(s.geometry, GeometryStatus::Placeholder(_)));
        let user_defined = predefined == "USERDEFINED";
        markings.push(json!({
            "name": name,
            "type_id": def.type_id,
            "class": "IfcSurfaceFeature",
            "predefined": predefined,
            "object_type": if user_defined { json!("ROAD_STUD") } else { Value::Null },
            "adheres_to": "PV-001",
            "placeholder": placeholder,
            "common": !user_defined,
        }));
    }
    let (name, def) = uncarried();
    markings.push(json!({
        "name": name, "type_id": def.type_id, "class": "IfcBuildingElementProxy", "predefined": "USERDEFINED", "object_type": "marking",
        "adheres_to": Value::Null, "placeholder": false, "common": false,
    }));
    json!({ "pavement": "PV-001", "markings": markings })
}

fn run_check(ifc: &Path, expected: Option<&Path>) {
    let python = match python_with_ifcopenshell() {
        Ok(python) => python,
        Err(why) => {
            let message = format!("SKIPPED: IfcOpenShell validation not run, no Python with ifcopenshell ({why}). Install it with `pip install ifcopenshell`.");
            assert!(std::env::var_os("AFROBIM_REQUIRE_IFCOPENSHELL").is_none(), "{message}");
            println!("{message}");
            eprintln!("{message}");
            return;
        }
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("check_ext_markings_ifc.py");
    let mut command = Command::new(python);
    command.arg(&script).arg(ifc);
    if let Some(expected) = expected {
        command.arg(expected);
    }
    let out = command.output().expect("run check_ext_markings_ifc.py");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    assert!(out.status.success(), "check_ext_markings_ifc.py failed on {}\n{stdout}\n{}", ifc.display(), String::from_utf8_lossy(&out.stderr));
}

#[test]
fn ifcopenshell_validates_every_kind_of_second_set_marking() {
    let f = fixture(true);
    let text = write_ifc_model(&f.document, &f.inputs(), &options());
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ifc = dir.join("afrobim-ifc-ext-markings.ifc");
    let expected = dir.join("afrobim-ifc-ext-markings.expected.json");
    std::fs::write(&ifc, text).unwrap();
    std::fs::write(&expected, serde_json::to_string_pretty(&expectations()).unwrap()).unwrap();
    run_check(&ifc, Some(&expected));
}

#[test]
fn ifcopenshell_validates_the_second_set_without_a_pavement() {
    let f = fixture(false);
    let text = write_ifc_model(&f.document, &f.inputs(), &options());
    assert!(!text.contains("IFCSURFACEFEATURE"), "nothing adheres without a pavement");
    let ifc = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("afrobim-ifc-ext-markings-no-pavement.ifc");
    std::fs::write(&ifc, text).unwrap();
    run_check(&ifc, None);
}

#[test]
fn the_exporter_runs_the_module_itself_when_it_is_handed_nothing() {
    let f = fixture(true);
    let bare: Vec<IfcElementInput> = f.document.elements().iter().map(|element| IfcElementInput { element, generated: None }).collect();
    let with = write_ifc_model(&f.document, &f.inputs(), &options());
    let without = write_ifc_model(&f.document, &bare, &options());
    assert_eq!(with.matches("IFCSURFACEFEATURE(").count(), without.matches("IFCSURFACEFEATURE(").count());
    assert_eq!(with.matches("IFCSURFACEFEATURE(").count(), definitions().len());
}

#[test]
fn every_catalogue_entry_has_a_feature_type() {
    // The type comes from the kind of the entry, and every entry of the catalogue has one.
    for spec in afrobim_markings::ext_catalogue() {
        let t = ext_feature_type(spec.id).unwrap_or_else(|| panic!("{}", spec.id));
        assert!(!t.because.is_empty());
        assert_eq!(t.predefined == "USERDEFINED", t.object_type.is_some(), "{}", spec.id);
        // A rumble strip is not a marking and is never chosen.
        assert!(!t.predefined.contains("RUMBLE"), "{}", spec.id);
    }
    let predefined = |id: &str| ext_feature_type(id).unwrap().predefined;
    for (id, want) in [
        ("1017", "LINEMARKING"), ("1001", "LINEMARKING"), ("M05", "LINEMARKING"), ("M34", "LINEMARKING"), ("M71", "LINEMARKING"),
        ("1023A", "SYMBOLMARKING"), ("1038-ahead", "SYMBOLMARKING"), ("1022", "SYMBOLMARKING"), ("M63", "SYMBOLMARKING"),
        ("1040", "HATCHMARKING"), ("1042", "HATCHMARKING"), ("M53", "HATCHMARKING"), ("M51", "HATCHMARKING"),
        ("M33", "PAVEMENTSURFACEMARKING"), ("M35", "PAVEMENTSURFACEMARKING"), ("STUD", "USERDEFINED"),
    ] {
        assert_eq!(predefined(id), want, "{id}");
    }
    assert_eq!(ext_feature_type("STUD").unwrap().object_type, Some("ROAD_STUD"));
    assert!(ext_feature_type("no-such-type").is_none());
}
