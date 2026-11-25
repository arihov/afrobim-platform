// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Default engineering values with citations.
//!
//! A default is a pre-filled starting value for a field. It is never a requirement of the
//! project: the engineer confirms or replaces it. Every value in a pack carries the document,
//! revision, publication date, clause or table, page, condition, status and URL it was read
//! from, so a dialog can show the source beside the field.
//!
//! Packs are JSON files embedded with `include_str!`. Only the DMRB pack is populated
//! (`Defaults::dmrb()`). Other bases (MoWT Uganda, SATCC, TRH) are further packs implementing
//! [`DefaultsSet`]; the project chooses its basis with [`Defaults::set_basis`].
//!
//! **No value is held that was not read from the published document.** Keys the application
//! asks for that the pack does not hold are listed by `missing()`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

pub mod review;
mod wanted;

pub use wanted::WANTED;

/// The embedded DMRB pack.
pub const DMRB_JSON: &str = include_str!("../data/dmrb.json");

#[derive(Debug, thiserror::Error)]
pub enum DefaultsError {
    #[error("pack JSON could not be read: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown basis '{0}'")]
    UnknownBasis(String),
}

/// What kind of statement the document makes. The document's own verb is kept in `verb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Requirement,
    DesirableMinimum,
    DesirableMaximum,
    AbsoluteMinimum,
    /// A value below the desirable minimum that is permitted only as an approved relaxation.
    Relaxation,
    Advice,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Requirement => "requirement",
            Status::DesirableMinimum => "desirable minimum",
            Status::DesirableMaximum => "desirable maximum",
            Status::AbsoluteMinimum => "absolute minimum",
            Status::Relaxation => "relaxation",
            Status::Advice => "advice",
        }
    }
}

/// The fixed list of units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    #[serde(rename = "m")]
    Metre,
    #[serde(rename = "mm")]
    Millimetre,
    #[serde(rename = "%")]
    Percent,
    #[serde(rename = "km/h")]
    KmPerHour,
    #[serde(rename = "m/s")]
    MetrePerSecond,
    #[serde(rename = "m/s3")]
    MetrePerSecondCubed,
    #[serde(rename = "deg")]
    Degree,
    #[serde(rename = "years")]
    Year,
    /// Vertical curve K: metres of curve length per percent change of gradient.
    #[serde(rename = "m/%")]
    MetrePerPercent,
    /// Ratio written 1:n or 1 in n; the stored number is n.
    #[serde(rename = "1:n")]
    OneIn,
    #[serde(rename = "m2")]
    SquareMetre,
    #[serde(rename = "ha")]
    Hectare,
    #[serde(rename = "ratio")]
    Ratio,
    /// Manning's n.
    #[serde(rename = "s/m^(1/3)")]
    ManningN,
    #[serde(rename = "none")]
    None,
    /// Only valid as the unit of a table dimension that is a text label.
    #[serde(rename = "label")]
    Label,
}

