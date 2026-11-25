// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Status and synchronise.
//!
//! A reference follows the *published* state of its shortcut: the hash and revision in the
//! manifest. A source that has been edited but not published is not followed (status says so in
//! `unpublished_changes`, and synchronise skips it), so every host only ever sees states somebody
//! chose to share.

use afrobim_kernel::{project_crs, CrsCheck, Document, ElementId, FindingKind, IncomingDataset, ProjectCrs};
use serde::{Deserialize, Serialize};

use crate::canon::element_hash;
use crate::deps::key_closure;
use crate::diff::{summarise, ChangeSummary};
use crate::manifest::FileStamp;
use crate::paths::resolve;
use crate::project::{DocCache, ShortcutProject};
use crate::reference::{host_copy, reference_index, reference_info, store_info, ReferenceOptions};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RefState {
    UpToDate,
    /// The shortcut was published again since this reference was synchronised.
    OutOfDate { old_revision: u64, new_revision: u64, old_hash: String, new_hash: String },
    /// The source file or the element in it cannot be found.
    SourceMissing { what: String },
    /// The shortcut is no longer in the manifest. The copy stays as it is.
    ShortcutRemoved,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceStatus {
    pub element: ElementId,
    pub key: String,
    pub name: String,
    pub state: RefState,
    /// The source declares a coordinate system that differs from the host's. Nothing is transformed.
    pub crs_differs: bool,
    /// Plain sentences about the coordinate systems, empty when there is nothing to say.
    pub crs_messages: Vec<String>,
    /// The source file holds edits that were not published (only checked when the file stamp changed).
    pub unpublished_changes: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct StatusOptions {
    /// Look at the source files (existence, then the stamp, then the content if the stamp
    /// changed). Off: the manifest alone is read, nothing else is opened.
    pub check_sources: bool,
}

impl Default for StatusOptions {
    fn default() -> Self {
        StatusOptions { check_sources: true }
    }
}

/// Compares the coordinate system a source declares with the host's, using the kernel's check.
/// Returns whether they differ and the warnings, as sentences. Nothing is transformed.
pub(crate) fn crs_findings(host: &Option<ProjectCrs>, source: &Option<ProjectCrs>, label: &str) -> (bool, Vec<String>) {
    match (host, source) {
        (Some(h), s) => {
            let check = CrsCheck::run(h, &IncomingDataset { label: label.to_string(), crs: s.clone(), extent: None });
            let msgs = check.findings.iter().filter(|f| f.warning).map(|f| f.message.clone()).collect();
            (check.has(FindingKind::DifferentSystem), msgs)
        }
        (None, Some(s)) => (false, vec![format!("{label} is in {}, but this document does not declare a coordinate system, so it cannot be checked. Nothing is transformed.", s.label())]),
        (None, None) => (false, Vec::new()),
    }
}

/// The status of every reference in the host.
pub fn status(host: &Document, project: &ShortcutProject, opts: &StatusOptions) -> Vec<ReferenceStatus> {
    let host_crs = project_crs(host);
    let mut cache = DocCache::new(project.root());
    let mut out = Vec::new();
    for e in host.elements() {
        let Some(info) = reference_info(e) else { continue };
        let mut row = ReferenceStatus { element: e.id, key: info.shortcut_key.clone(), name: e.name.clone(), state: RefState::UpToDate, crs_differs: false, crs_messages: Vec::new(), unpublished_changes: false };
        let Some(sc) = project.get(&info.shortcut_key) else {
            row.state = RefState::ShortcutRemoved;
            let (d, m) = crs_findings(&host_crs, &info.source_crs, &e.name);
            row.crs_differs = d;
            row.crs_messages = m;
            out.push(row);
            continue;
        };
        let (d, m) = crs_findings(&host_crs, &sc.source_crs, &e.name);
        row.crs_differs = d;
        row.crs_messages = m;
        let mut missing: Option<String> = None;
        if opts.check_sources {
            let file = resolve(project.root(), &sc.source_path);
            if !file.exists() {
                missing = Some(format!("the file {} does not exist", sc.source_path));
            } else if FileStamp::of(&file).ok() != sc.file_stamp {
                match cache.get(&sc.source_path) {
                    Err(err) => missing = Some(format!("{} cannot be read: {err}", sc.source_path)),
                    Ok(doc) => match doc.find(ElementId(sc.source_element_id)) {
                        None => missing = Some(format!("element {} is no longer in {}", sc.source_element_id, sc.source_path)),
                        Some(el) => row.unpublished_changes = element_hash(el) != sc.hash,
                    },
                }
            }
        }
        row.state = if let Some(what) = missing {
            RefState::SourceMissing { what }
        } else if sc.hash != info.source_hash {
            RefState::OutOfDate { old_revision: info.source_revision, new_revision: sc.revision, old_hash: info.source_hash.clone(), new_hash: sc.hash.clone() }
        } else {
            RefState::UpToDate
        };
        out.push(row);
    }
    out
}

/// Which references a synchronise takes.
#[derive(Debug, Clone)]
pub enum SyncTarget {
    All,
    Keys(Vec<String>),
    Elements(Vec<ElementId>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    AlreadyUpToDate,
    Refreshed { old_revision: u64, new_revision: u64, changes: ChangeSummary },
    /// Not touched, and why.
    Skipped { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncItem {
    pub element: ElementId,
    pub key: String,
    pub outcome: SyncOutcome,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncReport {
    pub items: Vec<SyncItem>,
}

impl SyncReport {
    /// The host elements whose data or placement changed. The application must rebuild what it
    /// derived from them (drawings, quantities, cached scenes).
    pub fn changed_elements(&self) -> Vec<ElementId> {
        self.items.iter().filter(|i| matches!(i.outcome, SyncOutcome::Refreshed { .. })).map(|i| i.element).collect()
    }
}

/// Refreshes the copies of the chosen references from the published state of their shortcuts.
///
/// A reference is refreshed only when its shortcut was published again, and only if the source
/// file still holds exactly what was published. The source file is only read. Locked references
/// are refreshed (that is what the lock is for); the lock stays on.
pub fn synchronise(host: &mut Document, project: &ShortcutProject, which: &SyncTarget, opts: &ReferenceOptions) -> SyncReport {
    let mut cache = DocCache::new(project.root());
    let targets: Vec<ElementId> = host
        .elements()
        .iter()
        .filter_map(|e| reference_info(e).map(|i| (e.id, i)))
        .filter(|(id, i)| match which {
            SyncTarget::All => true,
            SyncTarget::Keys(k) => k.contains(&i.shortcut_key),
            SyncTarget::Elements(ids) => ids.contains(id),
        })
        .map(|(id, _)| id)
        .collect();
    let mut report = SyncReport::default();
    for id in targets {
        let old = host.find(id).expect("listed above").clone();
        let mut info = reference_info(&old).expect("listed above");
        let key = info.shortcut_key.clone();
        let item = |outcome| SyncItem { element: id, key: key.clone(), outcome };
        let skip = |reason: String| SyncOutcome::Skipped { reason };
        let Some(sc) = project.get(&key).cloned() else {
            report.items.push(item(skip("the shortcut was removed from the project; the copy is kept as it is".into())));
            continue;
        };
        if sc.hash == info.source_hash && sc.revision == info.source_revision {
            report.items.push(item(SyncOutcome::AlreadyUpToDate));
            continue;
        }
        let src = match cache.get(&sc.source_path) {
            Err(e) => {
                report.items.push(item(skip(format!("the source cannot be read: {e}"))));
                continue;
            }
            Ok(d) => match d.find(ElementId(sc.source_element_id)) {
                None => {
                    report.items.push(item(skip(format!("element {} is not in {}", sc.source_element_id, sc.source_path))));
                    continue;
                }
                Some(e) => e.clone(),
            },
        };
        if element_hash(&src) != sc.hash {
            report.items.push(item(skip(format!("'{}' in {} has changed since it was published. Publish it from its own project, then synchronise.", sc.name, sc.source_path))));
            continue;
        }
        let closure = key_closure(project, &mut cache, &key, &opts.table);
        let empty = std::collections::BTreeMap::new();
        let local_map = closure.local_maps.get(&key).unwrap_or(&empty);
        let index = reference_index(host);
        let (copy, missing) = host_copy(&src, local_map, &index, opts);
        host.ensure_layer(&copy.layer);
        {
            let e = host.find_mut(id).expect("listed above");
            e.name = copy.name;
            e.category = copy.category;
            e.origin = copy.origin;
            e.width = copy.width;
            e.height = copy.height;
            e.site = copy.site;
            e.data = copy.data;
            e.layer = copy.layer;
            e.properties = copy.properties;
            let old_revision = info.source_revision;
            info.source_hash = sc.hash.clone();
            info.source_revision = sc.revision;
            info.source_path = sc.source_path.clone();
            info.source_element_id = sc.source_element_id;
            info.source_name = src.name.clone();
            info.source_category = src.category.clone();
            info.source_layer = src.layer.clone();
            info.source_crs = sc.source_crs.clone();
            info.synced_by = opts.by.clone();
            info.synced_at = opts.at.clone();
            info.id_map = copy.id_map;
            store_info(e, &info);
            let new = e.clone();
            let changes = summarise(&old, &new, missing);
            report.items.push(item(SyncOutcome::Refreshed { old_revision, new_revision: sc.revision, changes }));
        }
    }
    report
}
