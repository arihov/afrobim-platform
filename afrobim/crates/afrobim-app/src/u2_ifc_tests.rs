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
fn a_second_set_marking_is_exported_as_a_building_element_proxy() {
    // `afrobim-ifc` has no mapping for category `marking_ext`: the marking is written as a
    // generic IfcBuildingElementProxy named by its mark, with its category as the object type.
    // It is not an IfcSurfaceFeature and it is not attached to a pavement.
    let (doc, _, mark) = doc_with_stop_line();
    let text = export_text(&doc, &afrobim_ifc::IfcOptions { timestamp: Some(0), ..Default::default() });
    let line = text.lines().find(|l| l.contains(&format!("'{mark}'")) && l.contains("IFCBUILDINGELEMENTPROXY")).expect("the marking is in the file as a proxy");
    assert!(line.contains("'marking_ext'"), "{line}");
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
