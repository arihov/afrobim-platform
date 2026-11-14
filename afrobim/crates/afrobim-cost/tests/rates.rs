mod common;

use afrobim_cost::*;
use common::*;

#[test]
fn the_rate_library_round_trips_through_csv() {
    let mut lib = RateLibrary::default();
    lib.set(Rate {
        item_code: "1.01".into(),
        description: "Line, thermoplastic \"100 mm\"".into(),
        unit: "m2".into(),
        rate: d("15500.50"),
        currency: "UGX".into(),
        source_note: "quotation 12, supplier A\nsecond line".into(),
        date: Some(date("2026-01-10")),
    });
    lib.set(Rate { item_code: "1.01".into(), description: "same item in dollars".into(), unit: "m2".into(), rate: d("4.2"), currency: "USD".into(), source_note: "".into(), date: None });
    lib.set(Rate { item_code: "2.01".into(), description: "Culvert".into(), unit: "m".into(), rate: d("0.000125"), currency: "UGX".into(), source_note: "x".into(), date: Some(date("2025-12-31")) });
    let text = lib.to_csv();
    assert!(text.starts_with("item_code,description,unit,rate,currency,source_note,date\n"));
    assert!(text.contains("\"Line, thermoplastic \"\"100 mm\"\"\""));
    let back = RateLibrary::from_csv(&text).unwrap();
    assert_eq!(back, lib);
    // Setting the same code and currency again replaces, the other currency stays.
    let mut lib2 = back.clone();
    lib2.set(Rate { item_code: "1.01".into(), description: "".into(), unit: "m2".into(), rate: d("16000"), currency: "ugx".into(), source_note: "".into(), date: None });
    assert_eq!(lib2.rates.len(), 3);
    assert_eq!(lib2.find("1.01", Some("UGX"), "USD").unwrap().rate, d("16000"));
    // Without a currency: the preferred one, else the first.
    assert_eq!(lib2.find("1.01", None, "USD").unwrap().rate, d("4.2"));
    assert_eq!(lib2.find("1.01", None, "EUR").unwrap().currency, "UGX");
    assert!(lib2.find("9.99", None, "UGX").is_none());
}

#[test]
fn a_bad_rate_file_names_the_row() {
    let head = "item_code,description,unit,rate,currency,source_note,date\n";
    assert!(RateLibrary::from_csv(&format!("{head}1,x,m,12.5,UGX,,\n")).is_ok());
    let err = RateLibrary::from_csv(&format!("{head}1,x,m,12.5,UGX,,\n2,y,m,twelve,UGX,,\n")).unwrap_err().to_string();
    assert!(err.contains("row 3") && err.contains("twelve"), "{err}");
    let err = RateLibrary::from_csv(&format!("{head}1,x,m,12.5,UGX,,10/01/2026\n")).unwrap_err().to_string();
    assert!(err.contains("row 2") && err.contains("10/01/2026"), "{err}");
    assert!(RateLibrary::from_csv(&format!("{head}1,x,m,1,2\n")).is_err());
    assert!(RateLibrary::from_csv("code,rate\n").is_err());
    assert!(RateLibrary::from_csv(&format!("{head},x,m,1,UGX,,\n")).is_err());
    assert!(RateLibrary::from_csv("").is_err());
}

fn comp(kind: BuildUpKind, description: &str, value: Entry) -> Component {
    Component { kind, description: description.into(), value }
}

fn build_up(components: Vec<Component>) -> BuildUp {
    BuildUp { item_code: "1.01".into(), description: "Marking".into(), unit: "m2".into(), currency: "UGX".into(), components }
}

