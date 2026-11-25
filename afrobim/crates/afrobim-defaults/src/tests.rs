// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use super::*;
use crate::review::{review_markdown, review_plain};

fn pack() -> Pack {
    Pack::dmrb()
}

fn series(p: &Pack, key: &str) -> Vec<(f64, f64)> {
    let e = p.entry(key).unwrap_or_else(|| panic!("missing {key}"));
    e.table
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|r| match r.at {
            At::Num(a) => (a, r.value),
            _ => panic!("label table {key}"),
        })
        .collect()
}

#[test]
fn embedded_pack_loads() {
    let p = pack();
    assert_eq!(p.id(), "dmrb");
    assert!(p.all().len() > 150, "pack unexpectedly small: {}", p.all().len());
}

#[test]
fn every_entry_has_all_citation_fields() {
    let p = pack();
    for e in p.all() {
        let d = p.documents().get(&e.document).unwrap_or_else(|| panic!("{}: no document record {}", e.key, e.document));
        for (name, v) in [
            ("title", &e.title),
            ("clause", &e.clause),
            ("page", &e.page),
            ("condition", &e.condition),
            ("verb", &e.verb),
            ("group", &e.group),
            ("doc.reference", &d.reference),
            ("doc.title", &d.title),
            ("doc.version", &d.version),
            ("doc.published", &d.published),
            ("doc.url", &d.url),
            ("doc.annex", &d.annex),
            ("doc.copyright", &d.copyright),
        ] {
            assert!(!v.trim().is_empty(), "{}: empty {}", e.key, name);
        }
        assert!(d.url.starts_with("https://www.standardsforhighways.co.uk/"), "{}: url", e.key);
        assert!(e.pdf_page >= 1, "{}: pdf page", e.key);
        // the printed page is one less than the PDF page for every fetched document
        assert_eq!(e.page.parse::<u32>().unwrap() + 1, e.pdf_page, "{}: printed page vs pdf page", e.key);
        let c = p.citation(&e.key).unwrap();
        assert_eq!(c.document_reference, d.reference);
    }
    let m = p.meta();
    for s in [&m.read_on, &m.read_by, &m.review_status, &m.uganda_caveat, &m.licence.printed_notice, &m.licence.attribution] {
        assert!(!s.trim().is_empty());
    }
}

#[test]
fn units_come_from_the_fixed_list() {
    let allowed = ["m", "mm", "%", "km/h", "m/s", "m/s3", "deg", "years", "m/%", "1:n", "m2", "ha", "ratio", "s/m^(1/3)", "none", "label"];
    let v: serde_json::Value = serde_json::from_str(DMRB_JSON).unwrap();
    for e in v["entries"].as_array().unwrap() {
        let u = e["unit"].as_str().unwrap();
        assert!(allowed.contains(&u), "{}: unit {u}", e["key"]);
        if let Some(t) = e.get("table") {
            let du = t["dimension"]["unit"].as_str().unwrap();
            assert!(allowed.contains(&du), "{}: dimension unit {du}", e["key"]);
        }
    }
    // an unknown unit is refused by the type, not silently accepted
    let bad = DMRB_JSON.replacen("\"unit\": \"m/%\"", "\"unit\": \"furlongs\"", 1);
    assert!(Pack::from_json(&bad).is_err());
    // the label unit is for text dimensions only
    for e in pack().all() {
        assert_ne!(e.unit, Unit::Label, "{}: label unit on a value", e.key);
        if let Some(t) = &e.table {
            let labels = t.rows.iter().all(|r| matches!(r.at, At::Label(_)));
            let nums = t.rows.iter().all(|r| matches!(r.at, At::Num(_)));
            assert!(labels || nums, "{}: mixed dimension", e.key);
            assert_eq!(labels, t.dimension.unit == Unit::Label, "{}: dimension unit", e.key);
        }
    }
}