impl Unit {
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Metre => "m",
            Unit::Millimetre => "mm",
            Unit::Percent => "%",
            Unit::KmPerHour => "km/h",
            Unit::MetrePerSecond => "m/s",
            Unit::MetrePerSecondCubed => "m/s3",
            Unit::Degree => "deg",
            Unit::Year => "years",
            Unit::MetrePerPercent => "m/%",
            Unit::OneIn => "1:n",
            Unit::SquareMetre => "m2",
            Unit::Hectare => "ha",
            Unit::Ratio => "ratio",
            Unit::ManningN => "s/m^(1/3)",
            Unit::None => "",
            Unit::Label => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Scalar,
    Table,
    Formula,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lookup {
    /// Only the listed dimension values have a default.
    Exact,
    /// The document allows interpolation between listed values (no extrapolation).
    Linear,
}

/// Direction the physics says a table must move as the numeric dimension rises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Monotonic {
    None,
    IncreasingWithDimension,
    NondecreasingWithDimension,
    DecreasingWithDimension,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum At {
    Num(f64),
    Label(String),
}

impl At {
    pub fn show(&self) -> String {
        match self {
            At::Num(n) => fmt_num(*n),
            At::Label(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub at: At,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub name: String,
    pub unit: Unit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub dimension: Dimension,
    pub lookup: Lookup,
    pub monotonic: Monotonic,
    pub rows: Vec<Row>,
}

/// The publication a value was read from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub reference: String,
    pub title: String,
    pub version: String,
    /// Publication date as printed on the document footer.
    pub published: String,
    pub url: String,
    pub copyright: String,
    /// National application annex differences noticed, or the statement that none apply.
    pub annex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub group: String,
    pub title: String,
    pub kind: Kind,
    pub unit: Unit,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub table: Option<Table>,
    #[serde(default)]
    pub text: Option<String>,
    /// Key into the pack's document records.
    pub document: String,
    pub clause: String,
    /// Printed page number.
    pub page: String,
    /// Page index in the PDF file.
    pub pdf_page: u32,
    /// The exact condition the value applies to.
    pub condition: String,
    pub status: Status,
    /// The document's own verb, for example "shall", "should", "table value".
    pub verb: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub national_annex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Licence {
    pub name: String,
    pub printed_notice: String,
    pub website_notice: String,
    pub attribution: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackMeta {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub data_version: u32,
    pub read_on: String,
    pub read_by: String,
    pub review_status: String,
    pub uganda_caveat: String,
    pub licence: Licence,
}

#[derive(Debug, Clone, Deserialize)]
struct PackFile {
    schema_version: u32,
    pack: PackMeta,
    documents: BTreeMap<String, DocumentRecord>,
    entries: Vec<Entry>,
}

/// Full citation of one default.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Citation {
    pub pack_id: String,
    pub document_reference: String,
    pub document_title: String,
    pub version: String,
    pub published: String,
    pub clause: String,
    pub page: String,
    pub pdf_page: u32,
    pub condition: String,
    pub status: Status,
    pub verb: String,
    pub national_annex: Option<String>,
    pub document_annex: String,
    pub url: String,
    pub read_on: String,
}

impl Citation {
    /// One line for display beside a field.
    pub fn one_line(&self) -> String {
        format!(
            "{} {} v{} ({}), {}, p{}: {} ({})",
            self.pack_id.to_uppercase(),
            self.document_reference,
            self.version,
            self.published,
            self.clause,
            self.page,
            self.status.label(),
            self.verb
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Resolved {
    Number(f64),
    Formula(String),
    Text(String),
}

/// A default with its citation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DefaultValue {
    pub key: String,
    pub title: String,
    pub value: Resolved,
    pub unit: Unit,
    pub status: Status,
    /// True when the number was interpolated between two document values (only where the
    /// document allows it).
    pub interpolated: bool,
    pub citation: Citation,
}

impl DefaultValue {
    pub fn number(&self) -> Option<f64> {
        match self.value {
            Resolved::Number(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    Num(f64),
    Label(String),
}

/// The conditions a lookup is made under: design speed, road type, pipe diameter and so on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conditions(BTreeMap<String, Cond>);

impl Conditions {
    pub fn none() -> Self {
        Conditions(BTreeMap::new())
    }
    pub fn num(mut self, name: &str, v: f64) -> Self {
        self.0.insert(name.to_string(), Cond::Num(v));
        self
    }
    pub fn label(mut self, name: &str, v: &str) -> Self {
        self.0.insert(name.to_string(), Cond::Label(v.to_string()));
        self
    }
    pub fn design_speed(self, kmh: f64) -> Self {
        self.num("design_speed_kmh", kmh)
    }
}

/// A key the application asked for that the pack does not hold.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Missing {
    pub key: String,
    pub reason: String,
}

/// A set of default values on one standards basis.
pub trait DefaultsSet {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn meta(&self) -> &PackMeta;
    fn get(&self, key: &str, cond: &Conditions) -> Option<DefaultValue>;
    fn cite(&self, key: &str) -> Option<String>;
    fn all(&self) -> Vec<&Entry>;
    fn missing(&self) -> Vec<Missing>;
}

/// A data pack read from JSON.
pub struct Pack {
    meta: PackMeta,
    documents: BTreeMap<String, DocumentRecord>,
    entries: Vec<Entry>,
    index: BTreeMap<String, usize>,
    asked_unknown: Mutex<BTreeSet<String>>,
}

impl Pack {
    pub fn from_json(json: &str) -> Result<Pack, DefaultsError> {
        let f: PackFile = serde_json::from_str(json)?;
        let _ = f.schema_version;
        let index = f.entries.iter().enumerate().map(|(i, e)| (e.key.clone(), i)).collect();
        Ok(Pack { meta: f.pack, documents: f.documents, entries: f.entries, index, asked_unknown: Mutex::new(BTreeSet::new()) })
    }

    pub fn dmrb() -> Pack {
        Pack::from_json(DMRB_JSON).expect("embedded DMRB pack is valid; the self-test checks it")
    }

    pub fn documents(&self) -> &BTreeMap<String, DocumentRecord> {
        &self.documents
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.index.get(key).map(|&i| &self.entries[i])
    }

    pub fn citation(&self, key: &str) -> Option<Citation> {
        let e = self.entry(key)?;
        let d = self.documents.get(&e.document)?;
        Some(Citation {
            pack_id: self.meta.id.clone(),
            document_reference: d.reference.clone(),
            document_title: d.title.clone(),
            version: d.version.clone(),
            published: d.published.clone(),
            clause: e.clause.clone(),
            page: e.page.clone(),
            pdf_page: e.pdf_page,
            condition: e.condition.clone(),
            status: e.status,
            verb: e.verb.clone(),
            national_annex: e.national_annex.clone(),
            document_annex: d.annex.clone(),
            url: d.url.clone(),
            read_on: self.meta.read_on.clone(),
        })
    }

    /// Keys asked for with `get` or `cite` that the pack does not hold.
    fn note_asked(&self, key: &str) {
        if let Ok(mut s) = self.asked_unknown.lock() {
            s.insert(key.to_string());
        }
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

impl DefaultsSet for Pack {
    fn id(&self) -> &str {
        &self.meta.id
    }
    fn name(&self) -> &str {
        &self.meta.name
    }
    fn meta(&self) -> &PackMeta {
        &self.meta
    }

    fn get(&self, key: &str, cond: &Conditions) -> Option<DefaultValue> {
        let Some(e) = self.entry(key) else {
            self.note_asked(key);
            return None;
        };
        let citation = self.citation(key)?;
        let mk = |value: Resolved, interpolated: bool| DefaultValue {
            key: e.key.clone(),
            title: e.title.clone(),
            value,
            unit: e.unit,
            status: e.status,
            interpolated,
            citation: citation.clone(),
        };
        match e.kind {
            Kind::Scalar => e.value.map(|v| mk(Resolved::Number(v), false)),
            Kind::Formula => e.text.clone().map(|t| mk(Resolved::Formula(t), false)),
            Kind::Text => e.text.clone().map(|t| mk(Resolved::Text(t), false)),
            Kind::Table => {
                let t = e.table.as_ref()?;
                let c = cond.0.get(&t.dimension.name)?;
                match c {
                    Cond::Label(l) => t
                        .rows
                        .iter()
                        .find(|r| matches!(&r.at, At::Label(x) if x == l))
                        .map(|r| mk(Resolved::Number(r.value), false)),
                    Cond::Num(x) => {
                        let pts: Vec<(f64, f64)> = t.rows.iter().filter_map(|r| if let At::Num(a) = r.at { Some((a, r.value)) } else { None }).collect();
                        if let Some(&(_, v)) = pts.iter().find(|(a, _)| close(*a, *x)) {
                            return Some(mk(Resolved::Number(v), false));
                        }
                        if t.lookup == Lookup::Linear {
                            let mut sorted = pts.clone();
                            sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                            for w in sorted.windows(2) {
                                let ((a0, v0), (a1, v1)) = (w[0], w[1]);
                                if *x > a0 && *x < a1 {
                                    let f = (*x - a0) / (a1 - a0);
                                    return Some(mk(Resolved::Number(v0 + f * (v1 - v0)), true));
                                }
                            }
                        }
                        None
                    }
                }
            }
        }
    }

    fn cite(&self, key: &str) -> Option<String> {
        match self.citation(key) {
            Some(c) => Some(c.one_line()),
            None => {
                self.note_asked(key);
                None
            }
        }
    }

    fn all(&self) -> Vec<&Entry> {
        self.entries.iter().collect()
    }

    fn missing(&self) -> Vec<Missing> {
        let mut out: BTreeSet<Missing> = BTreeSet::new();
        for (key, reason) in WANTED {
            if !self.index.contains_key(*key) {
                out.insert(Missing { key: key.to_string(), reason: reason.to_string() });
            }
        }
        if let Ok(asked) = self.asked_unknown.lock() {
            for k in asked.iter() {
                if !self.index.contains_key(k) {
                    out.insert(Missing { key: k.clone(), reason: "asked for by the application; not in this pack".to_string() });
                }
            }
        }
        out.into_iter().collect()
    }
}

/// The defaults the project works with: the available packs and the chosen basis.
pub struct Defaults {
    sets: Vec<Box<dyn DefaultsSet>>,
    basis: usize,
}

impl Defaults {
    /// Only the DMRB pack, chosen as the basis.
    pub fn dmrb() -> Defaults {
        Defaults { sets: vec![Box::new(Pack::dmrb())], basis: 0 }
    }

    /// No packs: every lookup returns `None` (a project with no basis chosen).
    pub fn empty() -> Defaults {
        Defaults { sets: Vec::new(), basis: 0 }
    }

    /// Add another standards pack (for example a MoWT Uganda pack read from JSON).
    pub fn add_set(&mut self, set: Box<dyn DefaultsSet>) {
        self.sets.push(set);
    }

    /// The ids of the packs available, for a basis menu.
    pub fn bases(&self) -> Vec<(&str, &str)> {
        self.sets.iter().map(|s| (s.id(), s.name())).collect()
    }

    pub fn basis_id(&self) -> Option<&str> {
        self.sets.get(self.basis).map(|s| s.id())
    }

    pub fn set_basis(&mut self, id: &str) -> Result<(), DefaultsError> {
        match self.sets.iter().position(|s| s.id() == id) {
            Some(i) => {
                self.basis = i;
                Ok(())
            }
            None => Err(DefaultsError::UnknownBasis(id.to_string())),
        }
    }

    fn active(&self) -> Option<&dyn DefaultsSet> {
        self.sets.get(self.basis).map(|b| b.as_ref())
    }

    pub fn active_set(&self) -> Option<&dyn DefaultsSet> {
        self.active()
    }

    /// The default for `key` under `cond`, with its citation.
    pub fn get(&self, key: &str, cond: &Conditions) -> Option<DefaultValue> {
        self.active()?.get(key, cond)
    }

    /// A one-line citation for display beside a field.
    pub fn cite(&self, key: &str) -> Option<String> {
        self.active()?.cite(key)
    }

    /// Every entry of the chosen basis, for a browsable table.
    pub fn all(&self) -> Vec<&Entry> {
        self.active().map(|s| s.all()).unwrap_or_default()
    }

    /// Keys the application asked for that the chosen basis does not hold.
    pub fn missing(&self) -> Vec<Missing> {
        match self.active() {
            Some(s) => s.missing(),
            None => WANTED.iter().map(|(k, r)| Missing { key: k.to_string(), reason: r.to_string() }).collect(),
        }
    }
}

/// Format a number without a trailing `.0`.
pub fn fmt_num(n: f64) -> String {
    if (n - n.round()).abs() < 1e-12 && n.abs() < 1e12 {
        format!("{}", n.round() as i64)
    } else {
        let s = format!("{:.6}", n);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests;
