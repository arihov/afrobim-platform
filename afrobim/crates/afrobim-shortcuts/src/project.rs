//! The shortcut project: a folder with a manifest, and the operations on it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use afrobim_kernel::{project_crs, Document, ElementId, PROJECT_SETTINGS_CATEGORY};
use serde::{Deserialize, Serialize};

use crate::canon::element_hash;
use crate::deps::{local_closure, DependencyTable};
use crate::error::{Result, ShortcutError};
use crate::manifest::{FileStamp, Manifest, ShareStatus, Shortcut, MANIFEST_FILE};
use crate::paths::{normalised, resolve, to_stored};
use crate::reference::reference_info;

/// Loads each source document at most once per operation.
pub(crate) struct DocCache {
    root: PathBuf,
    docs: HashMap<String, std::result::Result<Document, String>>,
}

impl DocCache {
    pub(crate) fn new(root: &Path) -> DocCache {
        DocCache { root: root.to_path_buf(), docs: HashMap::new() }
    }

    pub(crate) fn get(&mut self, stored: &str) -> std::result::Result<&Document, String> {
        let root = self.root.clone();
        let entry = self.docs.entry(stored.to_string()).or_insert_with(|| {
            let p = resolve(&root, stored);
            if !p.exists() {
                return Err(format!("the file {} does not exist", p.display()));
            }
            Document::load(&p).map_err(|e| e.to_string())
        });
        entry.as_ref().map_err(|e| e.clone())
    }
}

/// What to publish and under whose name.
#[derive(Debug, Clone)]
pub struct PublishRequest {
    /// The source document: absolute, or relative to the project folder.
    pub source: PathBuf,
    pub element: ElementId,
    /// The key to publish under. Default: `category:name`.
    pub key: Option<String>,
    /// Free text: who is publishing and when. This crate reads no clock.
    pub by: String,
    pub at: String,
    pub note: Option<String>,
    /// For a new shortcut the default is work in progress; for an existing one it is unchanged.
    pub status: Option<ShareStatus>,
}

