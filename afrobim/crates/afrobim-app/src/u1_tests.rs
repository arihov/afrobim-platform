//! Glue tests that cross the new modules: annotation through save and open and the DXF export,
//! layers, units, annotation on a composed plan sheet, the speed band in the checks.

use afrobim_annotate as ann;
use afrobim_annotate::geometry::pt;
use afrobim_kernel::{Document, ElementId, SpeedBandRule};

use crate::alignment_glue;
use crate::anno_glue::{self, Kind, Session};

fn doc_with_alignment() -> (Document, ElementId) {
    let mut doc = Document::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
    alignment_glue::import_landxml(&mut doc, &path).unwrap();
    let (id, _) = alignment_glue::alignments(&doc).remove(0);
    (doc, id)
}

fn make(doc: &mut Document, kind: Kind, steps: &[&str]) -> ElementId {
    let mut s = Session::start(kind, doc, anno_glue::LAYER, None).unwrap();
    let mut created = None;
    for st in steps {
        let out = if st.is_empty() { s.enter(doc) } else { s.text(doc, st) };
        created = created.or(out.created);
    }
    created.unwrap_or_else(|| panic!("{kind:?} made nothing"))
}

#[test]
fn every_annotation_type_survives_save_and_open_and_reaches_the_dxf_export() {
    let (mut doc, id) = doc_with_alignment();
    let a = alignment_glue::alignment_of(&doc, id).unwrap();
    let on = a.point_at(a.start_station_m + 400.0).unwrap();
    let site = pt(on.x * 1000.0, on.y * 1000.0);
    let target = doc.add("sign", "ADS-001", pt(0.0, 0.0), 2000.0, 1000.0, "Signs", serde_json::json!({}));
    let mut ids = vec![
        make(&mut doc, Kind::Text, &["0,0", "", "", "Kerb to be removed"]),
        make(&mut doc, Kind::MText, &["0,2", "5,4", "Two lines\\nof text"]),
        make(&mut doc, Kind::DimAligned, &["0,0", "3,4", "-1,2"]),
        make(&mut doc, Kind::DimAngular, &["0,0", "4,0", "0,4", "2,2"]),
        make(&mut doc, Kind::DimRadius, &["0,0", "2.5,0", "1.5,1.5"]),
        make(&mut doc, Kind::DimArc, &["0,0", "2,0", "0,2", "2.6,2.6"]),
        make(&mut doc, Kind::DimLevel, &["9,9", "101.25"]),
        make(&mut doc, Kind::Leader, &["10,10", "12,12", "", "R61"]),
    ];
    // A station and offset dimension, and a leader that shows a mark.
    let mut s = Session::start(Kind::DimStation, &doc, anno_glue::LAYER, None).unwrap();
    s.point(&mut doc, site, 5.0);
    ids.push(s.point(&mut doc, pt(site.x + 3000.0, site.y + 3000.0), 5.0).created.unwrap());
    let mut s = Session::start(Kind::MLeader, &doc, anno_glue::LAYER, None).unwrap();
    s.point(&mut doc, pt(500.0, 500.0), 5.0);
    ids.push(s.point(&mut doc, pt(3000.0, 2500.0), 5.0).created.unwrap());
    assert_eq!(ids.len(), 10);

    // Save and open: every annotation is read back equal, with its text and its layer.
    let back = Document::from_json(&doc.to_json()).unwrap();
    for id in &ids {
        let (a, b) = (doc.find(*id).unwrap(), back.find(*id).unwrap());
        // serde_json reads a float to within the last digit, so the points are compared to a micrometre.
        let (ga, gb) = (anno_glue::grips_of(&doc, *id), anno_glue::grips_of(&back, *id));
        assert_eq!(ga.len(), gb.len());
        assert!(ga.iter().zip(&gb).all(|(p, q)| (p.x - q.x).abs() < 1e-3 && (p.y - q.y).abs() < 1e-3), "{} points differ after save and open", a.name);
        assert_eq!(anno_glue::describe(&doc, *id), anno_glue::describe(&back, *id));
        assert_eq!(a.layer, b.layer);
    }
    // The station dimension still shows the displayed station (1450 after the first station equation).
    let (_, summary) = anno_glue::describe(&back, ids[8]);
    assert!(summary.contains("1+450"), "{summary}");

    // The DXF export: real TEXT entities with the text, and the linework, read back by the reader.
    let mut dxf = afrobim_dxf::DxfDocument::new();
    let report = ann::export_dxf(&doc, &mut dxf);
    assert_eq!(report.annotations, 10);
    let text = dxf.finish();
    for needle in ["Kerb to be removed", "Two lines", "of text", "R61", "ADS-001", "101.250"] {
        assert!(text.contains(needle), "the DXF lacks '{needle}'");
    }
    let under = afrobim_dxf::reader::read_bytes(text.as_bytes()).unwrap();
    assert!(under.report.read.get("TEXT").copied().unwrap_or(0) >= 12, "{:?}", under.report.read);
    assert!(under.report.read.get("LINE").copied().unwrap_or(0) > 10);
    let _ = target;
}

