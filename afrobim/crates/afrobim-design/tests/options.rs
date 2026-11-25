// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Option comparison on two options over a synthetic flat ground, with volumes checked by
//! independent methods. All numbers (template widths, slopes, the rate) are test fixtures.
//!
//! Ground: the plane z = 100. Template: a 5 m flat lane each side of the centreline, side slopes
//! 1 vertical in 2 horizontal in fill. Road level h above the ground gives
//!   top width 10, bottom width 10 + 4 h, fill area  A(h) = (10 + 10 + 4h) h / 2 = 10 h + 2 h^2.
//!
//! Option A: straight, 0 to 400 m, level rising linearly from 102 to 104, so h = 2 + s / 200.
//!   exact volume  = integral of A ds = 200 [5 h^2 + (2/3) h^3] from h = 2 to 4
//!                 = 200 [(80 + 42.6667) - (20 + 5.3333)] = 19466.667 m3
//!   Sections every 10 m. Average end area over-reads a quadratic by L h^2 f'' / 12, with
//!   f'' = d2A/ds2 = 4 (dh/ds)^2 = 1e-4: 400 x 100 x 1e-4 / 12 = 0.3333 m3, so the average end
//!   area result is 19467.000 m3; Simpson's rule (prismoidal) is exact for a quadratic and
//!   gives 19466.667.
//!   Land take: width 10 + 4h = 18 to 26, linear, so area = 400 x 22 = 8800 m2.
//! Option B: a curve of R = 300 at PI (200, 0) turning to (400, 100), level constant 101 so h = 1,
//!   A = 12 m2 (constant: average end area exact), width 14. Volume = 12 L, land take = 14 L,
//!   with L = 200 + sqrt(200^2 + 100^2) - 2T + R delta, T = R tan(delta/2), delta = atan(1/2).

use std::collections::BTreeMap;

use afrobim_corridor::{CorridorDefinition, Daylight, Link, Slope, Template, Tin};
use afrobim_cost::*;
use afrobim_design::*;

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

fn ground() -> Tin {
    Tin::from_grid(-100.0, -500.0, 50.0, 50.0, 21, 21, |_, _| 100.0).unwrap()
}

fn definition() -> CorridorDefinition {
    let t = Template::symmetric("T", vec![Link::new("lane", 5.0, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 2.0, fill_run_per_rise: 2.0 }, 0.0);
    CorridorDefinition::single("def", t, 10.0)
}

fn set() -> OptionSet {
    let a = AlignmentOption {
        name: "A".into(),
        design: AlignmentDesign::new(
            HorizontalDesign::new("A", 0.0, vec![Pi::point(0.0, 0.0), Pi::point(400.0, 0.0)]),
            Some(VerticalDesign::new("A", vec![VPvi::point(0.0, 102.0), VPvi::point(400.0, 104.0)])),
        ),
        corridor: Some(definition()),
        note: String::new(),
    };
    let b = AlignmentOption {
        name: "B".into(),
        design: AlignmentDesign::new(
            HorizontalDesign::new("B", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(200.0, 0.0, 300.0), Pi::point(400.0, 100.0)]),
            Some(VerticalDesign::new("B", vec![VPvi::point(0.0, 101.0), VPvi::point(430.0, 101.0)])),
        ),
        corridor: Some(definition()),
        note: String::new(),
    };
    OptionSet { name: "Options".into(), surface: None, estimate: None, options: vec![a, b], criteria: vec![] }
}

fn fixture_estimate() -> Estimate {
    // A rate of 7 UGX per m3 of fill: a fixture, not a price.
    let mut est = Estimate::new("Fixture", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("1.01", "1", "Fill", "m3")];
    est.bill.rules = vec![MappingRule::new("fill", "1.01", "earthworks.fill", vec![])];
    est.rates.set(Rate {
        item_code: "1.01".into(),
        description: "fill".into(),
        unit: "m3".into(),
        rate: Decimal::parse("7").unwrap(),
        currency: "UGX".into(),
        source_note: "test fixture".into(),
        date: None,
    });
    est
}

