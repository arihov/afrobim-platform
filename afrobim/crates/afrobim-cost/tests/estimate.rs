mod common;

use afrobim_cost::*;
use common::*;

#[test]
fn items_sections_and_both_currencies_side_by_side() {
    let r = estimate(false).compute(&lines()).unwrap();
    // 3.0 m2 x 15500.50 = 46501.50 -> 46502; 3.5 x 21333.33 = 74666.655 -> 74667;
    // 24 x 485000 = 11640000; 10 nr x 125.50 USD = 1255.00 USD = 4643500 UGX at 3700.
    let amounts: Vec<_> = r.items.iter().map(|i| i.amount_reporting).collect();
    assert_eq!(amounts, vec![d("46502"), d("74667"), d("11640000"), d("4643500")]);
    assert_eq!(r.item("2.02").unwrap().amount, d("1255.00"));
    assert_eq!(r.item("2.02").unwrap().rate_currency.as_deref(), Some("USD"));
    // USD beside it: each converted by 3700 and rounded to cents.
    let usd: Vec<_> = r.items.iter().map(|i| i.amount_secondary.unwrap()).collect();
    assert_eq!(usd, vec![d("12.57"), d("20.18"), d("3145.95"), d("1255.00")]);
    assert_eq!((r.sections[0].amount_reporting, r.sections[0].amount_secondary), (d("121169"), Some(d("32.75"))));
    assert_eq!((r.sections[1].amount_reporting, r.sections[1].amount_secondary), (d("16283500"), Some(d("4400.95"))));
    assert_eq!((r.subtotal, r.subtotal_secondary), (d("16404669"), Some(d("4433.70"))));
    assert_eq!((r.grand_total, r.grand_total_secondary), (d("16404669"), Some(d("4433.70"))));
    // Native subtotals by the currency of the rate.
    assert_eq!(r.by_currency["UGX"], d("11761169"));
    assert_eq!(r.by_currency["USD"], d("1255.00"));
    // Sections add up to the total, items to their section.
    assert_eq!(r.sections.iter().map(|s| s.amount_reporting).sum::<Decimal>(), r.subtotal);
    assert!(r.unpriced.is_empty());
}

#[test]
fn additions_in_the_users_order_with_stated_bases() {
    let r = estimate(true).compute(&lines()).unwrap();
    // 12.5% of 16404669 = 2050583.625 -> 2050584; 4% of (16404669 + 2050584) = 738210.12;
    // lump 1000 USD = 3700000 UGX; 7.5% of 22893463 = 1717009.725 -> 1717010.
    let a: Vec<_> = r.additions.iter().map(|a| (a.id.as_str(), a.amount, a.running_total)).collect();
    assert_eq!(
        a,
        vec![
            ("prelims", d("2050584"), d("18455253")),
            ("cont", d("738210"), d("19193463")),
            ("prov", d("3700000"), d("22893463")),
            ("tax", d("1717010"), d("24610473")),
        ]
    );
    assert_eq!(r.additions[0].base, Some(d("16404669")));
    assert_eq!(r.additions[1].base, Some(d("18455253")));
    assert_eq!(r.additions[2].base, None);
    assert_eq!(r.grand_total, d("24610473"));
    // Secondary currency: each addition converted and rounded; the lump sum is exactly 1000.00.
    let usd: Vec<_> = r.additions.iter().map(|a| a.amount_secondary.unwrap()).collect();
    assert_eq!(usd, vec![d("554.21"), d("199.52"), d("1000.00"), d("464.06")]);
    assert_eq!(r.grand_total_secondary, Some(d("6651.49")));
}

#[test]
fn the_order_of_additions_changes_the_result() {
    let mut est = Estimate::new("order", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("1", "1", "Work", "nr")];
    est.bill.rules = vec![MappingRule::new("r", "1", "x", vec![])];
    est.rates.set(Rate { item_code: "1".into(), description: "".into(), unit: "nr".into(), rate: d("1000"), currency: "UGX".into(), source_note: "".into(), date: None });
    let mut set = QuantitySet::new();
    set.push_decimal(None, "A", "x", "", d("1"), "nr", &[]);
    let pct = |p: &str| Addition { id: "pct".into(), name: "pct".into(), kind: AdditionKind::Percentage { percent: d(p), base: AdditionBase::RunningTotal } };
    let lump = Addition { id: "lump".into(), name: "lump".into(), kind: AdditionKind::LumpSum(Money::new(d("100"), "UGX")) };
    // Lump first: 1000 + 100 = 1100, then 10% of 1100 = 110: 1210.
    est.additions = vec![lump.clone(), pct("10")];
    assert_eq!(est.compute(&set).unwrap().grand_total, d("1210"));
    // Percentage first: 10% of 1000 = 100, then the lump: 1200.
    est.additions = vec![pct("10"), lump];
    assert_eq!(est.compute(&set).unwrap().grand_total, d("1200"));
}

