// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Glue tests: the three drainage bases, the complete culvert check and the SANRAL methods.
//! Every number is an arbitrary test value.

use afrobim_drainage::design::DesignBasis;
use afrobim_kernel::{Document, ElementId, ModuleRegistry};

use crate::alignment_glue;
use crate::check_all::{run_all, Report, Sources, Status};
use crate::check_inputs::{self as inputs, CulvertInputs, DrainInputs, RationalInputs};

fn doc_with_alignment() -> (Document, ElementId) {
    let mut doc = Document::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
    alignment_glue::import_landxml(&mut doc, &path).unwrap();
    let (id, _) = alignment_glue::alignments(&doc).remove(0);
    (doc, id)
}

/// The model-wide report with every module the tests need registered.
fn report_of(doc: &Document) -> Report {
    let mut modules = ModuleRegistry::default();
    crate::modules_glue::register(&mut modules);
    crate::markings_glue::register(&mut modules);
    crate::supports_glue::register(&mut modules);
    let notes = |id: ElementId| doc.find(id).map(|e| modules.generate(e).map(|g| g.notes));
    let label = |c: &str| c.to_string();
    run_all(doc, &Sources { notes_of: &notes, label_of: &label, vis: None })
}

fn culvert_def(alignment: &str) -> afrobim_drainage::PipeCulvertDefinition {
    // A 900 mm bore, 12 m long, inverts 10.24 m and 10.0 m (arbitrary test values).
    afrobim_drainage::PipeCulvertDefinition {
        alignment: alignment.into(),
        station_m: 1250.0,
        skew_deg: 0.0,
        barrels: 1,
        internal_diameter_mm: 900.0,
        wall_thickness_mm: 100.0,
        length_mm: 12_000.0,
        barrel_spacing_mm: None,
        invert_level_inlet_m: 10.24,
        invert_level_outlet_m: 10.0,
        inlet_side: afrobim_drainage::Side::Left,
        centre_offset_m: 0.0,
        road_level_m: None,
        material: "as specified".into(),
        placeholder: false,
    }
}

fn alignment_name(doc: &Document, id: ElementId) -> String {
    doc.find(id).unwrap().name.clone()
}

