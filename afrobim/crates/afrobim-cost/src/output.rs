// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! CSV outputs. Amounts are written with their currency's decimals, quantities and rates exactly
//! as held.

use crate::{csv, CashFlow, Certificate, Comparison, Decimal, EstimateResult, UnmappedReason};

fn amount(v: Decimal, decimals: u32) -> String {
    v.to_fixed(decimals)
}

fn opt(v: Option<Decimal>, decimals: u32) -> String {
    v.map(|v| amount(v, decimals)).unwrap_or_default()
}

/// The priced bill: sections, items with quantity, rate and amount, section totals, bill total,
/// the additions in order and the grand total. With a secondary currency its amounts follow in a
/// last column. Items without a rate are listed with an empty rate and a zero amount.
pub fn priced_boq_csv(r: &EstimateResult) -> String {
    let rd = r.reporting.decimals;
    let mut header = vec![
        "type".to_string(),
        "code".into(),
        "description".into(),
        "unit".into(),
        "quantity".into(),
        "rate".into(),
        "rate_currency".into(),
        "amount_native".into(),
        format!("amount_{}", r.reporting.code),
    ];
    if let Some(s) = &r.secondary {
        header.push(format!("amount_{}", s.code));
    }
    let mut out = csv::row(&header);
    let line = |kind: &str, code: &str, desc: &str, unit: &str, qty: &str, rate: &str, cur: &str, native: &str, rep: Option<Decimal>, sec: Option<Decimal>| {
        let mut f = vec![kind.to_string(), code.into(), desc.into(), unit.into(), qty.into(), rate.into(), cur.into(), native.into(), opt(rep, rd)];
        if let Some(s) = &r.secondary {
            f.push(opt(sec, s.decimals));
        }
        csv::row(&f)
    };
    for s in &r.sections {
        out.push_str(&line("section", &s.code, &s.title, "", "", "", "", "", None, None));
        for i in r.items.iter().filter(|i| i.section == s.code) {
            let native = if i.rate.is_some() { amount(i.amount, i.native_decimals) } else { String::new() };
            out.push_str(&line(
                "item",
                &i.code,
                &i.description,
                &i.unit,
                &i.quantity.to_string(),
                &i.rate.map(|v| v.to_string()).unwrap_or_default(),
                i.rate_currency.as_deref().unwrap_or(""),
                &native,
                Some(i.amount_reporting),
                i.amount_secondary,
            ));
        }
        out.push_str(&line("section_total", &s.code, &format!("Total of {}", s.title), "", "", "", "", "", Some(s.amount_reporting), s.amount_secondary));
    }
    out.push_str(&line("subtotal", "", "Bill total", "", "", "", "", "", Some(r.subtotal), r.subtotal_secondary));
    for a in &r.additions {
        let desc = match (a.percent, a.base) {
            (Some(p), Some(b)) => format!("{} ({}% of {})", a.name, p, amount(b, rd)),
            _ => format!("{} (lump sum)", a.name),
        };
        out.push_str(&line("addition", &a.id, &desc, "", "", "", "", "", Some(a.amount), a.amount_secondary));
    }
    out.push_str(&line("grand_total", "", "Total", "", "", "", "", "", Some(r.grand_total), r.grand_total_secondary));
    out
}

/// The unpriced bill: sections and items with quantities only, for tenderers to price.
pub fn unpriced_boq_csv(r: &EstimateResult) -> String {
    let mut out = csv::row(&["type", "code", "description", "unit", "quantity"]);
    for s in &r.sections {
        out.push_str(&csv::row(&["section", &s.code, &s.title, "", ""]));
        for i in r.items.iter().filter(|i| i.section == s.code) {
            out.push_str(&csv::row(&["item", &i.code, &i.description, &i.unit, &i.quantity.to_string()]));
        }
    }
    out
}

