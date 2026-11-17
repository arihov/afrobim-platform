//! Paths in a manifest. Stored with forward slashes on every system, relative to the project
//! folder where the file is inside it, absolute only when it is outside.

use std::path::{Component, Path, PathBuf};

fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn real(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| lexical(p))
}

fn slashes(p: &Path) -> String {
    let parts: Vec<String> = p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let mut s = parts.join("/");
    s = s.replace('\\', "/");
    // A root component joins as "/" + "/" + ...; collapse the doubled separator.
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    s
}

/// The text to store for `path`. A relative `path` is taken as relative to `root`.
pub fn to_stored(root: &Path, path: &Path) -> String {
    let abs = if path.is_relative() { root.join(path) } else { path.to_path_buf() };
    let (r, a) = (real(root), real(&abs));
    match a.strip_prefix(&r) {
        Ok(rel) if !rel.as_os_str().is_empty() => slashes(rel),
        _ => slashes(&a),
    }
}

fn looks_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/') || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/') || Path::new(s).is_absolute()
}

/// The file a stored path names on this machine.
pub fn resolve(root: &Path, stored: &str) -> PathBuf {
    if looks_absolute(stored) {
        return PathBuf::from(stored);
    }
    let mut p = root.to_path_buf();
    for part in stored.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

/// The stored form used to compare two paths: the same file always gives the same text.
pub fn normalised(root: &Path, stored: &str) -> String {
    to_stored(root, &resolve(root, stored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_is_relative_with_forward_slashes_and_outside_is_absolute() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a").join("b")).unwrap();
        let f = root.join("a").join("b").join("x.afrobim");
        std::fs::write(&f, "{}").unwrap();
        assert_eq!(to_stored(root, &f), "a/b/x.afrobim");
        assert_eq!(to_stored(root, Path::new("a/b/../b/x.afrobim")), "a/b/x.afrobim");
        let other = tempfile::tempdir().unwrap();
        let g = other.path().join("y.afrobim");
        std::fs::write(&g, "{}").unwrap();
        let s = to_stored(root, &g);
        assert!(!s.contains('\\') && looks_absolute(&s), "{s}");
        assert_eq!(resolve(root, "a/b/x.afrobim"), f);
        assert!(looks_absolute("C:/work/x.afrobim"));
        assert_eq!(normalised(root, "a/./b/x.afrobim"), "a/b/x.afrobim");
    }
}
