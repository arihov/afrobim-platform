// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A small hand-worked project shared by the tests. The numbers are fixtures chosen to be easy to
//! check by hand; they are not rates, percentages or prices of any real contract.

#![allow(dead_code)]

use afrobim_cost::*;
use afrobim_kernel::ElementId;

pub fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}

pub fn date(s: &str) -> Date {
    Date::parse(s).unwrap()
}

pub fn e(n: u64) -> ElementId {
    ElementId(n)
}

fn attrs<'a>(pairs: &[(&'a str, &str)]) -> Vec<(&'a str, String)> {
    pairs.iter().map(|(k, v)| (*k, v.to_string())).collect()
}

/// L0..L4: three marking areas, a culvert, and a sign face no rule covers.
pub fn lines() -> QuantitySet {
    let mut s = QuantitySet::new();
    s.push_decimal(Some(e(1)), "MK-1", "marking.painted_area", "Marking MK-1", d("2.0"), "m2", &attrs(&[("width_mm", "100")]));
    s.push_decimal(Some(e(2)), "MK-2", "marking.painted_area", "Marking MK-2", d("3.5"), "m2", &attrs(&[("width_mm", "150")]));
    s.push_decimal(Some(e(3)), "MK-3", "marking.painted_area", "Marking MK-3", d("1.25"), "m2", &attrs(&[("width_mm", "100.0")]));
    s.push_decimal(Some(e(4)), "C-1", "culvert.length", "Culvert C-1", d("24"), "m", &attrs(&[("internal_diameter_mm", "900")]));
    s.push_decimal(Some(e(5)), "S-1", "sign.face", "Sign S-1", d("1"), "nr", &attrs(&[("kind", "Direction")]));
    s
}

/// The same project one revision later: MK-2 is 4.0 m2, a new MK-4 of 1.5 m2 at 100 mm, no culvert.
pub fn lines_revised() -> QuantitySet {
    let mut s = QuantitySet::new();
    s.push_decimal(Some(e(1)), "MK-1", "marking.painted_area", "Marking MK-1", d("2.0"), "m2", &attrs(&[("width_mm", "100")]));
    s.push_decimal(Some(e(2)), "MK-2", "marking.painted_area", "Marking MK-2", d("4.0"), "m2", &attrs(&[("width_mm", "150")]));
    s.push_decimal(Some(e(3)), "MK-3", "marking.painted_area", "Marking MK-3", d("1.25"), "m2", &attrs(&[("width_mm", "100")]));
    s.push_decimal(Some(e(6)), "MK-4", "marking.painted_area", "Marking MK-4", d("1.5"), "m2", &attrs(&[("width_mm", "100")]));
    s.push_decimal(Some(e(5)), "S-1", "sign.face", "Sign S-1", d("1"), "nr", &attrs(&[("kind", "Direction")]));
    s
}

pub fn bill() -> Bill {
    let mut b = Bill::neutral(2);
    b.sections[0].title = "Markings".into();
    b.sections[1].title = "Drainage and extras".into();
    b.items = vec![
        Item::measured("1.01", "1", "Marking 100 mm", "m2"),
        Item::measured("1.02", "1", "Marking 150 mm", "m2"),
        Item::measured("2.01", "2", "Pipe culvert 900 mm", "m"),
        Item::manual("2.02", "2", "Allowance", "nr", d("10"), "not in the model: entered by the estimator"),
    ];
    b.rules = vec![
        MappingRule::new("m100", "1.01", "marking.painted_area", vec![Condition::equals("width_mm", "100")]),
        MappingRule::new("m150", "1.02", "marking.painted_area", vec![Condition::equals("width_mm", "150")]),
        MappingRule::new("c900", "2.01", "culvert.length", vec![Condition::equals("internal_diameter_mm", "900")]),
    ];
    b.adjustments = vec![Adjustment { item: "1.01".into(), delta: d("-0.25"), reason: "overlap with existing marking".into() }];
    b
}

pub fn rates() -> RateLibrary {
    let mut lib = RateLibrary::default();
    let mut put = |code: &str, unit: &str, rate: &str, cur: &str| {
        lib.set(Rate {
            item_code: code.into(),
            description: format!("rate {code}"),
            unit: unit.into(),
            rate: d(rate),
            currency: cur.into(),
            source_note: "test fixture".into(),
            date: Some(date("2026-01-10")),
        })
    };
    put("1.01", "m2", "15500.50", "UGX");
    put("1.02", "m2", "21333.33", "UGX");
    put("2.01", "m", "485000", "UGX");
    put("2.02", "nr", "125.50", "USD");
    lib
}

/// UGX (whole shillings) reporting, USD (cents) alongside, 1 USD = 3700 UGX.
pub fn estimate(with_additions: bool) -> Estimate {
    let mut est = Estimate::new("Test estimate", Currency::new("UGX", 0));
    est.revision = "Rev A".into();
    est.currencies.add_currency(Currency::new("USD", 2));
    est.currencies.add_rate("USD", "UGX", d("3700"), date("2026-01-15"), "test fixture").unwrap();
    est.secondary_currency = Some("USD".into());
    est.bill = bill();
    est.rates = rates();
    if with_additions {
        est.additions = vec![
            Addition { id: "prelims".into(), name: "Preliminaries".into(), kind: AdditionKind::Percentage { percent: d("12.5"), base: AdditionBase::BillTotal } },
            Addition { id: "cont".into(), name: "Contingency".into(), kind: AdditionKind::Percentage { percent: d("4"), base: AdditionBase::RunningTotal } },
            Addition { id: "prov".into(), name: "Provisional sum".into(), kind: AdditionKind::LumpSum(Money::new(d("1000"), "USD")) },
            Addition { id: "tax".into(), name: "Tax".into(), kind: AdditionKind::Percentage { percent: d("7.5"), base: AdditionBase::RunningTotal } },
        ];
    }
    est
}