/// The estimate summary: each section, the bill total, each addition with its base and the grand
/// total, in the reporting currency and, when present, the secondary.
pub fn estimate_summary_csv(r: &EstimateResult) -> String {
    let rd = r.reporting.decimals;
    let mut header = vec!["line".to_string(), "description".into(), "base".into(), "percent".into(), format!("amount_{}", r.reporting.code)];
    if let Some(s) = &r.secondary {
        header.push(format!("amount_{}", s.code));
    }
    let mut out = csv::row(&header);
    let mut put = |line: &str, desc: &str, base: String, pct: String, rep: Decimal, sec: Option<Decimal>| {
        let mut f = vec![line.to_string(), desc.into(), base, pct, amount(rep, rd)];
        if let Some(s) = &r.secondary {
            f.push(opt(sec, s.decimals));
        }
        out.push_str(&csv::row(&f));
    };
    for s in &r.sections {
        put(&format!("section {}", s.code), &s.title, String::new(), String::new(), s.amount_reporting, s.amount_secondary);
    }
    put("bill total", "", String::new(), String::new(), r.subtotal, r.subtotal_secondary);
    for a in &r.additions {
        put(
            &format!("addition {}", a.id),
            &a.name,
            a.base.map(|b| amount(b, rd)).unwrap_or_default(),
            a.percent.map(|p| p.to_string()).unwrap_or_default(),
            a.amount,
            a.amount_secondary,
        );
    }
    put("grand total", "", String::new(), String::new(), r.grand_total, r.grand_total_secondary);
    out
}

/// The audit trail: every item with each line, element and rule behind its quantity, then its
/// adjustments and manual quantity, then the lines no item took.
pub fn audit_csv(r: &EstimateResult) -> String {
    let mut out = csv::row(&["item", "kind", "line", "element_id", "mark", "category", "line_quantity", "line_unit", "rule", "factor", "quantity", "reason"]);
    for m in &r.measured.items {
        for c in &m.contributions {
            out.push_str(&csv::row(&[
                m.item.code.as_str(),
                "line",
                &c.line.to_string(),
                &c.element.map(|e| e.0.to_string()).unwrap_or_default(),
                &c.mark,
                &c.category,
                &c.line_quantity.to_string(),
                &c.line_unit,
                &c.rule,
                &c.factor.to_string(),
                &c.quantity.to_string(),
                "",
            ]));
        }
        for a in &m.adjustments {
            out.push_str(&csv::row(&[m.item.code.as_str(), "adjustment", "", "", "", "", "", "", "", "", &a.delta.to_string(), &a.reason]));
        }
        if let (Some(q), crate::ItemKind::Manual { reason, .. }) = (m.manual, &m.item.kind) {
            out.push_str(&csv::row(&[m.item.code.as_str(), "manual", "", "", "", "", "", "", "", "", &q.to_string(), reason]));
        }
        out.push_str(&csv::row(&[m.item.code.as_str(), "quantity", "", "", "", "", "", "", "", "", &m.quantity.to_string(), ""]));
    }
    out.push_str(&unmapped_rows(r));
    out
}

fn unmapped_rows(r: &EstimateResult) -> String {
    let mut out = String::new();
    for u in &r.measured.unmapped {
        let why = match &u.reason {
            UnmappedReason::NoRule => "no rule matches".to_string(),
            UnmappedReason::UnitMismatch { rule, line_unit, item_unit } => format!("rule '{rule}' needs unit '{item_unit}' but the line is in '{line_unit}'"),
        };
        out.push_str(&csv::row(&[
            "",
            "unmapped",
            &u.line.id.to_string(),
            &u.line.element.map(|e| e.0.to_string()).unwrap_or_default(),
            &u.line.mark,
            &u.line.category,
            &u.line.quantity.to_string(),
            &u.line.unit,
            "",
            "",
            "",
            &why,
        ]));
    }
    out
}

/// Lines no item took, with the reason.
pub fn unmapped_csv(r: &EstimateResult) -> String {
    let mut out = csv::row(&["item", "kind", "line", "element_id", "mark", "category", "line_quantity", "line_unit", "rule", "factor", "quantity", "reason"]);
    out.push_str(&unmapped_rows(r));
    out
}