#[test]
fn additions_can_be_taken_of_a_section_or_of_other_additions() {
    let mut est = estimate(false);
    est.additions = vec![
        Addition { id: "a".into(), name: "A".into(), kind: AdditionKind::Percentage { percent: d("10"), base: AdditionBase::Section("1".into()) } },
        Addition { id: "b".into(), name: "B".into(), kind: AdditionKind::LumpSum(Money::new(d("500"), "UGX")) },
        Addition { id: "c".into(), name: "C".into(), kind: AdditionKind::Percentage { percent: d("50"), base: AdditionBase::Additions(vec!["a".into(), "b".into()]) } },
    ];
    let r = est.compute(&lines()).unwrap();
    // 10% of section 1 (121169) = 12116.9 -> 12117; 50% of (12117 + 500) = 6308.5 -> 6309.
    assert_eq!(r.additions[0].amount, d("12117"));
    assert_eq!(r.additions[2].base, Some(d("12617")));
    assert_eq!(r.additions[2].amount, d("6309"));
    // A reference to a later or missing addition is refused.
    est.additions.swap(0, 2);
    assert!(est.compute(&lines()).is_err());
    est.additions = vec![Addition { id: "x".into(), name: "".into(), kind: AdditionKind::Percentage { percent: d("1"), base: AdditionBase::Section("nope".into()) } }];
    assert!(est.compute(&lines()).is_err());
}

#[test]
fn currency_conversion_both_ways_by_hand() {
    let mut t = CurrencyTable::default();
    t.add_currency(Currency::new("UGX", 0));
    t.add_currency(Currency::new("USD", 2));
    t.add_rate("USD", "UGX", d("3700.5"), date("2026-01-15"), "entered").unwrap();
    let to_usd = Rounding::half_up(2);
    let to_ugx = Rounding::half_up(0);
    // UGX to USD divides: 1,000,000 / 3700.5 = 270.2337... -> 270.23.
    assert_eq!(t.convert(d("1000000"), "UGX", "USD", None, to_usd).unwrap(), d("270.23"));
    // USD to UGX multiplies: 270.23 x 3700.5 = 999986.115 -> 999986.
    assert_eq!(t.convert(d("270.23"), "USD", "UGX", None, to_ugx).unwrap(), d("999986"));
    // Same currency: only rounding.
    assert_eq!(t.convert(d("12.345"), "USD", "usd", None, to_usd).unwrap(), d("12.35"));
    // A currency without a rate is an error, not a guess.
    t.add_currency(Currency::new("EUR", 2));
    assert!(matches!(t.convert(d("1"), "EUR", "UGX", None, to_ugx), Err(CostError::NoExchangeRate { .. })));
    // Rates must be positive and between defined, different currencies.
    assert!(t.add_rate("USD", "UGX", d("0"), date("2026-01-15"), "").is_err());
    assert!(t.add_rate("USD", "USD", d("1"), date("2026-01-15"), "").is_err());
    assert!(t.add_rate("USD", "KES", d("1"), date("2026-01-15"), "").is_err());
}

#[test]
fn the_exchange_rate_in_force_is_chosen_by_date() {
    let mut est = estimate(false);
    est.currencies.add_rate("USD", "UGX", d("3800"), date("2026-03-01"), "later").unwrap();
    // No date: the latest. 10 x 125.50 USD = 1255.00 -> 4,769,000 UGX.
    assert_eq!(est.compute(&lines()).unwrap().item("2.02").unwrap().amount_reporting, d("4769000"));
    // As of a date between the two: the earlier rate.
    est.as_of = Some(date("2026-02-01"));
    assert_eq!(est.compute(&lines()).unwrap().item("2.02").unwrap().amount_reporting, d("4643500"));
    // As of a date before any rate was taken: refused.
    est.as_of = Some(date("2025-12-31"));
    assert!(matches!(est.compute(&lines()), Err(CostError::NoExchangeRate { .. })));
}