#[test]
fn a_layer_rename_keeps_its_elements_and_a_delete_is_refused_unless_empty() {
    let mut doc = Document::default();
    ann::layers::create(&mut doc, "KERBS", ann::LayerProps { colour: [200, 40, 40], lineweight_mm: 0.35 }).unwrap();
    doc.ensure_layer("KERBS");
    let id = doc.add("sign", "S1", pt(0.0, 0.0), 100.0, 100.0, "KERBS", serde_json::json!({}));
    ann::layers::rename(&mut doc, "KERBS", "EDGES").unwrap();
    assert_eq!(doc.find(id).unwrap().layer, "EDGES", "the element moved with its layer");
    let info = ann::layers::list(&doc).into_iter().find(|l| l.name == "EDGES").unwrap();
    assert_eq!((info.elements, info.colour, info.lineweight_mm), (1, [200, 40, 40], 0.35), "colour and weight moved too");
    assert!(matches!(ann::layers::delete(&mut doc, "EDGES", &ann::OnDelete::RefuseUnlessEmpty), Err(ann::LayerError::InUse { count: 1, .. })));
    assert!(doc.layers().iter().any(|l| l.name == "EDGES"), "refused: the layer is still there");
    // Moved to another layer, then it goes.
    ann::layers::create(&mut doc, "OTHER", ann::LayerProps::default()).unwrap();
    assert_eq!(ann::layers::delete(&mut doc, "EDGES", &ann::OnDelete::MoveTo("OTHER".into())), Ok(1));
    assert_eq!(doc.find(id).unwrap().layer, "OTHER");
    // The hidden settings layer is not listed, and the layer manager hides the underlay layer too.
    assert!(ann::layers::list(&doc).iter().all(|l| l.name != ann::SETTINGS_LAYER));
    assert!(crate::anno_ui::hidden_layer(crate::underlay::LAYER));
    // Layer 0 cannot be renamed or deleted.
    assert!(ann::layers::rename(&mut doc, "0", "ZERO").is_err());
    // A name DXF forbids is refused.
    assert!(ann::layers::create(&mut doc, "BAD/NAME", ann::LayerProps::default()).is_err());
    // Layer colour and weight are part of undo (they live in the settings element); on and lock
    // are not (the kernel keeps them outside its snapshot): state both.
    let before = doc.snapshot();
    ann::layers::set_colour(&mut doc, "OTHER", [1, 2, 3]).unwrap();
    assert_ne!(ann::layers::props(&doc, "OTHER").colour, [0, 0, 0]);
    doc.restore(before);
    assert_eq!(ann::layers::props(&doc, "OTHER").colour, [0, 0, 0], "a colour change is undone");
}

