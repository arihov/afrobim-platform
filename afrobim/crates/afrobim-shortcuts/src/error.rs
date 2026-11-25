// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use afrobim_kernel::DocumentError;

/// Everything this crate can refuse to do. Each message is a plain sentence for the user.
#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Document(#[from] DocumentError),
    #[error("shortcut manifest: {0}")]
    Manifest(String),
    #[error("there is no shortcut with the key '{0}'")]
    NoShortcut(String),
    #[error("{0}")]
    Invalid(String),
    #[error("'{0}' is a read-only reference to another document. Edit it in its source document and synchronise, or promote it to an independent copy first.")]
    Locked(String),
}

pub type Result<T> = std::result::Result<T, ShortcutError>;