#[test]
fn per_item_rounding_against_totals_only() {
    let mut est = Estimate::new("round", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = (1..=3).map(|n| Item::measured(&n.to_string(), "1", "x", "nr")).collect();
    est.bill.rules = (1..=3).map(|n| MappingRule::new("r", &n.to_string(), &format!("c{n}"), vec![])).collect();
    let mut set = QuantitySet::new();
    for n in 1..=3 {
        est.rates.set(Rate { item_code: n.to_string(), description: "".into(), unit: "nr".into(), rate: d("0.4"), currency: "UGX".into(), source_note: "".into(), date: None });
        set.push_decimal(None, "A", &format!("c{n}"), "", d("1"), "nr", &[]);
    }
    // Each 0.4 rounds to 0: the total of rounded amounts is 0.
    assert_eq!(est.compute(&set).unwrap().subtotal, d("0"));
    // Rounding only the total: 1.2 rounds to 1.
    est.bill.rounding.amount_rounding = AmountRounding::TotalsOnly;
    assert_eq!(est.compute(&set).unwrap().subtotal, d("1"));
    // Half to even for amounts: 2.5 -> 2 (and half away: 3).
    est.bill.rounding.amount_rounding = AmountRounding::PerItem;
    est.bill.rounding.amount_mode = RoundingMode::HalfEven;
    est.rates.set(Rate { item_code: "1".into(), description: "".into(), unit: "nr".into(), rate: d("2.5"), currency: "UGX".into(), source_note: "".into(), date: None });
    assert_eq!(est.compute(&set).unwrap().item("1").unwrap().amount_reporting, d("2"));
    est.bill.rounding.amount_mode = RoundingMode::HalfAwayFromZero;
    assert_eq!(est.compute(&set).unwrap().item("1").unwrap().amount_reporting, d("3"));
}

#[test]
fn items_without_a_usable_rate_are_listed_and_priced_at_zero() {
    let mut est = estimate(false);
    est.rates.rates.retain(|r| r.item_code != "1.02");
    est.rates.set(Rate { item_code: "2.01".into(), description: "".into(), unit: "km".into(), rate: d("1"), currency: "UGX".into(), source_note: "".into(), date: None });
    est.rates.rates.retain(|r| !(r.item_code == "2.01" && r.unit == "m"));
    let r = est.compute(&lines()).unwrap();
    assert_eq!(r.unpriced.iter().map(|u| u.code.as_str()).collect::<Vec<_>>(), vec!["1.02", "2.01"]);
    assert!(r.unpriced[0].reason.contains("no rate"));
    assert!(r.unpriced[1].reason.contains("per 'km'"));
    assert_eq!(r.item("1.02").unwrap().amount_reporting, d("0"));
    assert_eq!(r.subtotal, d("46502") + d("4643500"));
}

#[test]
fn comparison_of_two_model_revisions() {
    let est = estimate(false);
    let c = est.compare_revisions(&lines(), &lines_revised()).unwrap();
    let it = |code: &str| c.items.iter().find(|i| i.code == code).unwrap();
    // 1.01: 3.0 -> 4.5 m2 (MK-4 added); 46502 -> 4.5 x 15500.50 = 69752.25 -> 69752: +23250, +50.00 %.
    let i = it("1.01");
    assert_eq!((i.quantity_old, i.quantity_new, i.quantity_change), (d("3"), d("4.5"), d("1.5")));
    assert_eq!((i.amount_old, i.amount_new, i.amount_change), (d("46502"), d("69752"), d("23250")));
    assert_eq!((i.percent_change, i.status), (Some(d("50")), ChangeStatus::Changed));
    // 1.02: 3.5 -> 4.0; 74667 -> 85333 (4 x 21333.33 = 85333.32): +10666, +14.28 %.
    let i = it("1.02");
    assert_eq!((i.amount_change, i.percent_change), (d("10666"), Some(d("14.28"))));
    // 2.01: the culvert is gone from the model: 24 -> 0 m, -11,640,000, -100 %.
    let i = it("2.01");
    assert_eq!((i.quantity_change, i.amount_change, i.percent_change), (d("-24"), d("-11640000"), Some(d("-100"))));
    // 2.02 is a manual item: unchanged.
    assert_eq!(it("2.02").status, ChangeStatus::Unchanged);
    assert_eq!(c.subtotal_old, d("16404669"));
    assert_eq!(c.subtotal_new, d("69752") + d("85333") + d("4643500"));
    assert_eq!(c.subtotal_change, c.subtotal_new - c.subtotal_old);
    assert_eq!(c.grand_change, c.subtotal_change);
    assert!(comparison_csv(&c).contains("item,1.01,Marking 100 mm,m2,Changed,3,4.5,1.5,46502.00,69752.00,23250.00,50"));
}

#[test]
fn comparison_of_two_estimates_with_added_and_removed_items() {
    let old = estimate(true);
    let mut new = estimate(true);
    new.revision = "Rev B".into();
    new.bill.items.retain(|i| i.code != "1.02");
    new.bill.items.push(Item::manual("2.03", "2", "New extra", "nr", d("2"), "added in Rev B"));
    new.rates.set(Rate { item_code: "2.03".into(), description: "".into(), unit: "nr".into(), rate: d("1000"), currency: "UGX".into(), source_note: "".into(), date: None });
    new.additions.remove(1);
    new.bill.rules.retain(|r| r.item != "1.02");
    let (ro, rn) = (old.compute(&lines()).unwrap(), new.compute(&lines()).unwrap());
    let c = compare(&ro, &rn);
    let status = |code: &str| c.items.iter().find(|i| i.code == code).unwrap().status;
    assert_eq!(status("1.02"), ChangeStatus::Removed);
    assert_eq!(status("2.03"), ChangeStatus::Added);
    assert_eq!(c.items.iter().find(|i| i.code == "2.03").unwrap().percent_change, None);
    assert_eq!(c.items.last().unwrap().code, "1.02");
    assert_eq!(c.new_label, "Test estimate Rev B");
    let cont = c.additions.iter().find(|a| a.id == "cont").unwrap();
    assert_eq!((cont.amount_old, cont.amount_new), (d("738210"), d("0")));
    assert_eq!(c.grand_change, rn.grand_total - ro.grand_total);
}

#[test]
fn an_estimate_is_serde_and_the_result_is_deterministic() {
    let est = estimate(true);
    let json = serde_json::to_string(&est).unwrap();
    let back: Estimate = serde_json::from_str(&json).unwrap();
    assert_eq!(back, est);
    assert_eq!(back.compute(&lines()).unwrap(), est.compute(&lines()).unwrap());
}

#[test]
fn csv_outputs_priced_and_unpriced() {
    let r = estimate(true).compute(&lines()).unwrap();
    let priced = priced_boq_csv(&r);
    let rows: Vec<&str> = priced.lines().collect();
    assert_eq!(rows[0], "type,code,description,unit,quantity,rate,rate_currency,amount_native,amount_UGX,amount_USD");
    assert_eq!(rows[1], "section,1,Markings,,,,,,,");
    assert_eq!(rows[2], "item,1.01,Marking 100 mm,m2,3,15500.5,UGX,46502,46502,12.57");
    assert!(priced.contains("item,2.02,Allowance,nr,10,125.5,USD,1255.00,4643500,1255.00"));
    assert!(priced.contains("section_total,1,Total of Markings,,,,,,121169,32.75"));
    assert!(priced.contains("subtotal,,Bill total,,,,,,16404669,4433.70"));
    assert!(priced.contains("addition,prelims,Preliminaries (12.5% of 16404669),,,,,,2050584,554.21"));
    assert!(priced.contains("addition,prov,Provisional sum (lump sum),,,,,,3700000,1000.00"));
    assert!(priced.trim_end().ends_with("grand_total,,Total,,,,,,24610473,6651.49"));
    let unpriced = unpriced_boq_csv(&r);
    assert_eq!(
        unpriced,
        "type,code,description,unit,quantity\nsection,1,Markings,,\nitem,1.01,Marking 100 mm,m2,3\nitem,1.02,Marking 150 mm,m2,3.5\n\
         section,2,Drainage and extras,,\nitem,2.01,Pipe culvert 900 mm,m,24\nitem,2.02,Allowance,nr,10\n"
    );
    let summary = estimate_summary_csv(&r);
    assert!(summary.starts_with("line,description,base,percent,amount_UGX,amount_USD\nsection 1,Markings,,,121169,32.75\n"));
    assert!(summary.contains("addition tax,Tax,22893463,7.5,1717010,464.06"));
    assert!(summary.trim_end().ends_with("grand total,,,,24610473,6651.49"));
}
