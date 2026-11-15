//! Glue tests for the second marking set, the South African pavement basis, the three-way
//! drainage basis, the MoWT alignment checks, IFC options, per-check report inputs and
//! background runs. Every number here is an arbitrary test value.

use afrobim_kernel::{Document, ElementId};

use crate::alignment_glue;
use crate::road_glue;

fn doc_with_alignment() -> (Document, ElementId) {
    let mut doc = Document::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
    alignment_glue::import_landxml(&mut doc, &path).unwrap();
    let (id, _) = alignment_glue::alignments(&doc).remove(0);
    (doc, id)
}

#[allow(dead_code)]
fn fixture() -> (Document, ElementId) {
    doc_with_alignment()
}

// ─── South African pavement basis ───────────────────────────────────────────────────────

#[test]
fn the_sa_method_starts_empty_runs_with_entries_and_offers_no_layers() {
    let mut form = road_glue::DesignForm { method: road_glue::SA_METHOD, ..Default::default() };
    assert_eq!(form.trh4, road_glue::Trh4Form::default());
    // Empty fields are refused with a request for the first missing one, never defaulted.
    assert!(form.run().unwrap_err().contains("road category"));
    form.trh4.category = Some(1);
    assert!(form.run().unwrap_err().contains("daily E80"));
    form.trh4.daily_e80_per_lane = Some(300.0);
    form.trh4.hv_growth_percent = Some(4.0);
    form.trh4.e80_growth_percent = Some(2.0);
    form.trh4.years_survey_to_opening = Some(1.0);
    form.trh4.design_period_years = Some(20.0);
    form.trh4.climate_region = Some(1);
    form.trh4.subgrade_cbr_percent = Some(9.0);
    let out = form.run().unwrap();
    assert!(out.layers.is_none(), "the TRH4 catalogue is not available: no layers");
    let text = out.lines.join("\n");
    assert!(text.contains("Traffic class") && text.contains("catalogue") && text.contains("E80"), "{text}");
    // An old stored form without the field still loads, with the SA inputs empty.
    let old = r#"{"method":1,"mowt":{},"dmrb":{},"aashto":{}}"#;
    let loaded: road_glue::DesignForm = serde_json::from_str(old).unwrap();
    assert_eq!(loaded.trh4, road_glue::Trh4Form::default());
    assert_eq!(road_glue::methods().len(), 4);
}
