//! External references of whole documents: attach another project as an underlay.
//!
//! The list is kept in the host's project settings under [`DOCUMENT_REFERENCES_KEY`], so it is
//! saved with the host. The attached document is never copied into the host and never modified:
//! [`load_underlay`] reads it, as it is, at its own coordinates, for the application to draw.

use std::path::Path;

use afrobim_kernel::{project_crs, project_setting, set_project_setting, Document, ProjectCrs, PROJECT_SETTINGS_CATEGORY};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canon::value_hash;
use crate::diff::Bounds;
use crate::error::{Result, ShortcutError};
use crate::manifest::FileStamp;
use crate::paths::{resolve, to_stored};
use crate::sync::crs_findings;

pub const DOCUMENT_REFERENCES_KEY: &str = "document_references";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentReference {
    /// Name shown to the user; unique within the host.
    pub name: String,
    /// Relative to the project folder with forward slashes, or absolute when outside it.
    pub path: String,
    /// Content hash of the document when it was attached or last refreshed (see [`document_hash`]).
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<FileStamp>,
    /// Layers of the source to show. Empty: all of them.
    #[serde(default)]
    pub layers: Vec<String>,
    /// An underlay is never editable through the host; this flag records that the team wants it so.
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_crs: Option<ProjectCrs>,
    pub element_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extents: Option<Bounds>,
    pub attached_by: String,
    pub attached_at: String,
}

/// A hash of the content of a document: every element's content hash and layer, and the standard.
/// Independent of how the file is formatted, of element order and of project settings.
pub fn document_hash(doc: &Document) -> String {
    let mut rows: Vec<(u64, String, String)> = doc
        .elements()
        .iter()
        .filter(|e| e.category != PROJECT_SETTINGS_CATEGORY)
        .map(|e| (e.id.0, crate::canon::element_hash(e), e.layer.clone()))
        .collect();
    rows.sort();
    value_hash(&json!({ "standard": doc.standard, "elements": rows }))
}

fn extents_of(doc: &Document) -> Option<Bounds> {
    doc.extents().map(|b| Bounds { min_x: b.min.x, min_y: b.min.y, max_x: b.max.x, max_y: b.max.y })
}

