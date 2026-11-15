//! Glue tests: the report pack lists per check only that check's stored inputs, and its design
//! basis section names the bases in use.

use afrobim_kernel::{Document, ModuleRegistry, Pt};

use crate::check_inputs as inputs;
use crate::report_pack::{design_bases_lines, inputs_lines, keys_of_check};

#[test]
fn a_checks_inputs_section_lists_only_the_inputs_that_belong_to_that_check() {
    let mut doc = Document::default();
    let id = doc.add("sign", "ADS-001", Pt::new(0.0, 0.0), 100.0, 100.0, "Signs", serde_json::Value::Null);
    // A sign holds support inputs and visibility inputs (arbitrary test text).
    doc.find_mut(id).unwrap().properties.insert(inputs::KEY_SUPPORT.to_string(), r#"{"wind":"basic","height":"2.5"}"#.to_string());
    doc.find_mut(id).unwrap().properties.insert("check.visibility".to_string(), r#"{"eye":"1.05","speed":"60"}"#.to_string());
    let support = inputs_lines(&doc, id, "Sign support design (post and foundation)");
    let visibility = inputs_lines(&doc, id, "Sign visibility along the alignment (VISIBILITY, an analysis aid)");
    assert!(support.iter().any(|l| l.contains("wind")) && !support.iter().any(|l| l.contains("eye") || l.contains("speed")), "{support:?}");
    assert!(visibility.iter().any(|l| l.contains("eye")) && !visibility.iter().any(|l| l.contains("wind")), "{visibility:?}");
    // A check that reads no stored inputs shows none.
    assert!(inputs_lines(&doc, id, "Marking rules (Traffic Signs Manual Chapter 5 checks of the marking engine)").is_empty());
    assert!(keys_of_check("Culvert complete check (MoWT DDM 9.4 and FHWA HDS-5 inlet control)").contains(&inputs::KEY_CULVERT));
    assert!(!keys_of_check("Side drain capacity").contains(&inputs::KEY_CULVERT));
    let _ = ModuleRegistry::default();
}

#[test]
fn the_design_basis_section_names_the_speed_rule_and_the_bases_in_use() {
    let (mut doc, a) = {
        let mut doc = Document::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        crate::alignment_glue::import_landxml(&mut doc, &path).unwrap();
        let (a, _) = crate::alignment_glue::alignments(&doc).remove(0);
        (doc, a)
    };
    let before = design_bases_lines(&doc, "UK-TSM-2019").join("\n");
    assert!(before.contains("Speed band rule") && before.contains("no basis is stored") && before.contains("no limits source is stored"), "{before}");
    // A drain on the SANRAL basis and an alignment on the built-in MoWT basis.
    let drain = doc.add(afrobim_drainage::DRAIN_CATEGORY, "SD-1", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
    let mut d = inputs::DrainInputs::default();
    d.set_basis(afrobim_drainage::design::DesignBasis::Sanral);
    inputs::save(&mut doc, drain, inputs::KEY_DRAIN, &d);
    let builtin = crate::design_glue::BuiltinChoice { class: Some(afrobim_design::mowt::RoadClass::II), terrain: Some(afrobim_design::mowt::Terrain::Rolling), speed_kmh: Some(70.0), emax_percent: Some(7.0), ..Default::default() };
    crate::design_glue::store_check(&mut doc, a, &crate::design_glue::AlignCheckInputs { source: crate::design_glue::LimitSource::Builtin, builtin, ..Default::default() });
    let after = design_bases_lines(&doc, "UK-TSM-2019").join("\n");
    assert!(after.contains("SANRAL") && after.contains("SD-1"), "{after}");
    assert!(after.contains("built-in MoWT Road Design Manual Vol. I") && after.contains("70 km/h"), "{after}");
}
