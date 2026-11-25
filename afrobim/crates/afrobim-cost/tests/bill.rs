// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
mod common;

use afrobim_cost::*;
use common::*;

#[test]
fn mapping_and_summing_with_an_audit_trail() {
    let m = bill().measure(&lines()).unwrap();
    // 1.01 takes MK-1 (2.0) and MK-3 (1.25; "100.0" equals "100" as a number), less the 0.25 adjustment.
    let i = m.item("1.01").unwrap();
    assert_eq!(i.from_lines, d("3.25"));
    assert_eq!(i.unrounded, d("3.0"));
    assert_eq!(i.quantity, d("3"));
    assert_eq!(i.contributions.iter().map(|c| (c.line, c.mark.as_str(), c.rule.as_str())).collect::<Vec<_>>(), vec![(0, "MK-1", "m100"), (2, "MK-3", "m100")]);
    assert_eq!(i.elements(), vec![e(1), e(3)]);
    assert_eq!(i.quantity_of(e(3)), d("1.25"));
    assert_eq!(i.adjustments[0].reason, "overlap with existing marking");
    // 1.02 takes MK-2 only; 2.01 the culvert; the manual item has no lines.
    assert_eq!(m.item("1.02").unwrap().quantity, d("3.5"));
    assert_eq!(m.item("2.01").unwrap().elements(), vec![e(4)]);
    let manual = m.item("2.02").unwrap();
    assert!(manual.contributions.is_empty());
    assert_eq!((manual.manual, manual.quantity), (Some(d("10")), d("10")));
}

#[test]
fn unmapped_lines_are_reported_never_dropped() {
    let set = lines();
    let m = bill().measure(&set).unwrap();
    assert_eq!(m.lines_offered, 5);
    assert_eq!(m.lines_mapped, 4);
    assert_eq!(m.unmapped.len(), 1);
    assert_eq!(m.lines_mapped + m.unmapped.len(), m.lines_offered);
    assert_eq!(m.unmapped[0].line.mark, "S-1");
    assert_eq!(m.unmapped[0].reason, UnmappedReason::NoRule);
    assert_eq!(m.unmapped_totals(), vec![("sign.face".to_string(), "nr".to_string(), 1, d("1"))]);
    // The same lines appear in the audit CSV.
    let r = estimate(false).compute(&set).unwrap();
    assert!(audit_csv(&r).contains(",unmapped,4,5,S-1,sign.face,1,nr,"));
    assert!(unmapped_csv(&r).contains("no rule matches"));
}

#[test]
fn a_unit_mismatch_is_unmapped_unless_the_rule_gives_a_factor() {
    let mut set = QuantitySet::new();
    set.push_decimal(Some(e(1)), "MK-1", "marking.painted_length", "length", d("20"), "m", &[("width_mm", "100".to_string())]);
    let mut b = Bill::neutral(1);
    b.items = vec![Item::measured("1.01", "1", "Marking by area", "m2")];
    b.rules = vec![MappingRule::new("by_prefix", "1.01", "marking.*", vec![])];
    let m = b.measure(&set).unwrap();
    assert_eq!(m.item("1.01").unwrap().quantity, d("0"));
    assert_eq!(
        m.unmapped[0].reason,
        UnmappedReason::UnitMismatch { rule: "by_prefix".into(), line_unit: "m".into(), item_unit: "m2".into() }
    );
    // The user's own conversion: 0.1 m wide line, so 20 m of line is 2 m2.
    b.rules[0].factor = Some(d("0.1"));
    let m = b.measure(&set).unwrap();
    assert!(m.unmapped.is_empty());
    let c = &m.item("1.01").unwrap().contributions[0];
    assert_eq!((c.line_quantity, c.factor, c.quantity), (d("20"), d("0.1"), d("2")));
    assert_eq!(m.item("1.01").unwrap().quantity, d("2"));
}

#[test]
fn the_first_rule_wins_and_overlap_is_reported() {
    let mut b = bill();
    b.rules.push(MappingRule::new("any_marking_area", "1.02", "marking.painted_area", vec![Condition { key: "width_mm".into(), test: Match::AtLeast(d("100")) }]));
    let m = b.measure(&lines()).unwrap();
    // MK-1 fits m100 and the catch-all; it goes to m100. MK-2 fits m150 and the catch-all.
    assert_eq!(m.item("1.01").unwrap().contributions.len(), 2);
    assert_eq!(m.item("1.02").unwrap().contributions.len(), 1);
    assert_eq!(m.ambiguous.len(), 3);
    assert_eq!((m.ambiguous[0].line, m.ambiguous[0].used.as_str(), m.ambiguous[0].also.as_slice()), (0, "m100", &["any_marking_area".to_string()][..]));
}

