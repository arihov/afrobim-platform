//! References in a host document: read-only copies of shared objects.
//!
//! A reference is an ordinary element of the source's own category, at the source's own
//! coordinates, with the source's `data`, so the host opens and draws without the source present
//! and the owning module draws it as it draws the original. What marks it as a reference is in
//! its `properties`: `shortcut.reference` holds a JSON [`ReferenceInfo`], `shortcut.locked` is
//! `"1"` while it is protected. Its layer is the source layer with a prefix (`REF-`).
//!
//! The one thing rewritten in the copy is a field that names another element by id (see
//! [`crate::DependencyTable`]): the id in the source document means nothing in the host, so it is
//! replaced with the id of the host's reference to that element. Names are copied as they are.

use std::collections::{BTreeMap, HashMap};

use afrobim_kernel::{project_crs, Document, Element, ElementId, ProjectCrs};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canon::{element_hash, value_hash};
use crate::deps::{key_closure, remap_ids, DependencyTable};
use crate::error::{Result, ShortcutError};
use crate::project::{DocCache, ShortcutProject};

pub const REFERENCE_PROP: &str = "shortcut.reference";
pub const LOCKED_PROP: &str = "shortcut.locked";
pub const PROMOTED_PROP: &str = "shortcut.promoted_from";

/// What a reference remembers about its source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceInfo {
    pub shortcut_key: String,
    /// Source document as the manifest stored it when the reference was last synchronised.
    pub source_path: String,
    pub source_element_id: u64,
    /// Content hash and revision of the source at the last synchronise.
    pub source_hash: String,
    pub source_revision: u64,
    pub source_name: String,
    pub source_category: String,
    pub source_layer: String,
    /// Coordinate system of the source document, as it declared it.
    #[serde(default)]
    pub source_crs: Option<ProjectCrs>,
    pub locked: bool,
    /// Free text from the caller: who synchronised, and when.
    pub synced_by: String,
    pub synced_at: String,
    /// Source element id to host element id, for every element this one depends on (the id fields
    /// among them were rewritten with it).
    #[serde(default)]
    pub id_map: BTreeMap<u64, u64>,
}

/// Where a promoted copy came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotedFrom {
    pub shortcut_key: String,
    pub source_path: String,
    pub source_element_id: u64,
    pub source_hash: String,
    pub source_revision: u64,
    pub promoted_by: String,
    pub promoted_at: String,
}

pub fn reference_info(e: &Element) -> Option<ReferenceInfo> {
    serde_json::from_str(e.properties.get(REFERENCE_PROP)?).ok()
}

pub fn is_reference(e: &Element) -> bool {
    reference_info(e).is_some()
}

pub fn promoted_from(e: &Element) -> Option<PromotedFrom> {
    serde_json::from_str(e.properties.get(PROMOTED_PROP)?).ok()
}

/// The check the application calls before it modifies, moves, erases or recomputes an element.
/// An element that is not there, or not a locked reference, passes.
pub fn check_editable(doc: &Document, id: ElementId) -> Result<()> {
    match doc.find(id).and_then(|e| reference_info(e).map(|i| (e, i))) {
        Some((e, info)) if info.locked => Err(ShortcutError::Locked(e.name.clone())),
        _ => Ok(()),
    }
}

/// Every id in `ids` that is a locked reference.
pub fn locked_among(doc: &Document, ids: &[ElementId]) -> Vec<ElementId> {
    ids.iter().copied().filter(|&id| check_editable(doc, id).is_err()).collect()
}

/// Locks or unlocks a reference. Unlocking only lets the application edit the copy; the next
/// synchronise overwrites such edits, so promote instead when the copy is to be edited.
pub fn set_locked(doc: &mut Document, id: ElementId, locked: bool) -> Result<()> {
    let e = doc.find_mut(id).ok_or_else(|| ShortcutError::Invalid("no such element".into()))?;
    let mut info = reference_info(e).ok_or_else(|| ShortcutError::Invalid(format!("'{}' is not a reference", e.name)))?;
    info.locked = locked;
    store_info(e, &info);
    Ok(())
}

