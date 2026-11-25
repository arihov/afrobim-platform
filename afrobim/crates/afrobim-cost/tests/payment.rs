// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
mod common;

use std::collections::BTreeMap;

use afrobim_cost::*;
use common::*;

/// A: 100 m at 1000 (100,000). B: 50 m2 at 2000 (100,000). Whole shillings.
fn two_items() -> (Estimate, QuantitySet) {
    let mut est = Estimate::new("pay", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("A", "1", "Item A", "m"), Item::measured("B", "1", "Item B", "m2")];
    est.bill.rules = vec![MappingRule::new("a", "A", "a", vec![]), MappingRule::new("b", "B", "b", vec![])];
    for (code, unit, rate) in [("A", "m", "1000"), ("B", "m2", "2000")] {
        est.rates.set(Rate { item_code: code.into(), description: "".into(), unit: unit.into(), rate: d(rate), currency: "UGX".into(), source_note: "".into(), date: None });
    }
    let mut set = QuantitySet::new();
    set.push_decimal(Some(e(1)), "A1", "a", "", d("100"), "m", &[]);
    set.push_decimal(Some(e(2)), "B1", "b", "", d("50"), "m2", &[]);
    (est, set)
}

fn terms() -> PaymentTerms {
    PaymentTerms {
        rules: vec![
            PaymentRule { name: "Retention".into(), kind: PaymentRuleKind::Retention { percent: d("10"), cap: Some(d("15000")) } },
            PaymentRule {
                name: "Advance recovery".into(),
                kind: PaymentRuleKind::AdvanceRecovery { advance: d("30000"), percent: d("25"), start_threshold: d("20000") },
            },
            PaymentRule { name: "Tax".into(), kind: PaymentRuleKind::PercentOfGross { percent: d("5"), basis: GrossBasis::ThisPeriod, effect: Effect::Add } },
        ],
    }
}

fn qty(pairs: &[(&str, &str)]) -> BTreeMap<String, Decimal> {
    pairs.iter().map(|(c, q)| (c.to_string(), d(q))).collect()
}

#[test]
fn interim_payment_arithmetic_with_retention_and_advance_recovery() {
    let (est, set) = two_items();
    let r = est.compute(&set).unwrap();
    let rounding = Rounding::half_up(0);

    // Period 1: A 40 m = 40,000; B 10 m2 = 20,000; gross 60,000.
    // Retention 10% = 6,000. Advance recovery 25% of (60,000 - 20,000) = 10,000.
    // Tax added: 5% of this period's gross = 3,000. Net 60,000 - 6,000 - 10,000 + 3,000 = 47,000.
    let c1 = certify(&r, &terms(), "IPC 1", &qty(&[("A", "40"), ("B", "10")]), None, rounding).unwrap();
    assert_eq!((c1.gross_to_date, c1.gross_previous, c1.gross_this_period), (d("60000"), d("0"), d("60000")));
    let rules: Vec<_> = c1.rules.iter().map(|o| (o.name.as_str(), o.to_date, o.previous, o.this_period)).collect();
    assert_eq!(
        rules,
        vec![("Retention", d("6000"), d("0"), d("6000")), ("Advance recovery", d("10000"), d("0"), d("10000")), ("Tax", d("3000"), d("0"), d("3000"))]
    );
    assert_eq!((c1.net_this_period, c1.net_to_date), (d("47000"), d("47000")));
    assert!(c1.warnings.is_empty());

    // Period 2: A 90 m = 90,000; B 40 m2 = 80,000; gross to date 170,000; this period 110,000.
    // Retention 10% of 170,000 = 17,000 capped at 15,000: 9,000 this period.
    // Advance: 25% of (170,000 - 20,000) = 37,500 limited to the advance, 30,000: 20,000 this period.
    // Tax: 3,000 + 5% of 110,000 = 8,500 to date, 5,500 this period.
    // Net 110,000 - 9,000 - 20,000 + 5,500 = 86,500; to date 170,000 - 15,000 - 30,000 + 8,500 = 133,500.
    let c2 = certify(&r, &terms(), "IPC 2", &qty(&[("A", "90"), ("B", "40")]), Some(&c1.state), rounding).unwrap();
    assert_eq!((c2.gross_to_date, c2.gross_previous, c2.gross_this_period), (d("170000"), d("60000"), d("110000")));
    let l = |code: &str| c2.lines.iter().find(|l| l.code == code).unwrap();
    assert_eq!((l("A").quantity_previous, l("A").quantity_this_period, l("A").amount_this_period), (d("40"), d("50"), d("50000")));
    assert_eq!((l("B").quantity_previous, l("B").quantity_this_period, l("B").amount_this_period), (d("10"), d("30"), d("60000")));
    let rules: Vec<_> = c2.rules.iter().map(|o| (o.to_date, o.previous, o.this_period)).collect();
    assert_eq!(rules, vec![(d("15000"), d("6000"), d("9000")), (d("30000"), d("10000"), d("20000")), (d("8500"), d("3000"), d("5500"))]);
    assert_eq!((c2.net_this_period, c2.net_to_date), (d("86500"), d("133500")));
    // The periods add up to the to-date figure.
    assert_eq!(c1.net_this_period + c2.net_this_period, c2.net_to_date);

    // Period 3: nothing new entered; items stay at their previous quantities and nothing is paid.
    let c3 = certify(&r, &terms(), "IPC 3", &BTreeMap::new(), Some(&c2.state), rounding).unwrap();
    assert_eq!((c3.gross_this_period, c3.net_this_period, c3.net_to_date), (d("0"), d("0"), d("133500")));
    assert_eq!(c3.lines.iter().find(|l| l.code == "A").unwrap().quantity_to_date, d("90"));
}