#[test]
fn the_drainage_basis_round_trips_and_an_old_stored_boolean_still_loads() {
    // Stored before SANRAL: only the yes/no field.
    let old: DrainInputs = serde_json::from_str(r#"{"flow_m3_s":"0.3","flow_from_rational":false,"dmrb":true,"required_freeboard_m":""}"#).unwrap();
    assert_eq!(old.basis(), DesignBasis::Dmrb);
    let old_mowt: CulvertInputs = serde_json::from_str(r#"{"flow_m3_s":"0.3","flow_from_rational":false,"manning_n":"","entrance_ke":"","entrance_note":"","tailwater_m":"","dmrb":false,"shoulder_level_m":"","major_road":null}"#).unwrap();
    assert_eq!(old_mowt.basis(), DesignBasis::Mowt);
    // Each of the three bases survives a round trip through the stored text.
    for b in DesignBasis::EVERY {
        let mut d = DrainInputs::default();
        d.set_basis(b);
        let back: DrainInputs = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.basis(), b);
    }
}

#[test]
fn the_complete_culvert_check_names_the_governing_control_and_shows_outlet_lines_first() {
    let (mut doc, a) = doc_with_alignment();
    let def = culvert_def(&alignment_name(&doc, a));
    let id = afrobim_drainage::add_pipe_culvert(&mut doc, "CU-T", &def).unwrap();
    assert!(!inputs::circular_inlets().is_empty());
    // Inlet edge and allowable headwater start empty; the row then says the check is incomplete.
    let stored = CulvertInputs { flow_m3_s: "0.3".into(), manning_n: "0.013".into(), entrance_ke: "0.5".into(), tailwater_m: "0.2".into(), ..Default::default() };
    inputs::save(&mut doc, id, inputs::KEY_CULVERT, &stored);
    let r = report_of(&doc);
    let row = r.rows.iter().find(|x| x.element == id && x.check.starts_with("Culvert outlet control")).unwrap();
    assert_eq!(row.status, Status::InputNeeded);
    assert!(row.reason.contains("outlet control only"), "{}", row.reason);
    // Entered: the complete check runs, the outlet lines come first and the governing control is named.
    let full = CulvertInputs { inlet_edge: Some(0), allowable_headwater_m: "5".into(), ..stored };
    inputs::save(&mut doc, id, inputs::KEY_CULVERT, &full);
    let r = report_of(&doc);
    let row = r.rows.iter().find(|x| x.element == id && x.check.starts_with("Culvert complete check")).unwrap();
    assert!(matches!(row.status, Status::Passes | Status::Fails), "{row:?}");
    assert!(row.reason.contains("control governs"), "{}", row.reason);
    let outlet_first = def.outlet_control(0.3, 0.013, 0.5, 0.2).unwrap().calculation();
    assert_eq!(&row.lines[..outlet_first.len()], &outlet_first[..], "the existing outlet lines are shown first, unchanged");
    assert!(row.lines.len() > outlet_first.len(), "the additional lines follow");
}

#[test]
fn the_sanral_culvert_check_needs_the_slope_unit_answered_and_the_sanral_rational_runs() {
    let (mut doc, a) = doc_with_alignment();
    let def = culvert_def(&alignment_name(&doc, a));
    let id = afrobim_drainage::add_pipe_culvert(&mut doc, "CU-S", &def).unwrap();
    let mut stored = CulvertInputs { flow_m3_s: "0.3".into(), ..Default::default() };
    stored.set_basis(DesignBasis::Sanral);
    stored.sanral = inputs::SanralCulvertEntries { k_in: "0.5".into(), k_out: "1".into(), friction_mode: 0, friction_value: "0.013".into(), h2_m: "0.2".into(), road_class: Some(2), ..Default::default() };
    inputs::save(&mut doc, id, inputs::KEY_CULVERT, &stored);
    let r = report_of(&doc);
    let row = r.rows.iter().find(|x| x.element == id && x.check.starts_with("Culvert SANRAL")).unwrap();
    assert_eq!(row.status, Status::InputNeeded);
    assert!(row.reason.contains("how the slope is read"), "{}", row.reason);
    stored.sanral.slope_unit = Some(0);
    inputs::save(&mut doc, id, inputs::KEY_CULVERT, &stored);
    let r = report_of(&doc);
    let row = r.rows.iter().find(|x| x.element == id && x.check.starts_with("Culvert SANRAL")).unwrap();
    assert!(matches!(row.status, Status::Passes | Status::Fails), "{row:?}");
    assert!(!row.lines.is_empty());

    // The SANRAL rational method runs on entered shares and stays separate from the MoWT one.
    let mut s = inputs::SanralRational::default();
    assert!(s.run().is_err());
    s.area_km2 = "2".into();
    s.map_mm = "700".into();
    s.slope = ["".into(), "100".into(), "".into(), "".into()];
    s.permeability = ["".into(), "".into(), "100".into(), "".into()];
    s.vegetation = ["".into(), "".into(), "100".into(), "".into()];
    s.dolomite_percent = "0".into();
    s.ft_class = Some(1);
    s.alpha = "1".into();
    s.beta = "0".into();
    s.gamma = "0".into();
    s.return_period = "20".into();
    s.reaches = vec![inputs::ReachRow { kind: 1, r: String::new(), length: "3".into(), slope_or_velocity: "0.02".into() }];
    s.point_rainfall_mm = "80".into();
    s.area_reduction = 2;
    let (q, lines) = s.run().unwrap();
    assert!(q > 0.0 && !lines.is_empty());
    // The shared run_flow takes the SANRAL route only when it is chosen.
    let r = RationalInputs { use_sanral: true, sanral: s, ..Default::default() };
    assert!((r.run_flow().unwrap().0 - q).abs() < 1e-12);
    assert!(RationalInputs::default().run_flow().is_err());
}
