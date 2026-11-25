// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The collectors and the estimate module against real elements of the other crates.

mod common;

use afrobim_alignment::{AlignmentBuilder, Turn};
use afrobim_cost::*;
use afrobim_draw::{Colour, Primitive};
use afrobim_fabrication::SignQuantity;
use afrobim_kernel::{Document, ElementId, ModuleRegistry, Pt, Severity};
use afrobim_markings::{add_marking, LineType, MarkingDefinition};
use afrobim_signs::{ColourScheme, SignKind};
use afrobim_supports::{FoundationDefinition, FoundationShape, PostDefinition, Section as PostSection, FOUNDATION_CATEGORY, POST_CATEGORY};
use common::*;

fn close(a: Decimal, expected: f64, tol: f64) {
    assert!((a.to_f64() - expected).abs() <= tol, "{a} vs {expected}");
}

struct Model {
    doc: Document,
    marking: ElementId,
    post: ElementId,
    foundation: ElementId,
    bad_post: ElementId,
}

/// A document with one marking, a post, a foundation and a post that cannot be measured.
fn model() -> Model {
    let alignment = AlignmentBuilder::new("MC10", 0.0, 1000.0, 5000.0, std::f64::consts::FRAC_PI_2).line(200.0).arc(300.0, 100.0, Turn::Right).build().unwrap();
    let mut doc = Document::default();
    let al = doc.add(afrobim_alignment::CATEGORY, "MC10", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&alignment).unwrap());
    // Centre line 1008, 100 mm, stations 0 to 60 m: ten 2 m marks, 20 m painted, 2.000 m2.
    let def = MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(100).with_material("thermoplastic");
    let marking = add_marking(&mut doc, al, "RM-001", &def).unwrap();
    // CHS D100 t10, 3000 long: area 2827.433 mm2, volume 0.0084823 m3, coating pi x 100 x 3000 = 0.942478 m2.
    let post_def = PostDefinition {
        section: PostSection::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 },
        length_mm: 3000.0,
        embedment_mm: 1000.0,
        material: "S355".into(),
        finish: "galvanised".into(),
        placeholder: false,
    };
    let post = doc.add(POST_CATEGORY, "P-1", Pt::new(0.0, 0.0), 100.0, 3000.0, "Posts", post_def.to_value());
    // Block 1000 x 800 x 600, top at ground: 0.480 m3 of concrete, sides 2 x (1.0 + 0.8) x 0.6 = 2.160 m2.
    let fdef = FoundationDefinition {
        shape: FoundationShape::Block { length_mm: 1000.0, width_mm: 800.0, depth_mm: 600.0 },
        top_below_ground_mm: 0.0,
        concrete: "C25".into(),
        blinding_mm: None,
        socket: None,
        placeholder: false,
    };
    let foundation = doc.add(FOUNDATION_CATEGORY, "F-1", Pt::new(0.0, 0.0), 1000.0, 800.0, "Foundations", fdef.to_value());
    let bad_def = PostDefinition { length_mm: 0.0, ..post_def };
    let bad_post = doc.add(POST_CATEGORY, "P-BAD", Pt::new(0.0, 0.0), 100.0, 3000.0, "Posts", bad_def.to_value());
    Model { doc, marking, post, foundation, bad_post }
}

fn line<'a>(set: &'a QuantitySet, mark: &str, category: &str) -> &'a QuantityLine {
    set.lines.iter().find(|l| l.mark == mark && l.category == category).unwrap_or_else(|| panic!("no {category} for {mark}"))
}

#[test]
fn markings_are_collected_with_their_element_and_attributes() {
    let m = model();
    let mut set = QuantitySet::new();
    collect_markings(&m.doc, &mut set);
    assert_eq!(set.lines.len(), 4);
    let a = line(&set, "RM-001", "marking.painted_area");
    assert_eq!((a.element, a.unit.as_str()), (Some(m.marking), "m2"));
    close(a.quantity, 2.0, 1e-6);
    close(line(&set, "RM-001", "marking.painted_length").quantity, 20.0, 1e-6);
    close(line(&set, "RM-001", "marking.length").quantity, 60.0, 1e-3);
    assert_eq!(line(&set, "RM-001", "marking.marks").quantity, d("10"));
    assert_eq!(a.attributes["width_mm"], "100");
    assert_eq!(a.attributes["material"], "thermoplastic");
    assert_eq!(a.attributes["alignment"], "MC10");
    assert_eq!(a.attributes["colour"], "white");
}

