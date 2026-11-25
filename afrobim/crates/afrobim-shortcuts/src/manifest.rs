// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The manifest: the one small file in a shortcut project folder.

use std::path::Path;

use afrobim_kernel::ProjectCrs;
use serde::{Deserialize, Serialize};

use crate::error::{Result, ShortcutError};

/// File name of the manifest inside the project folder.
pub const MANIFEST_FILE: &str = "afrobim-shortcuts.json";
pub const MANIFEST_FORMAT: &str = "afrobim-shortcuts";
pub const MANIFEST_VERSION: u32 = 1;

/// The state of a shared object, in the words of a common data environment. A plain label the
/// team agrees on; nothing in this crate changes behaviour with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShareStatus {
    WorkInProgress,
    Shared,
    Published,
    Superseded,
    Archived,
}

impl ShareStatus {
    pub fn label(self) -> &'static str {
        match self {
            ShareStatus::WorkInProgress => "Work in progress",
            ShareStatus::Shared => "Shared",
            ShareStatus::Published => "Published",
            ShareStatus::Superseded => "Superseded",
            ShareStatus::Archived => "Archived",
        }
    }
}

/// Size and modified time of a file, the cheap test for "might this file have changed". The
/// caller can supply stamps it already has (from a directory watcher, for example).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub size: u64,
    /// Modified time, milliseconds since 1970-01-01 UTC (0 when the system does not say).
    pub modified_ms: i64,
}

impl FileStamp {
    pub fn of(path: &Path) -> std::io::Result<FileStamp> {
        let m = std::fs::metadata(path)?;
        let modified_ms = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Ok(FileStamp { size: m.len(), modified_ms })
    }
}

/// One shared object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shortcut {
    /// Unique, stable name other documents refer to it by.
    pub key: String,
    /// The element's name in its source document.
    pub name: String,
    pub category: String,
    /// Source document, relative to the project folder with forward slashes, or absolute when
    /// the file is outside the folder.
    pub source_path: String,
    /// The element's id in the source document.
    pub source_element_id: u64,
    /// SHA-256 of the canonical content (see [`crate::element_hash`]) at the last publish.
    pub hash: String,
    /// Starts at 1; goes up by one each time a publish finds the content changed.
    pub revision: u64,
    /// Who and when are free text supplied by the caller. This crate reads no clock and no user name.
    pub published_by: String,
    pub published_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub status: ShareStatus,
    /// Stamp of the source file at the last publish (the cheap change test).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_stamp: Option<FileStamp>,
    /// The coordinate system of the source document at the last publish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_crs: Option<ProjectCrs>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub name: String,
    pub shortcuts: Vec<Shortcut>,
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        Manifest { format: MANIFEST_FORMAT.into(), version: MANIFEST_VERSION, name: name.to_string(), shortcuts: Vec::new() }
    }

    pub fn get(&self, key: &str) -> Option<&Shortcut> {
        self.shortcuts.iter().find(|s| s.key == key)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a manifest serialises")
    }

    pub fn from_json(text: &str) -> Result<Manifest> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| ShortcutError::Manifest(e.to_string()))?;
        if m.format != MANIFEST_FORMAT {
            return Err(ShortcutError::Manifest(format!("format is '{}', expected '{MANIFEST_FORMAT}'", m.format)));
        }
        if m.version > MANIFEST_VERSION {
            return Err(ShortcutError::Manifest(format!("file version {} is newer than this program reads ({MANIFEST_VERSION})", m.version)));
        }
        Ok(m)
    }
}
