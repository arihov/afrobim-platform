//! Glue tests: what the IFC export does with the second marking set, and the export options.

use afrobim_ifc::AlignmentExport;
use afrobim_kernel::{Document, ElementId, ModuleRegistry};

use crate::alignment_glue;
use crate::ext_markings_glue::{self as glue, ExtRequest};

fn doc_with_stop_line() -> (Document, ElementId, String) {
    let mut doc = Document::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
    alignment_glue::import_landxml(&mut doc, &path).unwrap();
    let (a, _) = alignment_glue::alignments(&doc).remove(0);
    let start = alignment_glue::alignment_of(&doc, a).unwrap().start_station_m;
    let mut r = ExtRequest::new(afrobim_markings::Basis::Uk, "1002.1", start + 100.0, start + 100.0);
    r.offset_m = -3.5;
    let mark = glue::add(&mut doc, a, &r).unwrap();
    (doc, a, mark)
}

fn export_text(doc: &Document, options: &afrobim_ifc::IfcOptions) -> String {
    let mut modules = ModuleRegistry::default();
    crate::markings_glue::register(&mut modules);
    crate::modules_glue::register(&mut modules);
    let generated: Vec<_> = doc.elements().iter().map(|e| modules.generate(e).ok()).collect();
    let inputs: Vec<afrobim_ifc::IfcElementInput> = doc.elements().iter().zip(&generated).map(|(e, g)| afrobim_ifc::IfcElementInput { element: e, generated: g.as_ref() }).collect();
    afrobim_ifc::export_ifc_model(doc, &inputs, options).text
}

#[test]
fn a_second_set_marking_no_pavement_carries_is_exported_as_a_building_element_proxy() {
    // An IfcSurfaceFeature must adhere to an element, so a marking no pavement carries is the
    // documented proxy fallback: a generic IfcBuildingElementProxy named by its mark, with the
    // object type `marking`, as for the first set. It has no standard marking property sets.
    let (doc, _, mark) = doc_with_stop_line();
    let text = export_text(&doc, &afrobim_ifc::IfcOptions { timestamp: Some(0), ..Default::default() });
    let line = text.lines().find(|l| l.contains(&format!("'{mark}'")) && l.contains("IFCBUILDINGELEMENTPROXY")).expect("the marking is in the file as a proxy");
    assert!(line.contains("'marking'") && !line.contains("'marking_ext'"), "{line}");
    assert!(!text.contains("IFCSURFACEFEATURE") && !text.contains("Pset_RoadMarkingCommon"));
}

#[test]
fn a_second_set_marking_a_pavement_carries_is_an_ifc_surface_feature_adhering_to_it() {
    // The stop line (1002.1, a line painted across the road) is LINEMARKING, "2D lines painted on
    // pavement surfaces to form boundaries, centrelines and edge markings" (IFC 4.3).
    let (mut doc, a, mark) = doc_with_stop_line();
    let alignment = alignment_glue::alignment_of(&doc, a).unwrap();
    let layers = vec![afrobim_pavement::Layer::new("Surfacing", afrobim_pavement::Material::HotMixAsphalt, 50.0)];
    let mut pavement = afrobim_pavement::PavementDefinition::new(alignment.clone(), 8.0, 2.5, layers);
    pavement.start_station_m = alignment.start_station_m;
    pavement.end_station_m = alignment.start_station_m + 300.0;
    afrobim_pavement::add_pavement(&mut doc, "PV-001", &pavement).unwrap();
    let text = export_text(&doc, &afrobim_ifc::IfcOptions { timestamp: Some(0), ..Default::default() });
    let line = text.lines().find(|l| l.starts_with("#") && l.contains("IFCSURFACEFEATURE") && l.contains(&format!("'{mark}'"))).expect("the marking is an IfcSurfaceFeature");
    assert!(line.contains(".LINEMARKING."), "{line}");
    assert!(text.contains("IFCRELADHERESTOELEMENT"));
    assert!(text.contains("Pset_RoadMarkingCommon"));
}

#[test]
fn a_linear_export_validates_when_ifcopenshell_is_available() {
    let python_ok = std::process::Command::new("python3").args(["-c", "import ifcopenshell, ifcopenshell.validate"]).output().map(|o| o.status.success()).unwrap_or(false);
    if !python_ok {
        eprintln!("IfcOpenShell is not available: the validation test is skipped");
        return;
    }
    let (doc, _, _) = doc_with_stop_line();
    let text = export_text(&doc, &afrobim_ifc::IfcOptions { timestamp: Some(0), alignment: AlignmentExport::Linear, ..Default::default() });
    let dir = std::env::temp_dir().join(format!("afrobim-ifc-linear-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("linear.ifc");
    std::fs::write(&file, text).unwrap();
    let script = "import sys, ifcopenshell, ifcopenshell.validate as v\nf = ifcopenshell.open(sys.argv[1])\nl = v.json_logger()\nv.validate(f, l)\nerrs = [s for s in l.statements if s.get('level') in ('ERROR', 40, 'Error')]\nprint(len(f.by_type('IfcAlignment')), len(errs))\nfor s in errs[:5]:\n    print(s.get('message'))\n";
    let out = std::process::Command::new("python3").args(["-I", "-c", script]).arg(&file).output().unwrap();
    let said = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    let mut words = said.split_whitespace();
    let alignments: usize = words.next().and_then(|w| w.parse().ok()).unwrap_or_else(|| panic!("validator output: {said} {}", String::from_utf8_lossy(&out.stderr)));
    let errors: usize = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    assert!(alignments >= 1, "{said}");
    assert_eq!(errors, 0, "{said}");
}
