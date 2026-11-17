//! Cheap stale detection, for the application to call often (when a window gains focus, on a
//! timer, when a file watcher fires). Nothing here opens a document.

use std::collections::{BTreeMap, HashMap};

use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

use crate::manifest::{FileStamp, Manifest};
use crate::paths::normalised;
use crate::reference::reference_info;

/// What a host recorded about one reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedRef {
    pub key: String,
    pub element: ElementId,
    pub hash: String,
    pub revision: u64,
}

/// The references a host holds, with the hash and revision each recorded.
pub fn recorded_references(host: &Document) -> Vec<RecordedRef> {
    host.elements().iter().filter_map(|e| reference_info(e).map(|i| RecordedRef { key: i.shortcut_key, element: e.id, hash: i.source_hash, revision: i.source_revision })).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaleRef {
    pub key: String,
    pub element: ElementId,
    pub recorded_revision: u64,
    /// The manifest's revision, or `None` when the shortcut is no longer in it.
    pub current_revision: Option<u64>,
    pub recorded_hash: String,
    pub current_hash: Option<String>,
}

/// Which recorded references no longer match the manifest: published again, or removed.
/// Work is one hash-map lookup per reference.
pub fn stale(manifest: &Manifest, recorded: &[RecordedRef]) -> Vec<StaleRef> {
    let by_key: HashMap<&str, &crate::manifest::Shortcut> = manifest.shortcuts.iter().map(|s| (s.key.as_str(), s)).collect();
    recorded
        .iter()
        .filter_map(|r| match by_key.get(r.key.as_str()) {
            Some(s) if s.hash == r.hash => None,
            Some(s) => Some(StaleRef { key: r.key.clone(), element: r.element, recorded_revision: r.revision, current_revision: Some(s.revision), recorded_hash: r.hash.clone(), current_hash: Some(s.hash.clone()) }),
            None => Some(StaleRef { key: r.key.clone(), element: r.element, recorded_revision: r.revision, current_revision: None, recorded_hash: r.hash.clone(), current_hash: None }),
        })
        .collect()
}

/// The keys whose source file may have changed since it was published: its stamp in `current`
/// (keyed by the source path as the manifest stores it, or any spelling of it) differs from the
/// stamp recorded at publish, or there is no stamp for it. A key is not listed when the stamps are
/// equal. The caller then opens only those files (for example with [`crate::ShortcutProject::validate`]).
pub fn changed_sources(manifest: &Manifest, root: &std::path::Path, current: &BTreeMap<String, FileStamp>) -> Vec<String> {
    let current: HashMap<String, FileStamp> = current.iter().map(|(k, v)| (normalised(root, k), *v)).collect();
    manifest
        .shortcuts
        .iter()
        .filter(|s| match (current.get(&normalised(root, &s.source_path)), s.file_stamp) {
            (Some(now), Some(then)) => *now != then,
            _ => true,
        })
        .map(|s| s.key.clone())
        .collect()
}