#[test]
fn condition_kinds() {
    let mut set = QuantitySet::new();
    set.push_decimal(None, "P1", "post.length", "post", d("3"), "m", &[("section", "CHS 114.3 x 3.6".to_string()), ("finish", "".to_string())]);
    let line = &set.lines[0];
    let c = |key: &str, test: Match| Condition { key: key.into(), test }.clone();
    let rule = |conds: Vec<Condition>| MappingRule::new("r", "x", "post.*", conds).matches(line);
    assert!(rule(vec![c("section", Match::Contains("chs 114".into()))]));
    assert!(rule(vec![c("section", Match::OneOf(vec!["RHS".into(), "CHS 114.3 x 3.6".into()]))]));
    assert!(rule(vec![c("section", Match::Present)]));
    assert!(!rule(vec![c("finish", Match::Present)]));
    assert!(!rule(vec![c("missing", Match::Present)]));
    assert!(rule(vec![c("mark", Match::Equals("p1".into()))]));
    assert!(!rule(vec![c("section", Match::AtMost(d("1")))]));
    assert!(!MappingRule::new("r", "x", "sign.*", vec![]).matches(line));
    assert!(MappingRule::new("r", "x", "*", vec![]).matches(line));
}

#[test]
fn rounding_rules_are_explicit_and_configurable() {
    let mut set = QuantitySet::new();
    set.push_decimal(None, "A", "x.area", "area", d("1.2345"), "m2", &[]);
    set.push_decimal(None, "B", "x.count", "count", d("2.1"), "nr", &[]);
    let mut b = Bill::neutral(1);
    b.items = vec![Item::measured("1", "1", "Area", "m2"), Item::measured("2", "1", "Count", "nr")];
    b.rules = vec![MappingRule::new("a", "1", "x.area", vec![]), MappingRule::new("b", "2", "x.count", vec![])];
    // Default: 3 decimals, halves away from zero; 1.2345 gives 1.235.
    assert_eq!(b.measure(&set).unwrap().item("1").unwrap().quantity, d("1.235"));
    // Half to even gives 1.234 for the same sum.
    b.rounding.quantity = Rounding::new(3, RoundingMode::HalfEven);
    assert_eq!(b.measure(&set).unwrap().item("1").unwrap().quantity, d("1.234"));
    // Counts round up to whole numbers by unit.
    b.rounding.quantity_by_unit.insert("nr".into(), Rounding::new(0, RoundingMode::AwayFromZero));
    let m = b.measure(&set).unwrap();
    assert_eq!(m.item("2").unwrap().quantity, d("3"));
    // The unrounded sum is kept next to it.
    assert_eq!(m.item("2").unwrap().unrounded, d("2.1"));
}

#[test]
fn an_unsound_bill_is_refused_with_every_problem() {
    let mut b = bill();
    b.items.push(Item::measured("1.01", "9", "duplicate and no section", "m"));
    b.items.push(Item::manual("3.01", "1", "manual without reason", "nr", d("1"), " "));
    b.rules.push(MappingRule::new("lost", "9.99", "x", vec![]));
    b.rules.push(MappingRule::new("to_manual", "2.02", "x", vec![]));
    b.adjustments.push(Adjustment { item: "1.01".into(), delta: d("1"), reason: "".into() });
    let problems = b.validate();
    assert_eq!(problems.len(), 6, "{problems:?}");
    match b.measure(&lines()) {
        Err(CostError::Structure(p)) => assert_eq!(p.len(), 6),
        other => panic!("{other:?}"),
    }
}

#[test]
fn structure_and_rules_load_from_the_users_csv() {
    let mut b = Bill::default();
    b.from_structure_csv("kind,code,section,description,unit\nsection,A,,\"Markings, all\",\nitem,A1,A,Thermoplastic line,m2\nitem,A2,A,Studs,nr\n").unwrap();
    assert_eq!(b.sections, vec![Section { code: "A".into(), title: "Markings, all".into() }]);
    assert_eq!(b.items.len(), 2);
    assert_eq!(Bill::default().structure_to_csv().lines().count(), 1);
    let again = {
        let mut c = Bill::default();
        c.from_structure_csv(&b.structure_to_csv()).unwrap();
        c
    };
    assert_eq!(again.sections, b.sections);
    assert_eq!(again.items, b.items);

    b.rules_from_csv(
        "name,item,category,factor,conditions\n\
         line100,A1,marking.painted_area,,width_mm=100|150;colour~white\n\
         studs,A2,marking.marks,2,width_mm>=100;width_mm<=200;material?\n",
    )
    .unwrap();
    assert_eq!(b.rules.len(), 2);
    assert_eq!(b.rules[0].conditions[0].test, Match::OneOf(vec!["100".into(), "150".into()]));
    assert_eq!(b.rules[1].factor, Some(d("2")));
    let mut c = b.clone();
    c.rules_from_csv(&b.rules_to_csv()).unwrap();
    assert_eq!(c.rules, b.rules);
    assert!(b.rules_from_csv("name,item,category,factor,conditions\nr,A1,x,,width>>3\n").is_err());
    assert!(b.from_structure_csv("kind,code,section,description,unit\nthing,A,,x,\n").is_err());
}