#[test]
fn keys_are_unique_and_well_formed() {
    let p = pack();
    let mut seen = BTreeSet::new();
    for e in p.all() {
        assert!(seen.insert(e.key.clone()), "duplicate key {}", e.key);
        assert!(e.key.contains('.') && e.key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_'), "bad key {}", e.key);
    }
}

#[test]
fn entries_match_their_kind() {
    for e in pack().all() {
        match e.kind {
            Kind::Scalar => assert!(e.value.is_some() && e.table.is_none() && e.text.is_none(), "{}", e.key),
            Kind::Table => {
                let t = e.table.as_ref().unwrap_or_else(|| panic!("{}", e.key));
                assert!(e.value.is_none() && !t.rows.is_empty(), "{}", e.key);
            }
            Kind::Formula | Kind::Text => assert!(e.text.as_deref().map_or(false, |t| !t.is_empty()) && e.value.is_none(), "{}", e.key),
        }
        if e.kind != Kind::Table && e.kind != Kind::Scalar {
            assert_eq!(e.unit, Unit::None, "{}", e.key);
        }
    }
}

#[test]
fn tables_are_sorted_and_monotonic_where_physics_says_so() {
    let p = pack();
    let mut checked = 0;
    for e in p.all() {
        let Some(t) = &e.table else { continue };
        if t.dimension.unit == Unit::Label {
            assert_eq!(t.monotonic, Monotonic::None, "{}: monotonic on a label table", e.key);
            continue;
        }
        let pts = series(&p, &e.key);
        for w in pts.windows(2) {
            assert!(w[0].0 < w[1].0, "{}: dimension not strictly ascending", e.key);
            match t.monotonic {
                Monotonic::IncreasingWithDimension => assert!(w[1].1 > w[0].1, "{}: not increasing {:?}", e.key, w),
                Monotonic::NondecreasingWithDimension => assert!(w[1].1 >= w[0].1, "{}: decreasing {:?}", e.key, w),
                Monotonic::DecreasingWithDimension => assert!(w[1].1 < w[0].1, "{}: not decreasing {:?}", e.key, w),
                Monotonic::None => {}
            }
        }
        if t.monotonic != Monotonic::None {
            checked += 1;
        }
    }
    assert!(checked >= 35, "only {checked} monotonic tables checked");
    // the physics that must hold: sight distance, radius and K rise with design speed
    for key in ["link.ssd.desirable_min", "link.ssd.one_step_below", "link.horizontal.desirable_min_radius_superelevation_5", "link.vertical.crest_k_desirable_min", "link.overtaking.fosd"] {
        assert_eq!(p.entry(key).unwrap().table.as_ref().unwrap().monotonic, Monotonic::IncreasingWithDimension, "{key}");
    }
}

#[test]
fn table_2_10_is_internally_consistent() {
    let p = pack();
    let speeds = [50.0, 60.0, 70.0, 85.0, 100.0, 120.0];
    let rows = [
        ("link.horizontal.min_radius_adverse_camber_no_transition", 5.0),
        ("link.horizontal.min_radius_superelevation_2_5", 7.07),
        ("link.horizontal.min_radius_superelevation_3_5", 10.0),
        ("link.horizontal.desirable_min_radius_superelevation_5", 14.14),
        ("link.horizontal.one_step_below_radius_superelevation_7", 20.0),
        ("link.horizontal.two_steps_below_radius_superelevation_7", 28.28),
    ];
    // each row is a constant V^2/R (the document's own check column), within rounding of R
    for (key, v2r) in rows {
        for (v, r) in series(&p, key) {
            assert!((v * v / r - v2r).abs() / v2r < 0.05, "{key} at {v}: V2/R = {}", v * v / r);
        }
    }
    // one step below at speed v equals the desirable value at the next lower speed step
    for (hi, lo) in [("link.ssd.one_step_below", "link.ssd.desirable_min"), ("link.vertical.crest_k_one_step_below", "link.vertical.crest_k_desirable_min")] {
        let a = series(&p, hi);
        let b = series(&p, lo);
        for i in 1..speeds.len() {
            assert_eq!(a[i].1, b[i - 1].1, "{hi} vs {lo} at step {i}");
        }
    }
    // each radius row equals the previous row one speed step down
    let radii: Vec<Vec<(f64, f64)>> = rows.iter().map(|(k, _)| series(&p, k)).collect();
    for r in 1..radii.len() {
        for i in 1..speeds.len() {
            assert_eq!(radii[r][i].1, radii[r - 1][i - 1].1, "row {r} step {i}");
        }
    }
}

#[test]
fn get_returns_value_with_citation() {
    let d = Defaults::dmrb();
    let v = d.get("link.ssd.desirable_min", &Conditions::none().design_speed(100.0)).unwrap();
    assert_eq!(v.number(), Some(215.0));
    assert_eq!(v.unit, Unit::Metre);
    assert_eq!(v.status, Status::DesirableMinimum);
    assert_eq!(v.citation.document_reference, "CD 109");
    assert_eq!(v.citation.version, "1.1.0");
    assert_eq!(v.citation.published, "30-Oct-2026");
    assert!(v.citation.clause.contains("Table 2.10"));
    assert_eq!(v.citation.page, "17");
    assert!(!v.interpolated);
    let line = d.cite("link.ssd.desirable_min").unwrap();
    assert!(line.contains("CD 109") && line.contains("1.1.0") && line.contains("Table 2.10") && line.contains("p17"), "{line}");
    // a design speed the table does not list has no default
    assert!(d.get("link.ssd.desirable_min", &Conditions::none().design_speed(90.0)).is_none());
    // a table asked without its condition has no default
    assert!(d.get("link.ssd.desirable_min", &Conditions::none()).is_none());
    // scalar, formula and label lookups
    assert_eq!(d.get("link.superelevation.max_rural", &Conditions::none()).unwrap().number(), Some(7.0));
    assert!(matches!(d.get("link.superelevation.formula", &Conditions::none()).unwrap().value, Resolved::Formula(_)));
    let lw = d.get("xs.rural_all_purpose.lane_width", &Conditions::none().label("road_type/lane", "D3AP/L2")).unwrap();
    assert_eq!(lw.number(), Some(3.70));
    assert!(d.get("xs.rural_all_purpose.lane_width", &Conditions::none().label("road_type/lane", "D9/L1")).is_none());
    // FOSD has no value at 120 km/h (the table shows a dash)
    assert!(d.get("link.overtaking.fosd", &Conditions::none().design_speed(120.0)).is_none());
}

#[test]
fn interpolation_only_where_the_document_allows_it() {
    let d = Defaults::dmrb();
    // CD 123 clause 5.11: widths between listed radii shall be interpolated
    let mid = d.get("junction.lane_width.curve_inside_lane", &Conditions::none().num("inside_curve_radius_m", 12.5)).unwrap();
    assert!(mid.interpolated);
    assert!((mid.number().unwrap() - 7.75).abs() < 1e-9);
    // outside the listed range there is no extrapolation
    assert!(d.get("junction.lane_width.curve_inside_lane", &Conditions::none().num("inside_curve_radius_m", 5.0)).is_none());
    assert!(d.get("junction.lane_width.curve_inside_lane", &Conditions::none().num("inside_curve_radius_m", 120.0)).is_none());
    // exact-only table: a value between rows is not invented
    assert!(d.get("xs.headroom.sag_compensation", &Conditions::none().num("sag_radius_m", 1100.0)).is_none());
    assert_eq!(d.get("xs.headroom.sag_compensation", &Conditions::none().num("sag_radius_m", 1200.0)).unwrap().number(), Some(70.0));
}

#[test]
fn missing_lists_wanted_keys_the_pack_does_not_hold() {
    let d = Defaults::dmrb();
    let m = d.missing();
    let keys: Vec<&str> = m.iter().map(|x| x.key.as_str()).collect();
    for k in ["survey.setting_out.horizontal_tolerance", "drainage.pipe.min_cover", "pavement.cd226.layer_thickness", "earthworks.side_slope.angle"] {
        assert!(keys.contains(&k), "{k} should be missing");
    }
    assert!(!keys.contains(&"link.ssd.desirable_min"));
    assert!(m.iter().all(|x| !x.reason.is_empty()));
    // an application asking for a key that is not in the pack is recorded
    assert!(d.get("link.made.up.key", &Conditions::none()).is_none());
    assert!(d.cite("another.made.up").is_none());
    let keys: Vec<String> = d.missing().into_iter().map(|x| x.key).collect();
    assert!(keys.contains(&"link.made.up.key".to_string()) && keys.contains(&"another.made.up".to_string()));
    // every wanted key that the pack holds really is held, and every held one is well formed
    let p = pack();
    let held = WANTED.iter().filter(|(k, _)| p.entry(k).is_some()).count();
    assert!(held >= 10);
}

#[test]
fn a_second_pack_can_be_added_and_chosen_as_basis() {
    let mut d = Defaults::dmrb();
    assert_eq!(d.basis_id(), Some("dmrb"));
    // a minimal pack on another basis, built from the DMRB file with a different id and one value changed
    let other = DMRB_JSON.replacen("\"id\": \"dmrb\"", "\"id\": \"test_pack\"", 1).replacen("\"name\": \"UK Design Manual for Roads and Bridges (DMRB)\"", "\"name\": \"Test pack\"", 1);
    let other = other.replace("\"value\": 7,", "\"value\": 6,");
    d.add_set(Box::new(Pack::from_json(&other).unwrap()));
    assert_eq!(d.bases().len(), 2);
    let k = "link.superelevation.max_rural";
    assert_eq!(d.get(k, &Conditions::none()).unwrap().number(), Some(7.0));
    d.set_basis("test_pack").unwrap();
    assert_eq!(d.get(k, &Conditions::none()).unwrap().number(), Some(6.0));
    assert!(d.cite(k).unwrap().starts_with("TEST_PACK"));
    assert!(d.set_basis("nope").is_err());
    // a project with no basis chosen has no defaults
    let e = Defaults::empty();
    assert!(e.get(k, &Conditions::none()).is_none() && e.all().is_empty() && !e.missing().is_empty());
}

#[test]
fn prints_the_whole_pack_as_a_review_table() {
    let p = pack();
    let t = review_plain(&p);
    println!("{}", t);
    assert_eq!(t.lines().count(), p.all().len());
}

#[test]
fn review_markdown_is_current_and_says_it_is_unreviewed() {
    let md = review_markdown(&pack());
    let on_disk = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/REVIEW.md")).expect("REVIEW.md missing: run `cargo run -p afrobim-defaults --example write_review`");
    assert_eq!(md, on_disk, "REVIEW.md is out of date: run `cargo run -p afrobim-defaults --example write_review`");
    let head: String = md.lines().take(5).collect::<Vec<_>>().join(" ");
    assert!(head.contains("automated reader") && head.contains("must be reviewed"), "{head}");
}

#[test]
fn no_em_dashes_in_the_data_or_the_documents() {
    for f in ["data/dmrb.json", "README.md", "REVIEW.md"] {
        let s = std::fs::read_to_string(format!("{}/{}", env!("CARGO_MANIFEST_DIR"), f)).unwrap_or_default();
        assert!(!s.contains('\u{2014}'), "{f} contains an em dash");
    }
}

fn ug() -> Pack {
    Pack::ug_mowt()
}

#[test]
fn ug_pack_loads_and_every_entry_is_sourced() {
    let p = ug();
    assert_eq!(p.id(), UG_MOWT_ID);
    for e in p.all() {
        assert!(!e.source.trim().is_empty(), "{}: empty source", e.key);
        assert!(!e.clause.trim().is_empty() && !e.page.trim().is_empty() && !e.condition.trim().is_empty(), "{}", e.key);
        assert!(p.documents().contains_key(&e.document), "{}", e.key);
        assert!(p.citation(&e.key).is_some());
        assert!(!e.source.contains('\u{2014}') && !e.note.contains('\u{2014}'));
    }
}

#[test]
fn ug_design_vehicles_dv1_to_dv5_match_table_5_1() {
    let p = ug();
    let v = design_vehicles(&p);
    assert_eq!(v.len(), 5);
    assert_eq!(v[0].symbol, "DV-1");
    assert_eq!(v[4].symbol, "DV-5");
    assert_eq!((v[4].overall_length_m, v[4].min_turning_radius_m, v[4].min_inside_radius_m), (21.0, 13.7, 2.9));
    assert_eq!((v[0].overall_length_m, v[0].min_turning_radius_m, v[0].min_inside_radius_m), (5.8, 7.3, 4.2));
    assert_eq!(v[3].wheelbase_second_m, Some(9.1));
    assert_eq!(v[4].wheelbase_second_m, Some(12.8));
    assert_eq!(v[0].wheelbase_second_m, None);
    // DV-5 is the largest; overall length is the sum of overhangs and wheelbases
    assert!(v.windows(2).all(|w| w[1].overall_length_m > w[0].overall_length_m));
    for d in &v {
        let sum = d.front_overhang_m + d.wheelbase_first_m + d.wheelbase_second_m.unwrap_or(0.0) + d.rear_overhang_m;
        assert!((sum - d.overall_length_m).abs() < 1e-9, "{}: {sum}", d.symbol);
    }
}

#[test]
fn dmrb_holds_no_design_vehicles_and_standard_defaults_to_uganda() {
    assert!(design_vehicles(&Pack::dmrb()).is_empty());
    let mut d = Defaults::standard();
    assert_eq!(d.basis_id(), Some(UG_MOWT_ID));
    assert_eq!(d.bases().len(), 2);
    let ssd = d.get("link.ssd.min", &Conditions::none().design_speed(80.0)).unwrap();
    assert_eq!(ssd.number(), Some(115.0));
    d.set_basis("dmrb").unwrap();
    assert_eq!(d.basis_id(), Some("dmrb"));
}

#[test]
fn ug_class_tables_are_read_by_terrain() {
    let d = Defaults::standard();
    let c = Conditions::none().label("terrain", "Rolling");
    assert_eq!(d.get("link.design_speed.ii", &c).unwrap().number(), Some(70.0));
    assert_eq!(d.get("link.gradient_desirable_max.c", &c).unwrap().number(), Some(7.0));
}

#[test]
fn no_em_dashes_in_the_ug_pack() {
    let s = std::fs::read_to_string(format!("{}/data/ug-mowt-2010.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert!(!s.contains('\u{2014}'));
}
