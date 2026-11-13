//! Shared by the unit tests: the checkout's pack and fonts, and a temporary folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::settings::Settings;

/// The repository root: three levels above this crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate sits in <repo>/afrobim/crates")
        .to_path_buf()
}

/// The UK pack and the Transport fonts of the checkout; relative paths resolve against the
/// repository root.
pub fn settings() -> Settings {
    settings_with_fonts(&repo_root().join("fonts"))
}

pub fn settings_with_fonts(fonts: &Path) -> Settings {
    let root = repo_root();
    Settings::new(
        Ok(root.join("packs").join("UK-TSM-2019.json")),
        Ok(fonts.to_path_buf()),
        &root,
    )
}

/// A fresh folder under the system temporary directory, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("afrobim-mcp-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary folder can be created");
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
