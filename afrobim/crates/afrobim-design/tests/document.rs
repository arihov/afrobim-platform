//! A designed alignment in the document: stored as an ordinary alignment element, editable again,
//! drawn by its module, and usable by the other modules.

use afrobim_alignment::{Alignment, CATEGORY};
use afrobim_corridor::{add_surface, Corridor, CorridorDefinition, Daylight, Link, Slope, Template, Tin};
use afrobim_cost::{add_estimate, Bill, Currency, Decimal, Estimate, Item, MappingRule, Rate};
use afrobim_design::*;
use afrobim_kernel::{Document, ModuleRegistry};

fn design() -> AlignmentDesign {
    AlignmentDesign::new(
        HorizontalDesign::new("D1", 5_000.0, vec![Pi::point(0.0, 0.0), Pi::spiral_curve(600.0, 0.0, 300.0, 80.0, 80.0), Pi::point(1000.0, 500.0)]),
        Some(VerticalDesign::new("D1", vec![VPvi::point(5_000.0, 100.0), VPvi::with_length(5_400.0, 108.0, 150.0), VPvi::point(5_900.0, 101.0)])),
    )
}

#[test]
fn a_designed_alignment_is_stored_like_an_imported_one() {
    let mut doc = Document::default();
    let (did, aid) = add_design(&mut doc, &design()).unwrap();
    let e = doc.find(aid).unwrap();
    assert_eq!(e.category, CATEGORY);
    assert_eq!(e.layer, ALIGNMENTS_LAYER);
    assert!(!doc.layer_visible(ALIGNMENTS_LAYER));
    let stored: Alignment = serde_json::from_value(e.data.clone()).unwrap();
    let built = design().build().unwrap().alignment;
    assert_eq!(stored, built);
    assert!(stored.profile.is_some());
    // Found by name the way the drainage crate finds any alignment, and usable by the corridor.
    let found = afrobim_drainage::alignment_named(&doc, "D1").unwrap();
    assert_eq!(found, built);
    let tin = Tin::from_grid(-200.0, -200.0, 50.0, 50.0, 30, 20, |_, _| 99.0).unwrap();
    let t = Template::symmetric("T", vec![Link::new("lane", 3.5, Slope::Percent(-2.0))], Daylight { cut_run_per_rise: 2.0, fill_run_per_rise: 2.0 }, 0.0);
    let c = Corridor::build(&CorridorDefinition::single("c", t, 20.0), &found, &tin).unwrap();
    assert!(c.sections.len() > 20);
    // The design element round trips.
    assert_eq!(load_design(&doc, did).unwrap().horizontal.pis, design().horizontal.pis);
    assert_eq!(designs(&doc).len(), 1);
}

#[test]
fn storing_an_edit_keeps_the_alignment_element_and_refuses_the_impossible() {
    let mut doc = Document::default();
    let (did, aid) = add_design(&mut doc, &design()).unwrap();
    let mut d = load_design(&doc, did).unwrap();
    d.horizontal.move_pi(2, 1000.0, 800.0).unwrap();
    store_design(&mut doc, did, &d).unwrap();
    // Same element id and name; the data changed.
    let e = doc.find(aid).unwrap();
    assert_eq!(e.name, "D1");
    let stored: Alignment = serde_json::from_value(e.data.clone()).unwrap();
    assert_eq!(stored, d.build().unwrap().alignment);
    // An impossible design is refused and the document is unchanged.
    let before = doc.find(aid).unwrap().data.clone();
    let mut bad = d.clone();
    bad.horizontal.pis[1].radius = 20_000.0;
    assert!(store_design(&mut doc, did, &bad).is_err());
    assert_eq!(doc.find(aid).unwrap().data, before);
    // Adding the same design twice gives distinct names.
    let (_, a2) = add_design(&mut doc, &design()).unwrap();
    assert_ne!(doc.find(a2).unwrap().name, "D1");
}

