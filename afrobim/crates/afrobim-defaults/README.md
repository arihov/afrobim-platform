# afrobim-defaults

Default engineering values for AfroBIM dialogs, each traceable to the clause it came from.

Until round 6 AfroBIM held no engineering value: every dimension was an empty required input.
The owner's decision for round 6 is to pre-fill fields with DMRB manual defaults. This crate
holds those values and **the rule that still holds without exception: no value from memory**.
A default exists only if the DMRB document was fetched, the table or clause was read from the
PDF, and the value is stored with its full citation.

**The values were read by an automated reader on 2026-11-16 and have not been reviewed by an
engineer.** `REVIEW.md` lists every number beside its source so a highway engineer can check it
line by line. Treat the whole pack as unreviewed until that is done.

## What the pack is

`data/dmrb.json` (embedded with `include_str!`, `data_version` 1) holds:

* `documents`: the publication records (reference, title, version, publication date as printed
  on the footer, the URL fetched, the copyright year, national application annex notes);
* `entries`: one per default. Each has a stable key (`link.ssd.desirable_min`), a value (a
  number, a table looked up by a dimension such as design speed or road type, a formula as the
  document gives it, or a recorded text fact), a unit from a fixed list, the exact condition
  it applies to, the status, the document's own verb ("shall", "should", "table value"), the
  clause or table, the printed page and the PDF page index.

Rust API (`afrobim_defaults`):

```rust
let d = Defaults::dmrb();                       // the DMRB pack, chosen as the basis
let v = d.get("link.ssd.desirable_min", &Conditions::none().design_speed(100.0)); // Some(215 m) with citation
let line = d.cite("link.ssd.desirable_min");    // "DMRB CD 109 v1.1.0 (30-Oct-2026), Table 2.10 (clause 2.9), p17: desirable minimum (shall (cl. 2.9))"
let rows = d.all();                             // every entry, for a browsable table
let gaps = d.missing();                         // keys the application wants that the basis does not hold, with the reason
```

`DefaultsSet` is the trait a standards pack implements (`get`, `cite`, `all`, `missing`).
`Pack::from_json` reads any pack file in the same schema, so MoWT Uganda, SATCC or TRH packs are
further JSON files added with `Defaults::add_set`; the project picks its basis with
`Defaults::set_basis`. Only the DMRB pack is populated.

Statuses: `requirement`, `desirable_minimum`, `desirable_maximum`, `absolute_minimum`, `advice`,
and `relaxation` (a value below the desirable minimum that DMRB permits only as an approved
relaxation, see CD 109 clauses 2.9 to 2.13). Tables are looked up exactly. Interpolation happens
only where the document allows it (marked `linear` in the data) and is flagged on the result.

## A default is not a requirement

A default is a starting value. The project, the client's brief and the engineer decide the
design. Even a value whose status is `requirement` is a requirement of the DMRB, not of the
project: it binds a design only where the project adopts DMRB as its basis. The dialog must
show the citation beside the field and let the engineer overwrite it. Values below the desirable
minimum (status `relaxation`) need an approved relaxation or departure under GG 101, which
this crate does not encode (GG 101 was not fetched).

## Uganda context caveat

**DMRB is a UK standard. Its values assume UK traffic, vehicles, climate and practice.** Lane
widths, design speeds, return periods, climate change allowances and pavement values do not
automatically fit Uganda. For work under the MoWT Uganda manuals, the project should switch basis
when a MoWT pack exists. Until then the DMRB defaults are a starting point that the engineer
must confirm. Where a document has national application annexes, England values are used (or
the core document, which is common to all nations); the annex differences noticed are recorded
per document in the data.

## Licence and attribution