#[test]
fn posts_and_foundations_are_collected_and_unmeasurable_ones_are_skipped() {
    let m = model();
    let mut set = QuantitySet::new();
    collect_supports(&m.doc, &Default::default(), &mut set);
    let len = line(&set, "P-1", "post.length");
    assert_eq!((len.element, len.quantity), (Some(m.post), d("3")));
    assert_eq!(line(&set, "P-1", "post.count").quantity, d("1"));
    close(line(&set, "P-1", "post.coating_area").quantity, 0.942478, 1e-6);
    close(line(&set, "P-1", "post.mass").quantity, 66.586056, 1e-5);
    assert_eq!(len.attributes["section"], "CHS D100 t10");
    assert_eq!(len.attributes["finish"], "galvanised");
    let f = line(&set, "F-1", "foundation.concrete");
    assert_eq!((f.element, f.quantity), (Some(m.foundation), d("0.48")));
    close(line(&set, "F-1", "foundation.formwork").quantity, 2.16, 1e-9);
    assert_eq!(f.attributes["concrete"], "C25");
    // The post with no length is not measured; the reason comes from the supports crate.
    assert!(set.lines.iter().all(|l| l.mark != "P-BAD"));
    assert_eq!(set.skipped.len(), 1);
    assert_eq!((set.skipped[0].source.as_str(), set.skipped[0].mark.as_str()), ("supports", "P-BAD"));
    assert!(set.skipped[0].reason.contains("length"), "{}", set.skipped[0].reason);
    assert_eq!(m.bad_post, m.doc.elements().iter().find(|e| e.name == "P-BAD").unwrap().id);
}

#[test]
fn sign_faces_are_collected_from_the_take_off_rows() {
    let mut doc = Document::default();
    let id = doc.add(SIGN_CATEGORY, "ADS-001", Pt::new(0.0, 0.0), 1.0, 1.0, "Signs", serde_json::Value::Null);
    let q = |code: &str, w: f64, h: f64| SignQuantity {
        code: code.into(),
        catalog_key: Some("A1".into()),
        kind: SignKind::default(),
        scheme: ColourScheme::default(),
        ground: Colour::Green,
        speed_kph: 80,
        x_height_mm: 200.0,
        sw_mm: 50.0,
        width_mm: w,
        height_mm: h,
        legend_characters: 12,
        symbols: 1,
        warnings: 0,
        errors: 0,
    };
    let mut set = QuantitySet::new();
    collect_signs(&doc, &[q("ADS-001", 2000.0, 1500.0), q("NOT-IN-DOC", 600.0, 600.0)], &mut set);
    let area = line(&set, "ADS-001", "sign.face_area");
    assert_eq!((area.element, area.quantity, area.unit.as_str()), (Some(id), d("3"), "m2"));
    assert_eq!(area.attributes["ground"], "Green");
    assert_eq!(area.attributes["width_mm"], "2000");
    assert_eq!(line(&set, "ADS-001", "sign.legend").quantity, d("12"));
    // A sign whose element is not in the document is still measured, without an element.
    assert_eq!(line(&set, "NOT-IN-DOC", "sign.face").element, None);
    assert_eq!(line(&set, "NOT-IN-DOC", "sign.face_area").quantity, d("0.36"));
}

#[test]
fn earthworks_totals_are_collected() {
    let mut set = QuantitySet::new();
    let ew = afrobim_corridor::Earthworks {
        rows: vec![],
        total_cut_m3: 1250.5,
        total_fill_m3: 980.25,
        unmeasured_length_m: 0.0,
        options: Default::default(),
    };
    collect_earthworks(ElementId(7), "CORR-1", &ew, &mut set);
    assert_eq!(line(&set, "CORR-1", "earthworks.cut").quantity, d("1250.5"));
    assert_eq!(line(&set, "CORR-1", "earthworks.fill").quantity, d("980.25"));
    assert_eq!(set.lines[0].element, Some(ElementId(7)));
}

/// A bill for the model above: marking area at 100 mm, posts by metre, concrete by grade.
fn model_estimate() -> Estimate {
    let mut est = Estimate::new("Model estimate", Currency::new("UGX", 0));
    est.bill = Bill::neutral(2);
    est.bill.items = vec![
        Item::measured("1", "1", "Thermoplastic marking 100 mm", "m2"),
        Item::measured("2", "2", "Steel posts CHS D100 t10", "m"),
        Item::measured("3", "2", "Concrete C25", "m3"),
    ];
    est.bill.rules = vec![
        MappingRule::new("marking", "1", "marking.painted_area", vec![Condition::equals("width_mm", "100"), Condition::equals("material", "thermoplastic")]),
        MappingRule::new("posts", "2", "post.length", vec![Condition::equals("section", "CHS D100 t10")]),
        MappingRule::new("concrete", "3", "foundation.concrete", vec![Condition::equals("concrete", "C25")]),
    ];
    for (code, unit, rate) in [("1", "m2", "15000"), ("2", "m", "40000"), ("3", "m3", "500000")] {
        est.rates.set(Rate { item_code: code.into(), description: "".into(), unit: unit.into(), rate: d(rate), currency: "UGX".into(), source_note: "fixture".into(), date: None });
    }
    est
}