#[test]
fn volumes_land_take_and_geometry_against_independent_methods() {
    let tin = ground();
    let est = fixture_estimate();
    let mut feats = vec![
        CrossingFeature { name: "river 1".into(), kind: "river".into(), polyline: vec![(150.0, -50.0), (150.0, 50.0)] },
        CrossingFeature { name: "track".into(), kind: "road".into(), polyline: vec![(300.0, 40.0), (300.0, 100.0)] },
    ];
    let cmp = compare_options(&set(), &CompareInputs { ground: Some(&tin), crossings: feats.clone(), estimate: Some(&est), extra_quantities: BTreeMap::new() });
    let (a, b) = (&cmp.options[0], &cmp.options[1]);
    assert!(a.error.is_none() && b.error.is_none());

    // Option A.
    close(a.value("length_m").unwrap(), 400.0, 1e-9, "A length");
    assert_eq!(a.value("curves"), Some(0.0));
    assert_eq!(a.value("min_radius_m"), None);
    close(a.value("max_grade_pct").unwrap(), 0.5, 1e-9, "A max grade (2 m in 400 m)");
    close(a.value("rise_m").unwrap(), 2.0, 1e-9, "A rise");
    close(a.value("fall_m").unwrap(), 0.0, 1e-9, "A fall");
    close(a.value("cut_m3").unwrap(), 0.0, 1e-6, "A cut");
    // Independent: area A(s) = 10 h + 2 h^2 with h = 2 + s/200, sections every 10 m.
    let area = |s: f64| {
        let h = 2.0 + s / 200.0;
        10.0 * h + 2.0 * h * h
    };
    let mut avg_end_area = 0.0;
    for i in 0..40 {
        avg_end_area += 10.0 * (area(10.0 * i as f64) + area(10.0 * (i + 1) as f64)) / 2.0;
    }
    let mut simpson = area(0.0) + area(400.0);
    for i in 1..40 {
        simpson += if i % 2 == 1 { 4.0 } else { 2.0 } * area(10.0 * i as f64);
    }
    simpson *= 10.0 / 3.0;
    close(simpson, 19_466.666_666_666_67, 1e-6, "prismoidal (Simpson) equals the analytic integral");
    close(avg_end_area - simpson, 1.0 / 3.0, 1e-6, "average end area over-reads by 1/3 m3");
    close(a.value("fill_m3").unwrap(), avg_end_area, 1e-4, "A fill, average end area as computed by the corridor crate");
    close(a.value("fill_m3").unwrap(), 19467.0, 1e-4, "A fill, hand value");
    close(a.value("fill_m3").unwrap() - simpson, 1.0 / 3.0, 1e-4, "difference from the prismoidal value");
    close(a.value("balance_m3").unwrap(), -19467.0, 1e-4, "A balance");
    close(a.value("land_take_m2").unwrap(), 8800.0, 1e-6, "A land take");
    close(a.value("cost").unwrap(), 7.0 * 19467.0, 1.0, "A cost = 7 x fill");
    assert_eq!(a.value("crossings"), Some(1.0));
    assert!(a.notes.iter().any(|n| n.contains("river 1") && n.contains("0+150.0")), "{:?}", a.notes);

    // Option B.
    let delta = (0.5f64).atan();
    let t = 300.0 * (delta / 2.0).tan();
    let length = 200.0 + (200.0f64.powi(2) + 100.0f64.powi(2)).sqrt() - 2.0 * t + 300.0 * delta;
    close(b.value("length_m").unwrap(), length, 1e-6, "B length");
    assert_eq!(b.value("curves"), Some(1.0));
    close(b.value("min_radius_m").unwrap(), 300.0, 1e-9, "B min radius");
    close(b.value("max_grade_pct").unwrap(), 0.0, 1e-12, "B grade");
    close(b.value("fill_m3").unwrap(), 12.0 * length, 1e-3, "B fill = 12 L");
    close(b.value("land_take_m2").unwrap(), 14.0 * length, 1e-3, "B land take = 14 L");
    close(b.value("curve_share_pct").unwrap(), 100.0 * 300.0 * delta / length, 1e-6, "B curve share");
    close(b.value("deflection_deg_per_km").unwrap(), delta.to_degrees() / (length / 1000.0), 1e-6, "B turning per km");
    assert_eq!(b.value("crossings"), Some(2.0));
    // Every figure says how it was computed.
    assert!(a.figures.iter().all(|f| f.value.is_none() || !f.how.is_empty()));

    // Without ground, estimate or crossing data the figures are "not computed", with reasons.
    feats.clear();
    let bare = compare_options(&set(), &CompareInputs::default());
    let f = bare.options[0].figure("fill_m3").unwrap();
    assert!(f.value.is_none() && f.not_computed.as_deref().unwrap().contains("no ground surface"));
    assert!(bare.options[0].figure("cost").unwrap().not_computed.as_deref().unwrap().contains("no estimate"));
    assert!(bare.options[0].figure("crossings").unwrap().not_computed.as_deref().unwrap().contains("no crossing"));
    assert!(bare.options[0].value("length_m").is_some());
}