pub(crate) fn store_info(e: &mut Element, info: &ReferenceInfo) {
    e.properties.insert(REFERENCE_PROP.into(), serde_json::to_string(info).expect("reference info serialises"));
    if info.locked {
        e.properties.insert(LOCKED_PROP.into(), "1".into());
    } else {
        e.properties.remove(LOCKED_PROP);
    }
}

/// The references of a host by shortcut key.
pub fn reference_index(host: &Document) -> HashMap<String, ElementId> {
    host.elements().iter().filter_map(|e| reference_info(e).map(|i| (i.shortcut_key, e.id))).collect()
}

#[derive(Debug, Clone)]
pub struct ReferenceOptions {
    pub table: DependencyTable,
    /// Property the application uses to notice that an element's data changed (the civil objects
    /// use `civil.rev`). Set on every reference to a value derived from its data, so a scene
    /// cache keyed on it is rebuilt after a synchronise. `None` to leave it alone.
    pub revision_property: Option<String>,
    /// Prefix of the host layer a reference is put on.
    pub layer_prefix: String,
    /// Properties of the source that are not copied (they are per-document bookkeeping).
    pub skip_properties: Vec<String>,
    /// Who and when, free text, recorded in the reference.
    pub by: String,
    pub at: String,
}

impl Default for ReferenceOptions {
    fn default() -> Self {
        ReferenceOptions {
            table: DependencyTable::civil(),
            revision_property: Some("civil.rev".into()),
            layer_prefix: "REF-".into(),
            skip_properties: vec!["civil.rev".into()],
            by: String::new(),
            at: String::new(),
        }
    }
}

/// The element as it is to appear in the host.
pub(crate) struct HostCopy {
    pub name: String,
    pub category: String,
    pub origin: afrobim_kernel::Pt,
    pub width: f64,
    pub height: f64,
    pub site: Option<afrobim_kernel::SitePlacement>,
    pub layer: String,
    pub properties: BTreeMap<String, String>,
    pub data: Value,
    pub id_map: BTreeMap<u64, u64>,
}