/// A comparison of two estimates, item by item, then totals and additions.
pub fn comparison_csv(c: &Comparison) -> String {
    let mut out = csv::row(&[
        "type", "code", "description", "unit", "status", "quantity_old", "quantity_new", "quantity_change", "amount_old", "amount_new", "amount_change", "percent_change",
    ]);
    let fmt = |d: Decimal| d.to_fixed(2);
    for i in &c.items {
        out.push_str(&csv::row(&[
            "item",
            &i.code,
            &i.description,
            &i.unit,
            &format!("{:?}", i.status),
            &i.quantity_old.to_string(),
            &i.quantity_new.to_string(),
            &i.quantity_change.to_string(),
            &fmt(i.amount_old),
            &fmt(i.amount_new),
            &fmt(i.amount_change),
            &i.percent_change.map(|p| p.to_string()).unwrap_or_default(),
        ]));
    }
    let tot = |kind: &str, code: &str, name: &str, o: Decimal, n: Decimal, ch: Decimal| {
        csv::row(&[kind, code, name, "", "", "", "", "", &fmt(o), &fmt(n), &fmt(ch), ""])
    };
    out.push_str(&tot("subtotal", "", "Bill total", c.subtotal_old, c.subtotal_new, c.subtotal_change));
    for a in &c.additions {
        out.push_str(&tot("addition", &a.id, &a.name, a.amount_old, a.amount_new, a.amount_change));
    }
    out.push_str(&tot("grand_total", "", "Total", c.grand_old, c.grand_new, c.grand_change));
    out
}

/// The cash flow by month with the S-curve columns.
pub fn cash_flow_csv(cf: &CashFlow, decimals: u32) -> String {
    let mut out = csv::row(&["year", "month", &format!("amount_{}", cf.currency), "cumulative", "cumulative_percent"]);
    for m in &cf.months {
        out.push_str(&csv::row(&[&m.year.to_string(), &format!("{:02}", m.month), &amount(m.amount, decimals), &amount(m.cumulative, decimals), &m.cumulative_percent.to_fixed(2)]));
    }
    out.push_str(&csv::row(&["phased total", "", &amount(cf.phased_total, decimals), "", ""]));
    out.push_str(&csv::row(&["not phased", "", &amount(cf.unphased_total, decimals), "", ""]));
    out.push_str(&csv::row(&["grand total", "", &amount(cf.grand_total, decimals), "", ""]));
    out
}

/// An interim certificate: the items, then the rules, then the net amounts.
pub fn certificate_csv(c: &Certificate) -> String {
    let d = c.decimals;
    let mut out = csv::row(&[
        "type", "code", "description", "unit", "rate", "quantity_to_date", "quantity_previous", "quantity_this_period", "amount_to_date", "amount_previous", "amount_this_period",
    ]);
    for l in &c.lines {
        out.push_str(&csv::row(&[
            "item",
            &l.code,
            &l.description,
            &l.unit,
            &l.rate.to_string(),
            &l.quantity_to_date.to_string(),
            &l.quantity_previous.to_string(),
            &l.quantity_this_period.to_string(),
            &amount(l.amount_to_date, d),
            &amount(l.amount_previous, d),
            &amount(l.amount_this_period, d),
        ]));
    }
    let row3 = |kind: &str, name: &str, to_date: Decimal, prev: Decimal, period: Decimal| {
        csv::row(&[kind, "", name, "", "", "", "", "", &amount(to_date, d), &amount(prev, d), &amount(period, d)])
    };
    out.push_str(&row3("gross", "Gross value of work", c.gross_to_date, c.gross_previous, c.gross_this_period));
    for r in &c.rules {
        let kind = if r.effect == crate::Effect::Add { "add" } else { "deduct" };
        out.push_str(&row3(kind, &r.name, r.to_date, r.previous, r.this_period));
    }
    out.push_str(&row3("net", "Net amount", c.net_to_date, c.net_to_date - c.net_this_period, c.net_this_period));
    out
}
