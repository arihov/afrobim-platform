//! Resolves and checks every path a tool touches (port of C# `PathGuard`). Schedule reads are
//! limited to existing `.csv` files, `export_dxf` writes only directly inside the `out_dir` the
//! caller named, and the document tools only touch `.afrobim` and `.dxf` files.

use std::path::{Component, Path, PathBuf};

use crate::tools::ToolError;

const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `path` with `.` and `..` folded away, without touching the file system (so it works for a
/// file that does not exist yet). `..` at the root stays at the root.
pub fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A full path for `path`; a relative path resolves against `base_dir`.
pub fn resolve(path: &str, base_dir: &Path) -> Result<PathBuf, ToolError> {
    if path.trim().is_empty() {
        return Err(ToolError::new("path is empty"));
    }
    if path.contains('\0') {
        return Err(ToolError::new("path contains a NUL character"));
    }
    let p = Path::new(path);
    Ok(normalise(&if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }))
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// An existing `.csv` schedule file.
pub fn resolve_csv(path: &str, base_dir: &Path) -> Result<PathBuf, ToolError> {
    let full = resolve(path, base_dir)?;
    let shown = full.display();
    if !has_extension(&full, "csv") {
        return Err(ToolError::new(format!(
            "csv_path must name a .csv schedule file: {shown}"
        )));
    }
    if full.is_dir() {
        return Err(ToolError::new(format!(
            "csv_path is a folder, not a file: {shown}"
        )));
    }
    if !full.is_file() {
        return Err(ToolError::new(format!("schedule not found: {shown}")));
    }
    Ok(full)
}

/// A path that must carry the extension `ext` (no dot) and must not be a folder. `param` names
/// the argument in error messages. The file itself may or may not exist.
pub fn resolve_file(
    path: &str,
    base_dir: &Path,
    ext: &str,
    param: &str,
) -> Result<PathBuf, ToolError> {
    let full = resolve(path, base_dir)?;
    let shown = full.display();
    if !has_extension(&full, ext) {
        return Err(ToolError::new(format!(
            "{param} must name a .{ext} file: {shown}"
        )));
    }
    if full.is_dir() {
        return Err(ToolError::new(format!(
            "{param} is a folder, not a file: {shown}"
        )));
    }
    Ok(full)
}

/// Creates the folder a file is about to be written into.
pub fn prepare_parent(file: &Path) -> Result<(), ToolError> {
    match file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => std::fs::create_dir_all(dir)
            .map_err(|e| ToolError::new(format!("could not create folder {}: {e}", dir.display()))),
        _ => Ok(()),
    }
}

/// The output folder, created if needed. Refuses a path that is an existing file.
pub fn prepare_out_dir(path: &str, base_dir: &Path) -> Result<PathBuf, ToolError> {
    let full = resolve(path, base_dir)?;
    if full.is_file() {
        return Err(ToolError::new(format!(
            "out_dir is a file, not a folder: {}",
            full.display()
        )));
    }
    std::fs::create_dir_all(&full)
        .map_err(|e| ToolError::new(format!("could not create out_dir {}: {e}", full.display())))?;
    Ok(full)
}

/// The full path for `file_name` inside `out_dir`, after checking that it resolves to a file
/// directly in that folder and is not a link that would redirect the write elsewhere.
pub fn file_in_out_dir(out_dir: &Path, file_name: &str) -> Result<PathBuf, ToolError> {
    let full = normalise(&out_dir.join(file_name));
    if !is_directly_inside(out_dir, &full) {
        return Err(ToolError::new(format!(
            "refused: '{file_name}' resolves outside out_dir ({})",
            full.display()
        )));
    }
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        if meta.file_type().is_symlink() {
            return Err(ToolError::new(format!(
                "refused: {} is a link; writes must stay inside out_dir",
                full.display()
            )));
        }
        if meta.is_dir() {
            return Err(ToolError::new(format!(
                "refused: {} is a folder",
                full.display()
            )));
        }
    }
    Ok(full)
}

/// True when `candidate` is a direct child of `dir`.
pub fn is_directly_inside(dir: &Path, candidate: &Path) -> bool {
    let (dir, candidate) = (normalise(dir), normalise(candidate));
    candidate.file_name().is_some() && candidate.parent() == Some(dir.as_path())
}