#[test]
fn warnings_for_quantities_over_the_bill_or_below_the_previous() {
    let (est, set) = two_items();
    let r = est.compute(&set).unwrap();
    let rounding = Rounding::half_up(0);
    let none = PaymentTerms::default();
    let c1 = certify(&r, &none, "1", &qty(&[("A", "120")]), None, rounding).unwrap();
    assert_eq!(c1.warnings.len(), 1);
    assert!(c1.warnings[0].contains("exceeds the bill quantity"));
    assert_eq!(c1.net_this_period, d("120000"));
    let c2 = certify(&r, &none, "2", &qty(&[("A", "100")]), Some(&c1.state), rounding).unwrap();
    assert!(c2.warnings[0].contains("less than the previous"));
    assert_eq!(c2.gross_this_period, d("-20000"));
}

#[test]
fn a_lump_deduction_and_percent_of_gross_to_date() {
    let (est, set) = two_items();
    let r = est.compute(&set).unwrap();
    let t = PaymentTerms {
        rules: vec![
            PaymentRule { name: "Other deduction".into(), kind: PaymentRuleKind::Lump { amount: d("1500"), effect: Effect::Deduct } },
            PaymentRule { name: "Levy".into(), kind: PaymentRuleKind::PercentOfGross { percent: d("2.5"), basis: GrossBasis::ToDate, effect: Effect::Deduct } },
        ],
    };
    let c1 = certify(&r, &t, "1", &qty(&[("A", "40")]), None, Rounding::half_up(0)).unwrap();
    // 2.5% of 40,000 = 1,000; lump 1,500; net 40,000 - 2,500.
    assert_eq!(c1.net_this_period, d("37500"));
    let c2 = certify(&r, &t, "2", &qty(&[("A", "80")]), Some(&c1.state), Rounding::half_up(0)).unwrap();
    // To date levy 2.5% of 80,000 = 2,000 (1,000 this period); the lump is paid again (a period rule): 3,000 to date.
    assert_eq!(c2.rules[1].this_period, d("1000"));
    assert_eq!(c2.rules[0].to_date, d("3000"));
    assert_eq!(c2.net_this_period, d("40000") - d("1500") - d("1000"));
}

