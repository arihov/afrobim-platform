// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
mod common;

use afrobim_cost::*;
use common::*;

fn item_window(code: &str, start: &str, finish: &str) -> Phasing {
    Phasing { target: PhasingTarget::Item(code.into()), start: date(start), finish: date(finish), distribution: Distribution::Linear }
}

#[test]
fn spread_by_day_count_by_hand() {
    // 2026-01-20 to 2026-03-10 inclusive: Jan 20..31 = 12 days, Feb = 28, Mar 1..10 = 10; 50 days.
    // 100,000: Jan 12/50 = 24,000; Feb 28/50 = 56,000; Mar 10/50 = 20,000.
    let s = spread_linear(d("100000"), date("2026-01-20"), date("2026-03-10"), Rounding::half_up(0)).unwrap();
    assert_eq!(s, vec![((2026, 1), d("24000")), ((2026, 2), d("56000")), ((2026, 3), d("20000"))]);
    // Whole shillings, an uneven split: 1000 over Jan 29..Feb 4 (7 days: 3 and 4).
    // Jan 3/7 x 1000 = 428.57 -> 429; February takes the rest, 571 (4/7 is 571.43).
    let s = spread_linear(d("1000"), date("2026-01-29"), date("2026-02-04"), Rounding::half_up(0)).unwrap();
    assert_eq!(s, vec![((2026, 1), d("429")), ((2026, 2), d("571"))]);
    // With cents: 428.57 and 571.43.
    let s = spread_linear(d("1000"), date("2026-01-29"), date("2026-02-04"), Rounding::half_up(2)).unwrap();
    assert_eq!(s, vec![((2026, 1), d("428.57")), ((2026, 2), d("571.43"))]);
    // One day, one month; a leap February counts 29 days.
    assert_eq!(spread_linear(d("10"), date("2026-05-05"), date("2026-05-05"), Rounding::half_up(0)).unwrap(), vec![((2026, 5), d("10"))]);
    let s = spread_linear(d("590"), date("2024-02-01"), date("2024-03-30"), Rounding::half_up(0)).unwrap(); // 29 + 30 days
    assert_eq!(s, vec![((2024, 2), d("290")), ((2024, 3), d("300"))]);
    // Months always add back to the amount.
    let s = spread_linear(d("1234567"), date("2026-01-03"), date("2026-11-17"), Rounding::half_up(0)).unwrap();
    assert_eq!(s.iter().map(|(_, a)| *a).sum::<Decimal>(), d("1234567"));
    assert_eq!(s.len(), 11);
    assert!(spread_linear(d("1"), date("2026-02-01"), date("2026-01-01"), Rounding::half_up(0)).is_err());
}

#[test]
fn the_cash_flow_of_items_and_elements() {
    let mut est = estimate(false);
    // Item 2.01 (culvert, 11,640,000) over Jan 20..Mar 10: 2,793,600 / 6,518,400 / 2,328,000.
    // Element 1 (MK-1, 2.0 m2 of item 1.01 at 15500.50 = 31,001) over Jan 29..Feb 4:
    // Jan 3/7 x 31001 = 13,286.14 -> 13,286; Feb 17,715.
    est.phasing = vec![
        item_window("2.01", "2026-01-20", "2026-03-10"),
        Phasing { target: PhasingTarget::Element(e(1)), start: date("2026-01-29"), finish: date("2026-02-04"), distribution: Distribution::Linear },
    ];
    let (r, cf) = est.compute_with_cash_flow(&lines()).unwrap();
    let months: Vec<_> = cf.months.iter().map(|m| (m.year, m.month, m.amount, m.cumulative)).collect();
    assert_eq!(
        months,
        vec![(2026, 1, d("2806886"), d("2806886")), (2026, 2, d("6536115"), d("9343001")), (2026, 3, d("2328000"), d("11671001"))]
    );
    assert_eq!(cf.phased_total, d("11671001"));
    // What has no window: 16,404,669 - 11,671,001.
    assert_eq!(cf.grand_total, r.grand_total);
    assert_eq!(cf.unphased_total, d("4733668"));
    // S-curve: cumulative share of the phased total, 2 decimals.
    assert_eq!(cf.months[0].cumulative_percent, d("24.05")); // 2806886 / 11671001 = 24.0499 %
    assert_eq!(cf.months[2].cumulative_percent, d("100"));
    let s = cf.s_curve();
    assert_eq!(s.len(), 4);
    assert_eq!((s[0].date, s[0].cumulative), (date("2025-12-31"), d("0")));
    assert_eq!((s[1].date, s[3].date, s[3].percent), (date("2026-01-31"), date("2026-03-31"), d("100")));
    let csv = cash_flow_csv(&cf, 0);
    assert!(csv.starts_with("year,month,amount_UGX,cumulative,cumulative_percent\n2026,01,2806886,2806886,24.05\n"));
    assert!(csv.contains("not phased,,4733668,,"));
}