#[test]
fn an_estimate_lives_in_the_document_and_draws_a_summary() {
    let mut m = model();
    let id = add_estimate(&mut m.doc, "Estimate 1", &model_estimate()).unwrap();
    assert_eq!(estimates(&m.doc), vec![(id, "Estimate 1".to_string())]);
    let mut registry = ModuleRegistry::default();
    registry.register(Box::new(EstimateModule));

    // Before a computation: an Info note and no drawing.
    let g = registry.generate(m.doc.find(id).unwrap()).unwrap();
    assert_eq!(g.notes[0].code, "estimate.not_computed");
    assert!(g.drawing.primitives.is_empty());

    // Refresh: collects the document's quantities, prices them, stores the summary.
    let r = refresh_estimate(&mut m.doc, id, &[], &Default::default()).unwrap();
    // Marking 2.000 m2 x 15000 = 30000; post 3 m x 40000 = 120000; concrete 0.48 m3 x 500000 = 240000.
    assert_eq!(r.item("1").unwrap().amount_reporting, d("30000"));
    assert_eq!(r.item("2").unwrap().amount_reporting, d("120000"));
    assert_eq!(r.item("3").unwrap().amount_reporting, d("240000"));
    assert_eq!(r.grand_total, d("390000"));
    // Everything the bill did not take is reported: the lines of the other categories.
    assert!(r.measured.unmapped.len() > 5);
    assert_eq!(r.measured.lines_mapped + r.measured.unmapped.len(), r.measured.lines_offered);
    // The item's audit trail names the elements.
    assert_eq!(r.measured.item("1").unwrap().elements(), vec![m.marking]);
    assert_eq!(r.measured.item("2").unwrap().elements(), vec![m.post]);

    let g = registry.generate(m.doc.find(id).unwrap()).unwrap();
    assert_eq!(g.derived["grand_total"], "390000");
    assert_eq!(g.derived["section_1"], "30000");
    assert_eq!(g.derived["section_2"], "360000");
    assert_eq!(g.derived["currency"], "UGX");
    assert_eq!(g.derived["skipped_sources"], "1");
    let codes: Vec<&str> = g.notes.iter().map(|n| n.code.as_str()).collect();
    assert!(codes.contains(&"estimate.unmapped_lines") && codes.contains(&"estimate.skipped_sources"), "{codes:?}");
    assert!(g.notes.iter().all(|n| n.severity == Severity::Warning));
    // The drawing: the axis, then one bar per section; section 2 (360000) is the longest.
    assert_eq!(g.drawing.primitives.len(), 3);
    let bar = |i: usize| match &g.drawing.primitives[i] {
        Primitive::Fill { outline, colour } => {
            assert_eq!(*colour, Colour::Blue);
            outline[1].x
        }
        other => panic!("{other:?}"),
    };
    assert!((bar(2) - SUMMARY_BAR_MAX_MM).abs() < 1e-9);
    assert!((bar(1) - SUMMARY_BAR_MAX_MM * 30000.0 / 360000.0).abs() < 1e-9);
    assert_eq!(g.drawing.extents.max.x, SUMMARY_BAR_MAX_MM);

    // The element round-trips through the document file.
    let again = Document::from_json(&m.doc.to_json()).unwrap();
    assert_eq!(estimate_of(&again, id).unwrap(), estimate_of(&m.doc, id).unwrap());
    // Replacing the estimate clears the stored summary.
    set_estimate(&mut m.doc, id, &model_estimate()).unwrap();
    assert!(estimate_of(&m.doc, id).unwrap().computed.is_none());
    assert!(set_estimate(&mut m.doc, m.post, &model_estimate()).is_err());
    assert!(refresh_estimate(&mut m.doc, m.post, &[], &Default::default()).is_err());
}

#[test]
fn collect_all_runs_every_collector() {
    let m = model();
    let set = collect_all(&m.doc, &[], &CollectOptions::default());
    let cats: Vec<String> = set.totals_by_category().into_iter().map(|(c, _, _)| c).collect();
    for c in ["marking.painted_area", "post.length", "foundation.concrete"] {
        assert!(cats.iter().any(|x| x == c), "{c} missing from {cats:?}");
    }
    // Line ids are positions; the CSV lists every line.
    assert!(set.lines.iter().enumerate().all(|(i, l)| l.id == i));
    assert_eq!(lines_to_csv(&set).lines().count(), set.lines.len() + 1);
}