#[test]
fn payment_input_is_checked() {
    let (est, set) = two_items();
    let r = est.compute(&set).unwrap();
    let rounding = Rounding::half_up(0);
    assert!(certify(&r, &PaymentTerms::default(), "x", &qty(&[("ZZ", "1")]), None, rounding).is_err());
    let dup = PaymentTerms {
        rules: vec![
            PaymentRule { name: "R".into(), kind: PaymentRuleKind::Lump { amount: d("1"), effect: Effect::Add } },
            PaymentRule { name: "R".into(), kind: PaymentRuleKind::Lump { amount: d("1"), effect: Effect::Add } },
        ],
    };
    assert!(certify(&r, &dup, "x", &BTreeMap::new(), None, rounding).is_err());
    // An unpriced item cannot be certified.
    let mut est2 = est.clone();
    est2.rates.rates.retain(|x| x.item_code != "B");
    let r2 = est2.compute(&set).unwrap();
    assert!(certify(&r2, &PaymentTerms::default(), "x", &qty(&[("B", "1")]), None, rounding).is_err());
}

#[test]
fn quantities_to_date_from_percent_complete_of_elements() {
    // Item X (m) takes 10 m from element 1 and 30 m from element 2, and an adjustment of +5 m.
    let mut est = Estimate::new("progress", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("X", "1", "Item X", "m")];
    est.bill.rules = vec![MappingRule::new("x", "X", "x", vec![])];
    est.bill.adjustments = vec![Adjustment { item: "X".into(), delta: d("5"), reason: "site instruction".into() }];
    est.rates.set(Rate { item_code: "X".into(), description: "".into(), unit: "m".into(), rate: d("100"), currency: "UGX".into(), source_note: "".into(), date: None });
    let mut set = QuantitySet::new();
    set.push_decimal(Some(e(1)), "E1", "x", "", d("10"), "m", &[]);
    set.push_decimal(Some(e(2)), "E2", "x", "", d("30"), "m", &[]);
    let r = est.compute(&set).unwrap();
    let elements: BTreeMap<_, _> = [(e(1), d("100")), (e(2), d("50"))].into_iter().collect();
    let items: BTreeMap<_, _> = [("X".to_string(), d("40"))].into_iter().collect();
    // 10 x 100% + 30 x 50% + 5 x 40% = 10 + 15 + 2 = 27 m.
    let q = quantities_from_progress(&r, &elements, &items, &est.bill.rounding).unwrap();
    assert_eq!(q["X"], d("27"));
    // An element not in the map is 0% complete.
    let only_first: BTreeMap<_, _> = [(e(1), d("25"))].into_iter().collect();
    assert_eq!(quantities_from_progress(&r, &only_first, &BTreeMap::new(), &est.bill.rounding).unwrap()["X"], d("2.5"));
    // They feed the certificate: 27 m x 100 = 2,700.
    let c = certify(&r, &PaymentTerms::default(), "1", &q, None, Rounding::half_up(0)).unwrap();
    assert_eq!(c.gross_this_period, d("2700"));
    // Percentages outside 0..100 are refused.
    let bad: BTreeMap<_, _> = [(e(1), d("101"))].into_iter().collect();
    assert!(quantities_from_progress(&r, &bad, &BTreeMap::new(), &est.bill.rounding).is_err());
}

#[test]
fn the_certificate_csv() {
    let (est, set) = two_items();
    let r = est.compute(&set).unwrap();
    let c = certify(&r, &terms(), "IPC 1", &qty(&[("A", "40"), ("B", "10")]), None, Rounding::half_up(0)).unwrap();
    let text = certificate_csv(&c);
    assert!(text.contains("item,A,Item A,m,1000,40,0,40,40000,0,40000"));
    assert!(text.contains("gross,,Gross value of work,,,,,,60000,0,60000"));
    assert!(text.contains("deduct,,Advance recovery,,,,,,10000,0,10000"));
    assert!(text.contains("add,,Tax,,,,,,3000,0,3000"));
    assert!(text.trim_end().ends_with("net,,Net amount,,,,,,47000,0,47000"));
}