#[test]
fn the_module_draws_the_plan_and_reports_in_plain_words() {
    let mut doc = Document::default();
    let (did, _) = add_design(&mut doc, &design()).unwrap();
    let mut reg = ModuleRegistry::default();
    register(&mut reg);
    let g = reg.generate(doc.find(did).unwrap()).unwrap();
    assert!(g.drawing.primitives.len() > 20);
    assert_eq!(g.derived["pis"], "3");
    assert_eq!(g.derived["curves"], "1");
    assert!(g.derived["Curve PI 1"].contains("R 300.000 m") && g.derived["Curve PI 1"].contains("Ls 80.000/80.000"));
    assert!(g.notes.iter().all(|n| n.severity != afrobim_kernel::Severity::Error));
    // Labels for a front end that can draw text.
    let labels = plan_labels(&load_design(&doc, did).unwrap()).unwrap();
    assert!(labels.iter().any(|l| l.kind == PlanLabelKind::Pi && l.text.starts_with("PI 1  ")));
    assert!(labels.iter().any(|l| l.text.starts_with("TS ")) && labels.iter().any(|l| l.text.starts_with("SC ")) && labels.iter().any(|l| l.text.starts_with("ST ")));
    assert!(labels.iter().any(|l| l.kind == PlanLabelKind::CurveData && l.text.contains("R=300.00")));
    // With the front end's own formatting the same labels are written in its stations and lengths.
    let d = load_design(&doc, did).unwrap();
    assert_eq!(afrobim_design::plan_labels_with(&d, &|s| format!("{s:.3}"), None).unwrap().iter().filter(|l| l.kind != PlanLabelKind::CurveData).map(|l| l.text.replace("PI ", "")).count(), labels.iter().filter(|l| l.kind != PlanLabelKind::CurveData).count());
    let metres: &dyn Fn(f64) -> String = &|m| format!("{:.1} mm", m * 1000.0);
    let shown = afrobim_design::plan_labels_with(&d, &|s| format!("S{s:.0}"), Some(metres)).unwrap();
    assert!(shown.iter().any(|l| l.kind == PlanLabelKind::Pi && l.text.starts_with("PI 1  S")));
    assert!(shown.iter().any(|l| l.kind == PlanLabelKind::CurvePoint && l.text.starts_with("TS S")));
    assert!(shown.iter().any(|l| l.kind == PlanLabelKind::CurveData && l.text.contains("R=300000.0 mm") && l.text.contains("Ls=80000.0 mm/80000.0 mm") && !l.text.contains("R=300.00")));
    // Without a length formatter the text is the engine's.
    let same = afrobim_design::plan_labels_with(&d, &|s| afrobim_alignment::format_station(s, 3), None).unwrap();
    assert_eq!(same, labels);
    // A design made impossible in the data (as by editing the file) is shown with Error notes.
    let mut broken = load_design(&doc, did).unwrap();
    broken.horizontal.pis[1].radius = 20_000.0;
    let mut e = doc.find(did).unwrap().clone();
    e.data = serde_json::to_value(DesignElement { design: broken, alignment: None }).unwrap();
    let g = reg.generate(&e).unwrap();
    assert!(g.notes.iter().any(|n| n.severity == afrobim_kernel::Severity::Error && n.message.contains("tangent")));
}

#[test]
fn option_sets_persist_and_compare_from_the_document() {
    let mut doc = Document::default();
    let tin = Tin::from_grid(-100.0, -500.0, 50.0, 50.0, 21, 21, |_, _| 100.0).unwrap();
    let surface = add_surface(&mut doc, "Ground", &tin, None).unwrap();
    let mut est = Estimate::new("Fixture", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("1.01", "1", "Fill", "m3")];
    est.bill.rules = vec![MappingRule::new("fill", "1.01", "earthworks.fill", vec![])];
    est.rates.set(Rate { item_code: "1.01".into(), description: "fill".into(), unit: "m3".into(), rate: Decimal::parse("2").unwrap(), currency: "UGX".into(), source_note: "test fixture".into(), date: None });
    let estimate = add_estimate(&mut doc, "Rates", &est).unwrap();

    let t = Template::symmetric("T", vec![Link::new("lane", 5.0, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 2.0, fill_run_per_rise: 2.0 }, 0.0);
    let option = |name: &str, level: f64| AlignmentOption {
        name: name.into(),
        design: AlignmentDesign::new(
            HorizontalDesign::new(name, 0.0, vec![Pi::point(0.0, 0.0), Pi::point(400.0, 0.0)]),
            Some(VerticalDesign::new(name, vec![VPvi::point(0.0, level), VPvi::point(400.0, level)])),
        ),
        corridor: Some(CorridorDefinition::single("c", t.clone(), 50.0)),
        note: String::new(),
    };
    let set = OptionSet { name: "Alternatives".into(), surface: Some(surface), estimate: Some(estimate), options: vec![option("Low", 101.0), option("High", 103.0)], criteria: vec![] };
    let id = add_option_set(&mut doc, &set);
    assert_eq!(load_option_set(&doc, id).unwrap(), set);

    // A third alignment in the document, crossing both options at x = 100.
    let other = AlignmentDesign::new(HorizontalDesign::new("Cross road", 0.0, vec![Pi::point(100.0, -80.0), Pi::point(100.0, 80.0)]), None);
    add_design(&mut doc, &other).unwrap();
    let feats = crossing_features_from_alignments(&doc, &[]);
    assert_eq!(feats.len(), 1);

    let cmp = compare_in_document(&doc, id, feats).unwrap();
    // h = 1: A = 12, V = 4800; h = 3: A = 10 x 3 + 2 x 9 = 48, V = 19200. Rate 2 per m3.
    assert!((cmp.options[0].value("fill_m3").unwrap() - 4800.0).abs() < 1e-6);
    assert!((cmp.options[1].value("fill_m3").unwrap() - 19200.0).abs() < 1e-6);
    assert!((cmp.options[0].value("cost").unwrap() - 9600.0).abs() < 1e-6);
    assert!((cmp.options[1].value("cost").unwrap() - 38400.0).abs() < 1e-6);
    assert_eq!(cmp.options[0].value("crossings"), Some(1.0));
    assert_eq!(cmp.options[0].figure("cost").unwrap().unit, "UGX");

    // The module draws both options.
    let mut reg = ModuleRegistry::default();
    register(&mut reg);
    let g = reg.generate(doc.find(id).unwrap()).unwrap();
    assert!(!g.drawing.primitives.is_empty());
    assert_eq!(g.derived["options"], "2");
    // Editing the stored set.
    let mut s2 = set.clone();
    s2.options.pop();
    store_option_set(&mut doc, id, &s2).unwrap();
    assert_eq!(load_option_set(&doc, id).unwrap().options.len(), 1);
}