#[test]
fn units_format_a_known_value_and_the_same_text_parses_back() {
    let mut doc = Document::default();
    let mut st = ann::settings::load(&doc);
    st.units.length = ann::LengthUnit::Millimetre;
    st.units.length_decimals = 1;
    st.units.station_decimals = 2;
    st.units.area = ann::AreaUnit::Hectare;
    st.units.area_decimals = 3;
    st.units.angle = ann::AngleFormat::Dms;
    st.units.angle_decimals = 0;
    ann::settings::store(&mut doc, &st);
    let u = anno_glue::units(&doc);
    assert_eq!(u.length_u(12_345.678), "12345.7 mm");
    assert_eq!(u.parse_length("12345.7 mm").unwrap(), 12_345.7);
    assert_eq!(u.station(1250.5), "1+250.50");
    assert!((u.parse_station("1+250.50").unwrap() - 1250.5).abs() < 1e-9);
    assert_eq!(u.area_u(25_000_000_000.0), "2.500 ha");
    assert!((u.parse_area("2.500 ha").unwrap() - 25_000_000_000.0).abs() < 1.0);
    let shown = u.angle(37.5);
    assert!((u.parse_angle(&shown).unwrap() - 37.5).abs() < 1e-6, "{shown}");
    // A typed point in the display unit (millimetres here, then metres).
    assert_eq!(anno_glue::parse_point("1500,2500", None, &u), Some(pt(1500.0, 2500.0)));
    st.units.length = ann::LengthUnit::Metre;
    ann::settings::store(&mut doc, &st);
    assert_eq!(anno_glue::parse_point("1.5,2.5", None, &anno_glue::units(&doc)), Some(pt(1500.0, 2500.0)));
    // Stored values stay metric: the document holds millimetres whatever is shown.
    let (_, ann_id) = (0, make(&mut doc, Kind::DimAligned, &["0,0", "3,4", "-1,2"]));
    let Ok(ann::Ann::Dim(d)) = ann::read_absolute(doc.find(ann_id).unwrap()) else { panic!() };
    let ann::DimKind::Linear { p2, .. } = d.kind else { panic!() };
    assert_eq!((p2.x, p2.y), (3000.0, 4000.0));
}

#[test]
fn annotation_inside_a_plan_sheets_window_is_drawn_on_the_composed_sheet() {
    use afrobim_sheets::{Item, Model, ProjectInfo};
    // The demo sheet set: a plan series on the corridor alignment (see `sheets_glue::demo`).
    let (mut doc, _) = doc_with_alignment();
    let problems = crate::sheets_glue::demo(&mut doc);
    assert!(problems.is_empty(), "{problems:?}");
    let mut registry = afrobim_kernel::ModuleRegistry::default();
    crate::alignment_glue::register(&mut registry);
    afrobim_annotate::register(&mut registry);
    crate::sheets_glue::register(&mut registry);
    let set_id = afrobim_sheets::sheet_sets(&doc).first().map(|s| s.0).expect("the demo set");
    let set = afrobim_sheets::load_sheet_set(&doc, set_id).unwrap();
    let model = Model { doc: &doc, registry: &registry };
    let plain = crate::sheets_glue::compose_with_annotations(&model, &set, &ProjectInfo::default());
    // A text note at the start of the alignment, sized for the plan's own scale (demo 1:2000).
    let (_, a) = anno_glue::alignment_list(&doc).remove(0);
    let p = a.point_at(a.start_station_m + 50.0).unwrap();
    let mut st = ann::settings::load(&doc);
    st.annotation_scale = 2000.0;
    ann::settings::store(&mut doc, &st);
    let t = ann::TextData::new("PLAN NOTE", pt(p.x * 1000.0, p.y * 1000.0), &st.current_text(), 2000.0);
    ann::add_text(&mut doc, t, anno_glue::LAYER).unwrap();
    let model = Model { doc: &doc, registry: &registry };
    let with = crate::sheets_glue::compose_with_annotations(&model, &set, &ProjectInfo::default());
    let texts = |s: &afrobim_sheets::ComposedSet| s.sheets.iter().flat_map(|sh| sh.items.iter()).filter(|i| matches!(i, Item::Text(t) if t.text == "PLAN NOTE")).count();
    assert_eq!(texts(&plain), 0);
    assert!(texts(&with) >= 1, "the note is on a plan sheet");
    assert!(with.sheets.iter().map(|s| s.items.len()).sum::<usize>() > plain.sheets.iter().map(|s| s.items.len()).sum::<usize>());
}

#[test]
fn a_changed_rule_changes_the_speed_band_note() {
    let mut doc = Document::default();
    afrobim_kernel::set_speed_band_rule(&mut doc, &SpeedBandRule::Exact);
    assert!(crate::projset::resolve(&doc, 80.0, false, &crate::projset::restraint_thresholds()).band_note.contains("exact conversion"));
}