The documents carry this notice on their last page (the year printed varies by document, see
the data): "© Crown copyright. You may re-use this information (not including logos) free of
charge in any format or medium, under the terms of the Open Government Licence. To view this
licence: visit www.nationalarchives.gov.uk/doc/open-government-licence/, write to the
Information Policy Team, The National Archives, Kew, London TW9 4DU, or email
psi@nationalarchives.gsi.gov.uk." The standardsforhighways.co.uk terms page says the same
("Crown copyright (c) Crown copyright, 2025 ... free of charge in any format or medium, under the
terms of the Open Government Licence").

The fetched documents print no separate attribution sentence. The acknowledgement AfroBIM uses
is its own wording, not the publisher's: "Contains information from the Design Manual for Roads
and Bridges, (c) Crown copyright, used under the Open Government Licence." The Open Government
Licence text itself was not fetched in this session, so its attribution terms should be
confirmed before the product is distributed. The PDFs are kept outside the repository and are
not committed; only the values and citations are.

## What is in the pack

Documents read (all fetched 2026-11-16 from standardsforhighways.co.uk; version and publication
date as printed): CD 109 v1.1.0 (30-Oct-2026), CD 127 v1.0.1 (29-Jul-2021), CD 123 v2.1.0
(29-Nov-2021), CD 116 v2.1.0 (31-May-2023), CG 501 v2.2.0 (30-Sep-2025), CD 521 v1.2.0
(30-Sep-2025), CD 522 v0.1.0 (30-Sep-2025), CD 523 v1.1.1 (09-Oct-2026), CD 529 v1.1.0
(30-Sep-2025), CD 377 v5.0.1 (27-Feb-2026), CD 622 v2.0.0 (30-Sep-2025), MCHW CC 601 v1.0.0
(30-Sep-2025).

Groups: link design (CD 109 Table 2.10, superelevation, transitions, gradients, widening, sight
heights), cross-sections and headrooms (CD 127), junctions (CD 123), roundabouts (CD 116),
drainage (CG 501, CD 521, CD 522, CD 523, CD 529), restraint (CD 377 two tables), earthworks (two
recorded facts that no number is given). Cross-section widths in CD 127 are read from figures
(images in the PDF), so they were read visually and are the entries most worth a second look.

## What is missing

`Defaults::missing()` lists these with reasons (also at the end of `REVIEW.md`):

* CD 127 connector road cross-sections, VRS set-back (Table 2.24), and verge or berm widths the
  figures show as "Varies".
* CD 123 signal-controlled junction geometry and the stagger distance table 2.25 (located, not stored).
* CD 116 mini-roundabout tables.
* Drainage: minimum carrier pipe diameter, minimum pipe cover, maximum velocity and chamber
  spacing were not found in CG 501, CD 521, CD 522, CD 523, CD 527, CD 529 or CC 500 by keyword
  search (a keyword search is not exhaustive). CD 526 gully spacing was fetched but not read in;
  CD 527 sumpless gully velocity tables were read but not stored.
* Earthworks side slope angle: DMRB gives none (CD 622 Appendix D leaves side slopes to the
  geotechnical assessment). Earthworks surface tolerance: CC 601 clause 1.10 defers to a project
  document (CC 601/WSR/001).
* Survey and setting-out tolerances: no DMRB or MCHW Volume 1 clause tabulating them was found.
* CD 226 pavement values (not fetched; AfroBIM's pavement design uses CD 224 and CD 226
  equations from superseded revisions that should be refreshed) and lighting values.
* GG 101 (relaxation and departure procedure) was not fetched; Figure 2.1 (design speed band A
  or B) is a chart and was not stored.

## Self-test

`cargo test -p afrobim-defaults` checks that every entry has all citation fields, units come
from the fixed list, tables are sorted and monotonic where the physics says so (sight distance,
radius and K rise with design speed; each Table 2.10 radius row equals the row above shifted one
speed step; V^2/R is constant along each row), lookups and interpolation behave, `missing()`
reports the gaps, a second pack can be added and chosen as basis, `REVIEW.md` matches the data,
and nothing contains an em dash. `cargo test -p afrobim-defaults -- --nocapture prints_the_whole_pack_as_a_review_table`
prints the review table. Regenerate `REVIEW.md` with
`cargo run -p afrobim-defaults --example write_review`.
