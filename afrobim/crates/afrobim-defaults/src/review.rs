// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The review table: every number in a pack beside its source, for a highway engineer to check
//! line by line against the documents.

use crate::{fmt_num, DefaultsSet, Entry, Kind, Pack};
use std::collections::BTreeMap;

/// The value of an entry as one display string (tables as `at = value; ...`).
pub fn value_text(e: &Entry) -> String {
    match e.kind {
        Kind::Scalar => e.value.map(fmt_num).unwrap_or_default(),
        Kind::Table => {
            let t = e.table.as_ref().expect("table entry has a table");
            let rows: Vec<String> = t.rows.iter().map(|r| format!("{} = {}", r.at.show(), fmt_num(r.value))).collect();
            format!("by {} [{}]: {}", t.dimension.name, t.dimension.unit.symbol(), rows.join("; "))
        }
        Kind::Formula | Kind::Text => e.text.clone().unwrap_or_default(),
    }
}

fn esc(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Plain text rows, one per entry, for the printed self-test review.
pub fn review_plain(pack: &Pack) -> String {
    let mut out = String::new();
    for e in pack.all() {
        let d = &pack.documents()[&e.document];
        out.push_str(&format!(
            "{:<62} | {} {} | {} | {} | {} v{} {} cl {} p{}\n",
            e.key,
            value_text(e),
            e.unit.symbol(),
            e.status.label(),
            e.verb,
            d.reference,
            d.version,
            d.published,
            e.clause,
            e.page
        ));
    }
    out
}

/// The review table as Markdown (the content of REVIEW.md).
pub fn review_markdown(pack: &Pack) -> String {
    let m = pack.meta();
    let mut out = String::new();
    out.push_str("# Review table: DMRB default values\n\n");
    out.push_str(&format!(
        "**These values were read by an automated reader from the PDFs named below on {} and must be reviewed by a highway engineer before the product relies on them.** Review status: {}\n\n",
        m.read_on, m.review_status
    ));
    out.push_str(&format!("{}\n\n", m.uganda_caveat));
    out.push_str("This file is generated from `data/dmrb.json` (a test fails if it is out of date). Check each number against the cited clause or table. Page is the page number printed on the document; the PDF file index is `pdf` in the JSON. Status keeps the document's own verb.\n\n");
    out.push_str("## Documents\n\n| Reference | Title | Version | Published (as printed) | URL fetched | National annex notes |\n|---|---|---|---|---|---|\n");
    for d in pack.documents().values() {
        out.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", esc(&d.reference), esc(&d.title), esc(&d.version), esc(&d.published), esc(&d.url), esc(&d.annex)));
    }
    let mut groups: BTreeMap<&str, Vec<&Entry>> = BTreeMap::new();
    for e in pack.all() {
        groups.entry(e.group.as_str()).or_default().push(e);
    }
    out.push_str(&format!("\n## Values ({} entries)\n", pack.all().len()));
    for (g, es) in &groups {
        out.push_str(&format!("\n### {} ({} entries)\n\n| Key | Value | Unit | Status | Source | Condition |\n|---|---|---|---|---|---|\n", g, es.len()));
        for e in es {
            let d = &pack.documents()[&e.document];
            let mut cond = esc(&e.condition);
            if !e.note.is_empty() {
                cond.push_str(&format!(" Note: {}", esc(&e.note)));
            }
            if let Some(na) = &e.national_annex {
                cond.push_str(&format!(" Annex: {}", esc(na)));
            }
            out.push_str(&format!(
                "| `{}` | {} | {} | {} ({}) | {} v{} ({}), {}, p{} | {} |\n",
                e.key,
                esc(&value_text(e)),
                e.unit.symbol(),
                e.status.label(),
                esc(&e.verb),
                d.reference,
                d.version,
                d.published,
                esc(&e.clause),
                e.page,
                cond
            ));
        }
    }
    out.push_str("\n## Keys the application asks for that this pack does not hold\n\n| Key | What was looked at, and why it is missing |\n|---|---|\n");
    for x in pack.missing() {
        out.push_str(&format!("| `{}` | {} |\n", x.key, esc(&x.reason)));
    }
    out.push_str("\n## Licence\n\n");
    out.push_str(&format!("Licence: {}.\n\nPrinted notice: {}\n\nWebsite notice: {}\n\nAttribution: {}\n", m.licence.name, m.licence.printed_notice, m.licence.website_notice, m.licence.attribution));
    out
}