/// Builds the host's copy of `src`. `local_map` says which shortcut each source-local element id
/// is; `index` says which host element is the reference to each shortcut. Dependencies with no
/// host reference yet are returned by key.
pub(crate) fn host_copy(src: &Element, local_map: &BTreeMap<u64, String>, index: &HashMap<String, ElementId>, opts: &ReferenceOptions) -> (HostCopy, Vec<String>) {
    let mut id_map = BTreeMap::new();
    let mut missing = Vec::new();
    for (local, key) in local_map {
        match index.get(key) {
            Some(h) => {
                id_map.insert(*local, h.0);
            }
            None => missing.push(key.clone()),
        }
    }
    let mut data = src.data.clone();
    remap_ids(&mut data, &src.category, &opts.table, &id_map);
    let mut properties: BTreeMap<String, String> = src.properties.iter().filter(|(k, _)| !opts.skip_properties.contains(k) && !k.starts_with("shortcut.")).map(|(k, v)| (k.clone(), v.clone())).collect();
    if let Some(p) = &opts.revision_property {
        properties.insert(p.clone(), format!("sc-{}", &value_hash(&data)[..12]));
    }
    (
        HostCopy {
            name: src.name.clone(),
            category: src.category.clone(),
            origin: src.origin,
            width: src.width,
            height: src.height,
            site: src.site.clone(),
            layer: format!("{}{}", opts.layer_prefix, src.layer),
            properties,
            data,
            id_map,
        },
        missing,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreatedRef {
    pub key: String,
    pub element: ElementId,
    /// False when the host already had a reference to this shortcut and it was left as it is.
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateReport {
    /// The element for the shortcut that was asked for.
    pub root: ElementId,
    /// Every reference involved, dependencies first, the root last.
    pub items: Vec<CreatedRef>,
    /// Optional dependencies that were skipped, and similar.
    pub notes: Vec<String>,
    /// Coordinate system warnings: nothing is transformed, these only say what the user should look at.
    pub crs_messages: Vec<String>,
}

/// Adds a read-only reference to the shortcut `key` and to everything it depends on.
///
/// Refuses when the source has changed since it was published (publish it first), when a
/// dependency is not published, or when the host already has a non-reference element of the same
/// category and name. The source file is only read.
pub fn create_reference(host: &mut Document, project: &ShortcutProject, key: &str, opts: &ReferenceOptions) -> Result<CreateReport> {
    let mut cache = DocCache::new(project.root());
    let closure = key_closure(project, &mut cache, key, &opts.table);
    if !closure.problems.is_empty() {
        return Err(ShortcutError::Invalid(closure.problems.join("\n")));
    }
    let host_crs = project_crs(host);
    let mut index = reference_index(host);
    let mut report = CreateReport { root: ElementId(0), items: Vec::new(), notes: closure.notes.clone(), crs_messages: Vec::new() };
    for k in &closure.order {
        if let Some(id) = index.get(k) {
            report.items.push(CreatedRef { key: k.clone(), element: *id, created: false });
            continue;
        }
        let sc = project.get(k).expect("the closure only holds known keys").clone();
        let src = cache.get(&sc.source_path).map_err(ShortcutError::Invalid)?.find(ElementId(sc.source_element_id)).cloned().ok_or_else(|| ShortcutError::Invalid(format!("element {} is not in {}", sc.source_element_id, sc.source_path)))?;
        if element_hash(&src) != sc.hash {
            return Err(ShortcutError::Invalid(format!("'{}' has changed in {} since it was published. Publish it from its own project first; a reference only follows published revisions.", sc.name, sc.source_path)));
        }
        if host.elements().iter().any(|e| e.category == src.category && e.name == src.name && !is_reference(e)) {
            return Err(ShortcutError::Invalid(format!("the host already has its own {} named '{}'. Rename or remove it before referencing the shared one.", src.category, src.name)));
        }
        let empty = BTreeMap::new();
        let (copy, _missing) = host_copy(&src, closure.local_maps.get(k).unwrap_or(&empty), &index, opts);
        host.ensure_layer(&copy.layer);
        let id = host.add(&copy.category, &copy.name, copy.origin, copy.width, copy.height, &copy.layer, copy.data.clone());
        let info = ReferenceInfo {
            shortcut_key: k.clone(),
            source_path: sc.source_path.clone(),
            source_element_id: sc.source_element_id,
            source_hash: sc.hash.clone(),
            source_revision: sc.revision,
            source_name: src.name.clone(),
            source_category: src.category.clone(),
            source_layer: src.layer.clone(),
            source_crs: sc.source_crs.clone(),
            locked: true,
            synced_by: opts.by.clone(),
            synced_at: opts.at.clone(),
            id_map: copy.id_map.clone(),
        };
        let e = host.find_mut(id).expect("just added");
        e.site = copy.site;
        e.properties = copy.properties;
        store_info(e, &info);
        index.insert(k.clone(), id);
        report.items.push(CreatedRef { key: k.clone(), element: id, created: true });
        let (_, msgs) = crate::sync::crs_findings(&host_crs, &sc.source_crs, &sc.name);
        for m in msgs {
            if !report.crs_messages.contains(&m) {
                report.crs_messages.push(m);
            }
        }
    }
    report.root = report.items.last().map(|i| i.element).unwrap_or(ElementId(0));
    Ok(report)
}

/// Turns a reference into an independent, editable element. The data and placement stay as they
/// are; the element moves back to the source's layer name, loses its lock and its link to the
/// shortcut, and records where it came from in `shortcut.promoted_from`. Other references that
/// pointed at it by id keep pointing at it.
pub fn promote(host: &mut Document, id: ElementId, by: &str, at: &str) -> Result<PromotedFrom> {
    let e = host.find(id).ok_or_else(|| ShortcutError::Invalid("no such element".into()))?;
    let info = reference_info(e).ok_or_else(|| ShortcutError::Invalid(format!("'{}' is not a reference", e.name)))?;
    let from = PromotedFrom {
        shortcut_key: info.shortcut_key.clone(),
        source_path: info.source_path.clone(),
        source_element_id: info.source_element_id,
        source_hash: info.source_hash.clone(),
        source_revision: info.source_revision,
        promoted_by: by.into(),
        promoted_at: at.into(),
    };
    host.ensure_layer(&info.source_layer);
    let e = host.find_mut(id).expect("found above");
    e.properties.remove(REFERENCE_PROP);
    e.properties.remove(LOCKED_PROP);
    e.properties.insert(PROMOTED_PROP.into(), serde_json::to_string(&from).expect("serialises"));
    e.layer = info.source_layer;
    Ok(from)
}
