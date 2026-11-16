//! Tests of the round 6 work package A1: the defaults in the interface, pipe networks, grading and
//! intersections as document elements.

use afrobim_defaults::Conditions;

use crate::defaults_glue as dg;

#[test]
fn the_defaults_basis_gives_a_cited_value_or_nothing() {
    dg::set_basis("dmrb");
    let v = dg::get("link.ssd.desirable_min", &Conditions::none().design_speed(100.0)).expect("a value at 100 km/h");
    assert_eq!(v.number(), Some(215.0));
    assert!(dg::citation_line(&v).contains("CD 109"), "{}", dg::citation_line(&v));
    // A speed the table does not list has no default: nothing is borrowed from a neighbour.
    assert!(dg::get("link.ssd.desirable_min", &Conditions::none().design_speed(95.0)).is_none());
    // A key the pack does not hold has no default.
    assert!(dg::get("drainage.pipe.min_cover", &Conditions::none()).is_none());
    // With no basis chosen every field is an empty input.
    dg::set_basis(dg::NO_BASIS);
    assert!(dg::get("link.ssd.desirable_min", &Conditions::none().design_speed(100.0)).is_none());
    assert_eq!(dg::basis_id(), "none");
    dg::set_basis("dmrb");
    assert_eq!(dg::basis_id(), "dmrb");
}

#[test]
fn the_basis_is_kept_in_the_project_settings_and_read_back() {
    let mut doc = afrobim_kernel::Document::default();
    dg::store_basis(&mut doc, dg::NO_BASIS);
    let back = afrobim_kernel::Document::from_json(&doc.to_json()).unwrap();
    dg::set_basis("dmrb");
    dg::sync(&back, true);
    assert_eq!(dg::basis_id(), "none");
    dg::store_basis(&mut doc, "dmrb");
    dg::sync(&doc, true);
    assert_eq!(dg::basis_id(), "dmrb");
}

#[test]
fn road_type_choices_come_from_the_pack() {
    dg::set_basis("dmrb");
    let mut c = dg::RoadChoice { group: 3, road: "S2".into() };
    assert_eq!(c.prefix(), Some("xs.rural_all_purpose"));
    let lane = dg::number("xs.rural_all_purpose.lane_width", &Conditions::none().label("road_type/lane", "S2/L1"));
    assert_eq!(lane, Some(3.65));
    // The urban all-purpose cross-section has no verge width in the pack ("Varies" in the figure).
    c.group = 4;
    c.road = "SU2".into();
    assert!(dg::number(&c.key("verge_width").unwrap(), &c.cond()).is_none());
    assert!(dg::labels_of("xs.rural_all_purpose.carriageway_width").contains(&"WS2".to_string()));
}

pub(crate) fn demo_doc() -> afrobim_kernel::Document {
    let mut doc = afrobim_kernel::Document::default();
    let notes = crate::civil_demo::build(&mut doc);
    assert!(notes.is_empty(), "{notes:?}");
    doc
}

pub(crate) fn ctx_with_tins(doc: &afrobim_kernel::Document) -> crate::civil_build::Ctx {
    let base = crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins: Default::default() };
    let mut tins = std::collections::HashMap::new();
    for e in doc.elements().iter().filter(|e| e.category == crate::civil::SURFACE) {
        if let Some(t) = crate::civil_build::build(e, &base).tin {
            tins.insert(e.id.0, t);
        }
    }
    crate::civil_build::Ctx { elements: doc.elements().to_vec(), tins }
}