pub fn document_references(host: &Document) -> Vec<DocumentReference> {
    project_setting(host, DOCUMENT_REFERENCES_KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn store(host: &mut Document, list: &[DocumentReference]) {
    set_project_setting(host, DOCUMENT_REFERENCES_KEY, serde_json::to_value(list).expect("serialises"));
}

/// Attaches a whole document. Only the host's settings change; the document is read, not touched.
pub fn attach_document(host: &mut Document, root: &Path, name: &str, path: &Path, layers: &[String], locked: bool, by: &str, at: &str) -> Result<DocumentReference> {
    let mut list = document_references(host);
    if list.iter().any(|r| r.name.eq_ignore_ascii_case(name)) {
        return Err(ShortcutError::Invalid(format!("a document reference named '{name}' already exists")));
    }
    let file = if path.is_relative() { root.join(path) } else { path.to_path_buf() };
    let doc = Document::load(&file)?;
    let r = DocumentReference {
        name: name.to_string(),
        path: to_stored(root, &file),
        hash: document_hash(&doc),
        stamp: FileStamp::of(&file).ok(),
        layers: layers.to_vec(),
        locked,
        source_crs: project_crs(&doc),
        element_count: doc.elements().iter().filter(|e| e.category != PROJECT_SETTINGS_CATEGORY).count(),
        extents: extents_of(&doc),
        attached_by: by.into(),
        attached_at: at.into(),
    };
    list.push(r.clone());
    store(host, &list);
    Ok(r)
}

pub fn detach_document(host: &mut Document, name: &str) -> bool {
    let mut list = document_references(host);
    let n = list.len();
    list.retain(|r| r.name != name);
    let changed = list.len() != n;
    if changed {
        store(host, &list);
    }
    changed
}

/// Changes which layers are shown and the lock. An empty `layers` shows all.
pub fn set_document_view(host: &mut Document, name: &str, layers: &[String], locked: bool) -> Result<()> {
    let mut list = document_references(host);
    let r = list.iter_mut().find(|r| r.name == name).ok_or_else(|| ShortcutError::Invalid(format!("no document reference named '{name}'")))?;
    r.layers = layers.to_vec();
    r.locked = locked;
    store(host, &list);
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocState {
    UpToDate,
    OutOfDate { old_hash: String, new_hash: String },
    SourceMissing { what: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentStatus {
    pub name: String,
    pub state: DocState,
    pub crs_differs: bool,
    pub crs_messages: Vec<String>,
}

/// The status of every attached document. When the file's stamp equals the recorded one the
/// content is taken as unchanged without reading it (`use_stamp`); otherwise it is hashed.
pub fn document_status(host: &Document, root: &Path, use_stamp: bool) -> Vec<DocumentStatus> {
    let host_crs = project_crs(host);
    document_references(host)
        .into_iter()
        .map(|r| {
            let file = resolve(root, &r.path);
            let mut crs = (false, Vec::new());
            let state = if !file.exists() {
                DocState::SourceMissing { what: format!("the file {} does not exist", r.path) }
            } else if use_stamp && r.stamp.is_some() && FileStamp::of(&file).ok() == r.stamp {
                crs = crs_findings(&host_crs, &r.source_crs, &r.name);
                DocState::UpToDate
            } else {
                match Document::load(&file) {
                    Err(e) => DocState::SourceMissing { what: format!("{} cannot be read: {e}", r.path) },
                    Ok(doc) => {
                        crs = crs_findings(&host_crs, &project_crs(&doc), &r.name);
                        let h = document_hash(&doc);
                        if h == r.hash {
                            DocState::UpToDate
                        } else {
                            DocState::OutOfDate { old_hash: r.hash.clone(), new_hash: h }
                        }
                    }
                }
            };
            DocumentStatus { name: r.name, state, crs_differs: crs.0, crs_messages: crs.1 }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentRefresh {
    pub elements_before: usize,
    pub elements_after: usize,
    pub extents_before: Option<Bounds>,
    pub extents_after: Option<Bounds>,
    pub hash_changed: bool,
}

/// Records the document as it is now (hash, stamp, coordinate system, size). Reports what changed.
pub fn refresh_document(host: &mut Document, root: &Path, name: &str) -> Result<DocumentRefresh> {
    let mut list = document_references(host);
    let r = list.iter_mut().find(|r| r.name == name).ok_or_else(|| ShortcutError::Invalid(format!("no document reference named '{name}'")))?;
    let file = resolve(root, &r.path);
    let doc = Document::load(&file)?;
    let hash = document_hash(&doc);
    let out = DocumentRefresh {
        elements_before: r.element_count,
        elements_after: doc.elements().iter().filter(|e| e.category != PROJECT_SETTINGS_CATEGORY).count(),
        extents_before: r.extents,
        extents_after: extents_of(&doc),
        hash_changed: hash != r.hash,
    };
    r.hash = hash;
    r.stamp = FileStamp::of(&file).ok();
    r.source_crs = project_crs(&doc);
    r.element_count = out.elements_after;
    r.extents = out.extents_after;
    store(host, &list);
    Ok(out)
}

/// Reads the attached document for drawing: its elements at their own coordinates, restricted to
/// the layers the reference shows. The project settings element is left out. The file is not modified.
pub fn load_underlay(root: &Path, r: &DocumentReference) -> Result<Document> {
    let mut doc = Document::load(&resolve(root, &r.path))?;
    let keep: Vec<_> = doc.snapshot().into_iter().filter(|e| e.category != PROJECT_SETTINGS_CATEGORY && (r.layers.is_empty() || r.layers.contains(&e.layer))).collect();
    doc.restore(keep);
    Ok(doc)
}