#[test]
fn a_rate_build_up_sums_to_the_rate() {
    use BuildUpKind::*;
    // Direct 1000 + 500 + 2500 = 4000; overheads 10% of direct = 400; profit 5% of everything
    // before it (4400) = 220; the rate is 4620.
    let b = build_up(vec![
        comp(Labour, "gang", Entry::Amount(d("1000"))),
        comp(Plant, "marking machine", Entry::Amount(d("500"))),
        comp(Materials, "thermoplastic", Entry::Amount(d("2500"))),
        comp(Overheads, "overheads", Entry::Percent { percent: d("10"), of: PercentBase::Direct }),
        comp(Profit, "profit", Entry::Percent { percent: d("5"), of: PercentBase::Running }),
    ]);
    let r = b.total(Rounding::half_up(2)).unwrap();
    assert_eq!((r.direct, r.rate), (d("4000"), d("4620")));
    assert_eq!(r.components.iter().map(|(_, a)| *a).collect::<Vec<_>>(), vec![d("1000"), d("500"), d("2500"), d("400"), d("220")]);
    // Profit on direct plus overheads gives the same here; profit on direct alone gives 200.
    let mut b2 = b.clone();
    b2.components[4].value = Entry::Percent { percent: d("5"), of: PercentBase::Direct };
    assert_eq!(b2.total(Rounding::half_up(2)).unwrap().rate, d("4600"));
    b2.components[4].value = Entry::Percent { percent: d("5"), of: PercentBase::DirectAndOverheads };
    assert_eq!(b2.total(Rounding::half_up(2)).unwrap().rate, d("4620"));
    // Entered amounts for overheads and profit are as good as percentages.
    b2.components[3].value = Entry::Amount(d("450"));
    b2.components[4].value = Entry::Amount(d("100"));
    assert_eq!(b2.total(Rounding::half_up(2)).unwrap().rate, d("4550"));
}

#[test]
fn build_up_rounding_and_rules() {
    use BuildUpKind::*;
    // 7.5% of 333.33 = 24.99975 -> 25.00 at 2 decimals; rate 358.33.
    let b = build_up(vec![comp(Materials, "m", Entry::Amount(d("333.33"))), comp(Overheads, "o", Entry::Percent { percent: d("7.5"), of: PercentBase::Direct })]);
    let r = b.total(Rounding::half_up(2)).unwrap();
    assert_eq!((r.components[1].1, r.rate), (d("25"), d("358.33")));
    // A direct cost cannot be a percentage.
    let bad = build_up(vec![comp(Labour, "l", Entry::Percent { percent: d("10"), of: PercentBase::Direct })]);
    assert!(bad.total(Rounding::half_up(2)).is_err());
    // The summed rate goes into the library with its note and date.
    let mut lib = RateLibrary::default();
    lib.set_from_build_up(&b, Rounding::half_up(2), "built up from entered components", Some(date("2026-02-01"))).unwrap();
    let r = lib.find("1.01", None, "UGX").unwrap();
    assert_eq!((r.rate, r.unit.as_str(), r.currency.as_str(), r.date), (d("358.33"), "m2", "UGX", Some(date("2026-02-01"))));
    // And the estimate prices with it.
    let mut est = Estimate::new("x", Currency::new("UGX", 0));
    est.bill = Bill::neutral(1);
    est.bill.items = vec![Item::measured("1.01", "1", "Marking", "m2")];
    est.bill.rules = vec![MappingRule::new("r", "1.01", "x", vec![])];
    est.rates = lib;
    let mut set = QuantitySet::new();
    set.push_decimal(None, "A", "x", "", d("10"), "m2", &[]);
    assert_eq!(est.compute(&set).unwrap().subtotal, d("3583")); // 3583.30 to whole shillings
}

#[test]
fn an_item_can_take_a_rate_under_another_code_and_currency() {
    let mut est = estimate(false);
    est.rates.set(Rate { item_code: "SHARED".into(), description: "".into(), unit: "m2".into(), rate: d("100"), currency: "UGX".into(), source_note: "".into(), date: None });
    est.rates.set(Rate { item_code: "SHARED".into(), description: "".into(), unit: "m2".into(), rate: d("0.5"), currency: "USD".into(), source_note: "".into(), date: None });
    est.bill.items[1].rate_code = Some("SHARED".into());
    // Preferred currency is the reporting one: 3.5 m2 x 100 UGX.
    assert_eq!(est.compute(&lines()).unwrap().item("1.02").unwrap().amount_reporting, d("350"));
    // Asking for USD: 3.5 x 0.50 = 1.75 USD = 6475 UGX.
    est.bill.items[1].rate_currency = Some("USD".into());
    let r = est.compute(&lines()).unwrap();
    assert_eq!((r.item("1.02").unwrap().amount, r.item("1.02").unwrap().amount_reporting), (d("1.75"), d("6475")));
}