impl PublishRequest {
    pub fn new(source: impl Into<PathBuf>, element: ElementId, by: &str, at: &str) -> PublishRequest {
        PublishRequest { source: source.into(), element, key: None, by: by.into(), at: at.into(), note: None, status: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PublishAction {
    Created,
    /// The content changed: the revision went up.
    Updated { old_revision: u64 },
    /// The content is the same as the last publish; nothing but the file stamp was refreshed.
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishOutcome {
    pub key: String,
    pub action: PublishAction,
    pub revision: u64,
    pub hash: String,
    /// Set when the shortcut used to point at a different document or element: last publisher wins.
    pub previous_source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Level {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueKind {
    MissingFile,
    UnreadableFile,
    MissingElement,
    CategoryMismatch,
    HashMismatch,
    DuplicateKey,
    DuplicateName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub level: Level,
    pub kind: IssueKind,
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RepairReport {
    /// Keys whose source path now points at the new file.
    pub repaired: Vec<String>,
    /// Keys whose element in the new file has different content from the published hash. The
    /// path is repaired; the hash is not touched, so validation and references still say so.
    pub content_differs: Vec<String>,
}

pub struct ShortcutProject {
    root: PathBuf,
    manifest: Manifest,
}

impl ShortcutProject {
    /// Creates the folder (and its parents) and an empty manifest. Refuses to overwrite one.
    pub fn create(root: &Path, name: &str) -> Result<ShortcutProject> {
        std::fs::create_dir_all(root)?;
        if root.join(MANIFEST_FILE).exists() {
            return Err(ShortcutError::Invalid(format!("{} already holds a shortcut project", root.display())));
        }
        let p = ShortcutProject { root: root.to_path_buf(), manifest: Manifest::new(name) };
        p.save()?;
        Ok(p)
    }

    pub fn open(root: &Path) -> Result<ShortcutProject> {
        let text = std::fs::read_to_string(root.join(MANIFEST_FILE))?;
        Ok(ShortcutProject { root: root.to_path_buf(), manifest: Manifest::from_json(&text)? })
    }

    /// Writes the manifest (to a temporary file first, then renamed over the old one).
    pub fn save(&self) -> Result<()> {
        let tmp = self.root.join(format!("{MANIFEST_FILE}.tmp"));
        std::fs::write(&tmp, self.manifest.to_json())?;
        std::fs::rename(&tmp, self.root.join(MANIFEST_FILE))?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn list(&self) -> &[Shortcut] {
        &self.manifest.shortcuts
    }

    pub fn get(&self, key: &str) -> Option<&Shortcut> {
        self.manifest.get(key)
    }

    /// The shortcut for an element of a source document, if it is published.
    pub fn find_by_source(&self, stored_path: &str, element: u64) -> Option<&Shortcut> {
        let want = normalised(&self.root, stored_path);
        self.manifest.shortcuts.iter().find(|s| s.source_element_id == element && normalised(&self.root, &s.source_path) == want)
    }

    pub fn source_file(&self, key: &str) -> Option<PathBuf> {
        self.get(key).map(|s| resolve(&self.root, &s.source_path))
    }

    /// Creates or updates a shortcut from an element of a document, and saves the manifest.
    ///
    /// The source file is only read. A reference element cannot be published: publish the
    /// original. Last publisher wins: publishing under an existing key replaces what the
    /// shortcut points at, and the outcome says what it used to point at.
    pub fn publish(&mut self, req: &PublishRequest) -> Result<PublishOutcome> {
        let file = if req.source.is_relative() { self.root.join(&req.source) } else { req.source.clone() };
        let doc = Document::load(&file)?;
        let el = doc.find(req.element).ok_or_else(|| ShortcutError::Invalid(format!("element {} is not in {}", req.element.0, file.display())))?;
        if el.category == PROJECT_SETTINGS_CATEGORY {
            return Err(ShortcutError::Invalid("the project settings cannot be shared".into()));
        }
        if reference_info(el).is_some() {
            return Err(ShortcutError::Invalid(format!("'{}' is a reference to another document. Publish the original from its own document.", el.name)));
        }
        let key = req.key.clone().unwrap_or_else(|| format!("{}:{}", el.category, el.name));
        if key.trim().is_empty() {
            return Err(ShortcutError::Invalid("a shortcut key cannot be empty".into()));
        }
        if let Some(other) = self.manifest.shortcuts.iter().find(|s| s.key != key && s.category == el.category && s.name.eq_ignore_ascii_case(&el.name)) {
            return Err(ShortcutError::Invalid(format!("'{}' ({}) is already shared under the key '{}'. Names must be unique within a category.", el.name, el.category, other.key)));
        }
        let stored = to_stored(&self.root, &file);
        let hash = element_hash(el);
        let stamp = FileStamp::of(&file).ok();
        let crs = project_crs(&doc);
        let existing = self.manifest.shortcuts.iter().position(|s| s.key == key);
        let outcome = match existing {
            None => {
                self.manifest.shortcuts.push(Shortcut {
                    key: key.clone(),
                    name: el.name.clone(),
                    category: el.category.clone(),
                    source_path: stored,
                    source_element_id: el.id.0,
                    hash: hash.clone(),
                    revision: 1,
                    published_by: req.by.clone(),
                    published_at: req.at.clone(),
                    note: req.note.clone(),
                    status: req.status.unwrap_or(ShareStatus::WorkInProgress),
                    file_stamp: stamp,
                    source_crs: crs,
                });
                PublishOutcome { key, action: PublishAction::Created, revision: 1, hash, previous_source: None }
            }
            Some(i) => {
                let s = &mut self.manifest.shortcuts[i];
                let moved = normalised(&self.root, &s.source_path) != normalised(&self.root, &stored) || s.source_element_id != el.id.0;
                let previous_source = moved.then(|| format!("{}#{}", s.source_path, s.source_element_id));
                s.file_stamp = stamp;
                s.source_crs = crs;
                if s.hash == hash && !moved {
                    if let Some(st) = req.status {
                        s.status = st;
                    }
                    PublishOutcome { key, action: PublishAction::Unchanged, revision: s.revision, hash, previous_source }
                } else {
                    let old_revision = s.revision;
                    s.revision += 1;
                    s.hash = hash.clone();
                    s.name = el.name.clone();
                    s.category = el.category.clone();
                    s.source_path = stored;
                    s.source_element_id = el.id.0;
                    s.published_by = req.by.clone();
                    s.published_at = req.at.clone();
                    if req.note.is_some() {
                        s.note = req.note.clone();
                    }
                    if let Some(st) = req.status {
                        s.status = st;
                    }
                    PublishOutcome { key, action: PublishAction::Updated { old_revision }, revision: s.revision, hash, previous_source }
                }
            }
        };
        self.save()?;
        Ok(outcome)
    }

    /// Publishes an element and every element of the same document it depends on (see
    /// [`DependencyTable`]), dependencies first.
    pub fn publish_closure(&mut self, req: &PublishRequest, table: &DependencyTable) -> Result<Vec<PublishOutcome>> {
        let file = if req.source.is_relative() { self.root.join(&req.source) } else { req.source.clone() };
        let doc = Document::load(&file)?;
        let mut out = Vec::new();
        for id in local_closure(&doc, req.element, table) {
            let mut r = req.clone();
            r.element = id;
            if id != req.element {
                r.key = None;
            }
            out.push(self.publish(&r)?);
        }
        Ok(out)
    }

    /// Removes a shortcut. Documents that reference it report "shortcut removed"; their copies stay.
    pub fn remove(&mut self, key: &str) -> Result<Shortcut> {
        let i = self.manifest.shortcuts.iter().position(|s| s.key == key).ok_or_else(|| ShortcutError::NoShortcut(key.into()))?;
        let s = self.manifest.shortcuts.remove(i);
        self.save()?;
        Ok(s)
    }

    /// Changes the status label (and who and when). The revision does not change: the content did not.
    pub fn set_status(&mut self, key: &str, status: ShareStatus, by: &str, at: &str) -> Result<()> {
        let s = self.manifest.shortcuts.iter_mut().find(|s| s.key == key).ok_or_else(|| ShortcutError::NoShortcut(key.into()))?;
        s.status = status;
        s.published_by = by.into();
        s.published_at = at.into();
        self.save()
    }

    /// A source document moved: points every shortcut that used `old_path` at `new_path`.
    ///
    /// The new file must hold each element under the same id and category, else nothing changes.
    /// Hashes are left alone; `content_differs` in the report names shortcuts whose element is not
    /// what was published.
    pub fn repair_source_path(&mut self, old_path: &str, new_path: &Path) -> Result<RepairReport> {
        let want = normalised(&self.root, old_path);
        let new_file = if new_path.is_relative() { self.root.join(new_path) } else { new_path.to_path_buf() };
        let doc = Document::load(&new_file)?;
        let idx: Vec<usize> = (0..self.manifest.shortcuts.len()).filter(|&i| normalised(&self.root, &self.manifest.shortcuts[i].source_path) == want).collect();
        if idx.is_empty() {
            return Err(ShortcutError::Invalid(format!("no shortcut uses the source path '{old_path}'")));
        }
        let mut report = RepairReport::default();
        for &i in &idx {
            let s = &self.manifest.shortcuts[i];
            let el = doc.find(ElementId(s.source_element_id)).filter(|e| e.category == s.category).ok_or_else(|| {
                ShortcutError::Invalid(format!("{} has no {} element with id {} (needed by '{}'); nothing was changed", new_file.display(), s.category, s.source_element_id, s.key))
            })?;
            if element_hash(el) != s.hash {
                report.content_differs.push(s.key.clone());
            }
        }
        let stored = to_stored(&self.root, &new_file);
        let stamp = FileStamp::of(&new_file).ok();
        for &i in &idx {
            let s = &mut self.manifest.shortcuts[i];
            s.source_path = stored.clone();
            s.file_stamp = stamp;
            report.repaired.push(s.key.clone());
        }
        self.save()?;
        Ok(report)
    }

    /// Checks the manifest against the files: missing files and elements, category changes,
    /// content that no longer matches the published hash, duplicate keys and names.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut cache = DocCache::new(&self.root);
        for (i, s) in self.manifest.shortcuts.iter().enumerate() {
            let mut push = |level, kind, message: String| issues.push(Issue { level, kind, key: s.key.clone(), message });
            if self.manifest.shortcuts[..i].iter().any(|o| o.key == s.key) {
                push(Level::Error, IssueKind::DuplicateKey, format!("the key '{}' is used more than once", s.key));
            }
            if self.manifest.shortcuts[..i].iter().any(|o| o.key != s.key && o.category == s.category && o.name.eq_ignore_ascii_case(&s.name)) {
                push(Level::Warning, IssueKind::DuplicateName, format!("'{}' ({}) is shared under more than one key; references by name are ambiguous", s.name, s.category));
            }
            let path = resolve(&self.root, &s.source_path);
            if !path.exists() {
                push(Level::Error, IssueKind::MissingFile, format!("the source file {} does not exist (moved? use repair_source_path)", path.display()));
                continue;
            }
            match cache.get(&s.source_path) {
                Err(e) => push(Level::Error, IssueKind::UnreadableFile, format!("{} cannot be read: {e}", s.source_path)),
                Ok(doc) => match doc.find(ElementId(s.source_element_id)) {
                    None => push(Level::Error, IssueKind::MissingElement, format!("element {} is not in {}", s.source_element_id, s.source_path)),
                    Some(el) => {
                        if el.category != s.category {
                            push(Level::Error, IssueKind::CategoryMismatch, format!("element {} in {} is now a {}, published as a {}", s.source_element_id, s.source_path, el.category, s.category));
                        }
                        if element_hash(el) != s.hash {
                            push(Level::Warning, IssueKind::HashMismatch, format!("'{}' in {} has changed since it was published (revision {}); publish it again to share the change", s.name, s.source_path, s.revision));
                        }
                    }
                },
            }
        }
        issues
    }
}
