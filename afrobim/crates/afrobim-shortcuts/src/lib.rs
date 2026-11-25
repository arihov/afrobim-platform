// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Data shortcuts for AfroBIM: a road project split over several documents, each referencing named
//! objects from the others, read-only, and told when a source changed.
//!
//! See the README for the concepts, the file formats and the limits. In short:
//!
//! - [`ShortcutProject`] is a folder with a manifest ([`MANIFEST_FILE`]) listing shared objects.
//!   A document publishes elements into it; each publish records a content hash and a revision.
//! - [`create_reference`] puts a locked copy of a shared object (and what it depends on) into a
//!   host document, at the source's own coordinates. [`status`] says which are out of date,
//!   [`synchronise`] refreshes them and reports what changed, [`promote`] makes an editable
//!   copy, [`check_editable`] is what the application calls before modifying an element.
//! - [`attach_document`] attaches a whole document as an underlay, with the same status logic.
//! - [`stale`] and [`changed_sources`] are cheap checks that open no document.
//!
//! Nothing here modifies a source file, moves geometry or transforms coordinates. A difference in
//! coordinate system is reported through the kernel's `CrsCheck` and nothing else is done.

mod canon;
mod deps;
mod diff;
mod docref;
mod error;
mod manifest;
mod paths;
mod project;
mod reference;
mod stale;
mod sync;

pub use canon::{canonical_json, element_hash, file_hash, sha256_hex, value_hash};
pub use deps::{local_closure, remap_ids, values_at, DependencyRule, DependencyTable, TargetBy};
pub use diff::{json_diff, summarise, Bounds, ChangeKind, ChangeSummary, FieldChange, MAX_FIELDS};
pub use docref::{
    attach_document, detach_document, document_hash, document_references, document_status, load_underlay, refresh_document, set_document_view, DocState, DocumentRefresh, DocumentReference, DocumentStatus,
    DOCUMENT_REFERENCES_KEY,
};
pub use error::{Result, ShortcutError};
pub use manifest::{FileStamp, Manifest, ShareStatus, Shortcut, MANIFEST_FILE, MANIFEST_FORMAT, MANIFEST_VERSION};
pub use paths::{normalised, resolve, to_stored};
pub use project::{Issue, IssueKind, Level, PublishAction, PublishOutcome, PublishRequest, RepairReport, ShortcutProject};
pub use reference::{
    check_editable, create_reference, is_reference, locked_among, promote, promoted_from, reference_index, reference_info, set_locked, CreateReport, CreatedRef, PromotedFrom, ReferenceInfo, ReferenceOptions,
    LOCKED_PROP, PROMOTED_PROP, REFERENCE_PROP,
};
pub use stale::{changed_sources, recorded_references, stale, RecordedRef, StaleRef};
pub use sync::{status, synchronise, RefState, ReferenceStatus, StatusOptions, SyncItem, SyncOutcome, SyncReport, SyncTarget};