/// A file name stem for a sign code: path separators and characters invalid on either Windows
/// or Linux become `_`, and a name that is empty, all dots or a Windows device name gets a
/// leading `_`.
pub fn safe_stem(code: &str) -> String {
    const INVALID: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let replaced: String = code
        .chars()
        .map(|c| {
            if INVALID.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = replaced.trim().trim_end_matches('.');
    if stem.is_empty() || stem.chars().all(|c| c == '.') {
        return format!("_{}", stem.replace('.', "_"));
    }
    let first = stem.split('.').next().unwrap_or("");
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(first))
    {
        return format!("_{stem}");
    }
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn normalise_folds_dots_without_touching_disk() {
        assert_eq!(
            normalise(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(normalise(Path::new("/../x")), PathBuf::from("/x"));
    }

    #[test]
    fn resolve_uses_the_base_for_relative_paths_only() {
        let base = Path::new("/base/dir");
        assert_eq!(
            resolve("out/x", base).unwrap(),
            PathBuf::from("/base/dir/out/x")
        );
        assert_eq!(resolve("/abs/x", base).unwrap(), PathBuf::from("/abs/x"));
        assert!(resolve("  ", base).is_err());
        assert!(resolve("a\0b", base).is_err());
    }

    #[test]
    fn safe_stem_never_leaves_the_folder() {
        let tmp = TempDir::new("stem");
        for code in [
            "../../evil",
            "..",
            ".",
            "",
            "a/b\\c",
            "CON",
            "C:x",
            "con.txt",
            "tab\there",
        ] {
            let name = format!("{}.dxf", safe_stem(code));
            let full = file_in_out_dir(tmp.path(), &name).unwrap_or_else(|e| panic!("{code}: {e}"));
            assert!(
                is_directly_inside(tmp.path(), &full),
                "{code} -> {}",
                full.display()
            );
        }
        assert_eq!(safe_stem("ADS-1"), "ADS-1");
        assert_eq!(safe_stem("CON"), "_CON");
        assert_eq!(safe_stem(".."), "_");
        assert_eq!(safe_stem("../../evil"), ".._.._evil");
    }

    #[test]
    fn file_in_out_dir_refuses_escapes_links_and_folders() {
        let tmp = TempDir::new("guard");
        let dir = tmp.path();
        assert!(file_in_out_dir(dir, "../x.dxf").is_err());
        assert!(file_in_out_dir(dir, "/tmp/x.dxf").is_err());
        assert!(file_in_out_dir(dir, "sub/x.dxf").is_err());
        assert!(file_in_out_dir(dir, "x.dxf").is_ok());

        std::fs::create_dir(dir.join("folder.dxf")).unwrap();
        assert!(file_in_out_dir(dir, "folder.dxf").is_err());

        #[cfg(unix)]
        {
            std::fs::write(dir.join("target.txt"), "x").unwrap();
            std::os::unix::fs::symlink(dir.join("target.txt"), dir.join("link.dxf")).unwrap();
            assert!(file_in_out_dir(dir, "link.dxf")
                .unwrap_err()
                .to_string()
                .contains("link"));
        }
    }

    #[test]
    fn csv_paths_must_be_existing_csv_files() {
        let tmp = TempDir::new("csv");
        let dir = tmp.path();
        std::fs::write(dir.join("schedule.txt"), "x").unwrap();
        std::fs::write(dir.join("ok.CSV"), "x").unwrap();
        std::fs::create_dir(dir.join("dir.csv")).unwrap();
        assert!(resolve_csv("schedule.txt", dir).is_err());
        assert!(resolve_csv("absent.csv", dir).is_err());
        assert!(resolve_csv("dir.csv", dir).is_err());
        assert_eq!(resolve_csv("ok.CSV", dir).unwrap(), dir.join("ok.CSV"));
    }

    #[test]
    fn out_dir_is_created_and_must_not_be_a_file() {
        let tmp = TempDir::new("outdir");
        let made = prepare_out_dir("a/b", tmp.path()).unwrap();
        assert!(made.is_dir());
        std::fs::write(tmp.path().join("file"), "x").unwrap();
        assert!(prepare_out_dir("file", tmp.path()).is_err());
    }
}