#[test]
fn weighted_scores_use_the_users_weights_and_the_csv_shows_both() {
    let tin = ground();
    let cmp = compare_options(&set(), &CompareInputs { ground: Some(&tin), ..CompareInputs::default() });
    // Fill: A 19467, B 12 L = 5053.3 -> lower is better: A scaled 0, B scaled 1.
    // Length: A 400, B 421.1 -> A scaled 1, B scaled 0.
    // Weights 2 and 1: A = (2 x 0 + 1 x 1) / 3 = 0.3333, B = (2 x 1 + 1 x 0) / 3 = 0.6667.
    let crit = vec![
        Criterion { key: "fill_m3".into(), weight: 2.0, direction: Direction::LowerIsBetter },
        Criterion { key: "length_m".into(), weight: 1.0, direction: Direction::LowerIsBetter },
    ];
    let t = score(&cmp, &crit);
    close(t.options[0].score.unwrap(), 1.0 / 3.0, 1e-9, "A score");
    close(t.options[1].score.unwrap(), 2.0 / 3.0, 1e-9, "B score");
    // A weight of zero leaves a criterion out; a missing figure withholds the score, with the reason.
    let t = score(&cmp, &[Criterion { key: "cost".into(), weight: 1.0, direction: Direction::LowerIsBetter }]);
    assert!(t.options[0].score.is_none() && t.options[0].reason.as_deref().unwrap().contains("cost"));
    let t = score(&cmp, &[Criterion { key: "length_m".into(), weight: 0.0, direction: Direction::LowerIsBetter }]);
    assert!(t.options[0].score.is_none());

    let csv = options_comparison_csv(&cmp);
    assert!(csv.starts_with("Figure,Unit,\"A\",\"B\",How computed\n"));
    assert!(csv.contains("\"Length\",\"m\",400.000,"));
    assert!(csv.contains("\"Cost\"") && csv.contains("n/a"));
    assert!(csv.contains("# not computed,\"A\",\"Cost\""));
    let sc = scoring_csv(&score(&cmp, &crit));
    assert!(sc.starts_with("# Weighted scores use the USER'S OWN weights"));
    assert!(sc.contains("0.333") && sc.contains("0.667"));
}

#[test]
fn an_option_that_cannot_be_built_is_listed_not_hidden() {
    let mut s = set();
    s.options[1].design.horizontal.pis[1].radius = 5000.0;
    let cmp = compare_options(&s, &CompareInputs::default());
    assert!(cmp.options[1].error.as_deref().unwrap().contains("tangent"));
    assert!(cmp.options[1].figure("length_m").unwrap().not_computed.as_deref().unwrap().contains("cannot be built"));
    assert!(cmp.options[0].value("length_m").is_some());
}
