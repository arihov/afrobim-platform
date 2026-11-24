//! Safe file writing: a file is written whole or not at all.
//!
//! The content goes to a temporary file in the same folder as the target, is flushed and synced to
//! disk, and only then renamed over the target. A failure, a crash or a power cut before the
//! rename leaves the old file as it was; the temporary file is removed on a failure we can see.
//!
//! On Unix `rename` replaces an existing file in one step. On Windows `std::fs::rename` calls
//! `MoveFileExW` (with a fallback to `SetFileInformationByHandle`), which replaces an existing
//! target as well, so no extra dependency is needed. That path has not been run on Windows.
//!
//! A recovery file is the autosave copy of a document: `<name>.afrobim.recover` beside it.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// What is added to a document's file name to give its recovery file.
pub const RECOVER_SUFFIX: &str = ".recover";

/// Writes `bytes` to `path` atomically (see the module notes).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, |w| w.write_all(bytes))
}

/// Writes to `path` atomically, with the content produced by `fill`. If `fill` fails, nothing is
/// renamed: the old file stays whole and no temporary file is left behind.
pub fn write_atomic_with(path: &Path, fill: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
    // A symbolic link is followed, as a plain write would: the real file is the one replaced.
    let target = match fs::canonicalize(path) {
        Ok(real) => real,
        Err(_) => path.to_path_buf(),
    };
    let tmp = temp_name(&target)?;
    let result = write_and_swap(&target, &tmp, fill);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_swap(target: &Path, tmp: &Path, fill: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
    let mut file: File = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    fill(&mut file)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    // The new file keeps the permissions of the one it replaces.
    if let Ok(meta) = fs::metadata(target) {
        let _ = fs::set_permissions(tmp, meta.permissions());
    }
    fs::rename(tmp, target)?;
    sync_folder(target);
    Ok(())
}

/// Makes the rename itself durable. Best effort: folders cannot be opened for this on Windows.
#[cfg(unix)]
fn sync_folder(target: &Path) {
    if let Some(dir) = target.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_folder(_target: &Path) {}

/// A name for the temporary file: hidden, beside the target, unique to this process and call.
fn temp_name(target: &Path) -> io::Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = target.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"))?;
    let mut tmp_name = OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.{}{TEMP_SUFFIX}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    Ok(target.with_file_name(tmp_name))
}

/// The ending of a temporary file written by this module.
pub const TEMP_SUFFIX: &str = ".tmp";

/// The recovery file of a document file: its whole name with `.recover` added, so
/// `road.afrobim` gives `road.afrobim.recover`.
pub fn recover_path(document: &Path) -> PathBuf {
    let mut name = document.file_name().map(OsString::from).unwrap_or_else(|| OsString::from("untitled.afrobim"));
    name.push(RECOVER_SUFFIX);
    document.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("afrobim-atomic-{label}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn a_new_file_is_written_whole_with_no_temporary_left() {
        let dir = folder("new");
        let path = dir.join("a.afrobim");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert_eq!(names(&dir), vec!["a.afrobim"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_rename_replaces_a_file_that_is_already_there() {
        let dir = folder("replace");
        let path = dir.join("a.afrobim");
        fs::write(&path, b"old content that is longer").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(names(&dir), vec!["a.afrobim"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_write_that_fails_midway_leaves_the_old_file_intact_and_no_temporary() {
        let dir = folder("fail");
        let path = dir.join("a.afrobim");
        fs::write(&path, b"the good old file").unwrap();
        let err = write_atomic_with(&path, |w| {
            w.write_all(b"half of the new fi")?;
            Err(io::Error::other("disk full"))
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "disk full");
        assert_eq!(fs::read(&path).unwrap(), b"the good old file");
        assert_eq!(names(&dir), vec!["a.afrobim"], "the temporary file is removed");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failure_with_no_old_file_leaves_nothing_at_all() {
        let dir = folder("fail-new");
        let path = dir.join("a.afrobim");
        let _ = write_atomic_with(&path, |_| Err(io::Error::other("no")));
        assert!(names(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_is_an_error_and_not_a_panic() {
        let dir = folder("missing");
        assert!(write_atomic(&dir.join("no-such-folder").join("a.afrobim"), b"x").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_followed_and_stays_a_link() {
        let dir = folder("link");
        let real = dir.join("real.afrobim");
        let link = dir.join("link.afrobim");
        fs::write(&real, b"old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, b"new").unwrap();
        assert_eq!(fs::read(&real).unwrap(), b"new");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_permissions_of_the_old_file_are_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("perm");
        let path = dir.join("a.afrobim");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_recovery_file_adds_to_the_whole_name() {
        assert_eq!(recover_path(Path::new("/p/road.afrobim")), PathBuf::from("/p/road.afrobim.recover"));
        assert_eq!(recover_path(Path::new("road.afrobim")), PathBuf::from("road.afrobim.recover"));
        assert_eq!(recover_path(Path::new("/p/no extension")), PathBuf::from("/p/no extension.recover"));
    }
}