#[test]
fn year_end_gaps_and_additions() {
    let mut est = estimate(true);
    // Item 2.02 (4,643,500) over Dec 30..Jan 2: 2 days each side, 2,321,750 each.
    // Item 1.02 (74,667) in March 2027 only, leaving February 2027 empty.
    // Addition "prelims" (2,050,584) over Jan 1..Jan 31 2027.
    est.phasing = vec![
        item_window("2.02", "2026-12-30", "2027-01-02"),
        item_window("1.02", "2027-03-01", "2027-03-31"),
        Phasing { target: PhasingTarget::Addition("prelims".into()), start: date("2027-01-01"), finish: date("2027-01-31"), distribution: Distribution::Linear },
    ];
    let (_, cf) = est.compute_with_cash_flow(&lines()).unwrap();
    let months: Vec<_> = cf.months.iter().map(|m| (m.year, m.month, m.amount)).collect();
    assert_eq!(
        months,
        vec![(2026, 12, d("2321750")), (2027, 1, d("2321750") + d("2050584")), (2027, 2, d("0")), (2027, 3, d("74667"))]
    );
    assert_eq!(cf.phased_total, d("4643500") + d("2050584") + d("74667"));
    assert_eq!(cf.unphased_total, d("24610473") - cf.phased_total);
}

#[test]
fn phasing_input_is_checked() {
    let est = estimate(false);
    let r = est.compute(&lines()).unwrap();
    let rounding = Rounding::half_up(0);
    // The same target twice, an unknown item, a window that ends before it starts.
    let twice = vec![item_window("2.01", "2026-01-01", "2026-01-31"), item_window("2.01", "2026-02-01", "2026-02-28")];
    assert!(cash_flow_with(&r, &twice, rounding).is_err());
    assert!(cash_flow_with(&r, &[item_window("9.99", "2026-01-01", "2026-01-31")], rounding).is_err());
    assert!(cash_flow_with(&r, &[item_window("2.01", "2026-02-01", "2026-01-31")], rounding).is_err());
    // An element inside an item that is phased as a whole is not phased twice.
    let both = vec![
        item_window("1.01", "2026-01-01", "2026-01-31"),
        Phasing { target: PhasingTarget::Element(e(1)), start: date("2026-02-01"), finish: date("2026-02-28"), distribution: Distribution::Linear },
    ];
    let cf = cash_flow_with(&r, &both, rounding).unwrap();
    assert_eq!(cf.phased_total, d("46502"));
    assert_eq!(cf.warnings.len(), 1);
    // No phasing at all: no months, everything unphased.
    let cf = cash_flow_with(&r, &[], rounding).unwrap();
    assert!(cf.months.is_empty() && cf.s_curve().is_empty());
    assert_eq!(cf.unphased_total, r.grand_total);
}

#[test]
fn phasing_is_serde() {
    let p = item_window("2.01", "2026-01-20", "2026-03-10");
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"2026-01-20\""));
    assert_eq!(serde_json::from_str::<Phasing>(&json).unwrap(), p);
}
